use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{timeout, Duration};
use uuid::Uuid;

use crate::config::ServerConfig;
use crate::logging::{format_service_config, format_uuid};
use crate::utils::crypto::{sha256_with_salt, MAGIC_SALT};
use crate::utils::{CryptoContext, Frame, FrameReader, Message};
use crate::{console_info, debug, error, info, log_debug, log_info, warn};

/// Main server structure that handles client connections and proxy management
pub struct Server {
    config: ServerConfig,
    clients: Arc<RwLock<HashMap<String, ClientConnection>>>,
    proxy_listeners: Arc<RwLock<HashMap<u16, ProxyListenerInfo>>>,
    proxy_connections: Arc<RwLock<HashMap<String, ProxyConnectionInfo>>>,
}

/// Represents a connected client with its communication channel and proxy configurations
#[derive(Clone)]
struct ClientConnection {
    client_id: String,
    sender: mpsc::UnboundedSender<Message>,
    crypto: Arc<CryptoContext>,
    proxies: HashMap<String, ProxyInfo>,
}

/// Configuration information for a proxy service
#[derive(Clone)]
struct ProxyInfo {
    local_ip: String,
    local_port: u16,
    remote_port: u16,
}

/// Information about an active proxy connection for data forwarding
struct ProxyConnectionInfo {
    sender: mpsc::UnboundedSender<Vec<u8>>,
    client_id: String,
}

/// Information about a proxy listener bound to a specific port
struct ProxyListenerInfo {
    listener: Arc<TcpListener>,
    client_id: String,
    proxy_id: String,
    cancel_tx: mpsc::UnboundedSender<()>,
}

impl Server {
    /// Creates a new server instance with the given configuration
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            proxy_listeners: Arc::new(RwLock::new(HashMap::new())),
            proxy_connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Starts the server and begins accepting client connections
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.listen_addr).await?;
        log_info!("Server ready, listening on {}", self.config.listen_addr);

        // listen for client to connect
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let server = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = server.handle_client(stream, addr).await {
                            error!("Error handling client {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handles a single client connection through its entire lifecycle
    async fn handle_client(&self, mut stream: TcpStream, addr: SocketAddr) -> Result<()> {
        log_debug!("New client connection from {}", addr);

        // Read authentication message
        let mut frame_reader = FrameReader::new();
        let mut buffer = [0u8; 4096];

        // take 30s to receive buffer data
        let n = timeout(Duration::from_secs(30), stream.read(&mut buffer)).await??;
        if n == 0 {
            return Err(anyhow::anyhow!("Connection closed during auth"));
        }

        frame_reader.feed_data(&buffer[..n]);

        let frame = match frame_reader.try_read_frame()? {
            Some(frame) => frame,
            None => return Err(anyhow::anyhow!("Incomplete auth frame")),
        };

        // --- Parse authentication ---

        let (client_id, crypto) = match frame.message {
            Message::Auth { enc_token, client_id, name: client_name } => {
                if enc_token != sha256_with_salt(self.config.token.as_bytes(), MAGIC_SALT) {
                    let response = Message::AuthResponse {
                        success: false,
                        session_key: None,
                        name: self.config.name.clone(),
                        error: Some("Invalid token".to_string()),
                    };
                    let response_frame = Frame::new(response);
                    stream.write_all(&response_frame.serialize()?).await?;
                    return Err(anyhow::anyhow!("Authentication failed for {}", addr));
                }

                // Derive session key
                let session_key = CryptoContext::derive_session_key(&self.config.token, &client_id)?;
                let crypto = Arc::new(CryptoContext::new(&session_key)?);

                // Send success response
                let response = Message::AuthResponse {
                    success: true,
                    session_key: Some(session_key.clone()),
                    name: self.config.name.clone(),
                    error: None,
                };
                let response_frame = Frame::new(response);
                stream.write_all(&response_frame.serialize()?).await?;

                log_info!("Client {} authenticated successfully", client_id);
                // console_info!("Client {} authenticated", format_uuid(&client_id, "client")); TODO:
                (client_id, crypto)
            }
            _ => return Err(anyhow::anyhow!("Expected auth message")),
        };

        // --- Create client connection ---

        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        let client_conn = ClientConnection {
            client_id: client_id.clone(),
            sender: tx,
            crypto: crypto.clone(),
            proxies: HashMap::new(),
        };

        {
            let mut clients_guard = self.clients.write().await;
            // if the client_id has been created in the pool, reject
            if clients_guard.contains_key(&client_id) {
                return Err(anyhow::anyhow!("Client ID {} already exists", client_id));
            }
            clients_guard.insert(client_id.clone(), client_conn);
        }

        // Handle incoming messages from client
        let clients = self.clients.clone();
        let proxy_listeners = self.proxy_listeners.clone();
        let proxy_connections = self.proxy_connections.clone();
        let client_id_clone = client_id.clone();
        let bind_host = self.config.bind_host.clone();

        let (mut stream_read, mut stream_write) = stream.into_split();

        let read_task = {
            let clients = clients.clone();
            let client_id = client_id.clone();
            let proxy_listeners = proxy_listeners.clone();
            let proxy_connections = proxy_connections.clone();
            let client_id_for_cleanup = client_id.clone();

            tokio::spawn(async move {
                let mut frame_reader = FrameReader::new();
                let mut buffer = [0u8; 4096];

                loop {
                    match stream_read.read(&mut buffer).await {
                        Ok(0) => break,
                        Ok(n) => {
                            frame_reader.feed_data(&buffer[..n]);

                            while let Some(frame) = frame_reader.try_read_frame().unwrap_or(None) {
                                match Self::handle_client_message(
                                    frame.message,
                                    &client_id,
                                    &clients,
                                    &proxy_listeners,
                                    &proxy_connections,
                                    &bind_host,
                                )
                                .await
                                {
                                    Ok(_) => {}
                                    Err(e) => {
                                        error!("Error handling client message: {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error reading from client {}: {}", client_id, e);
                            break;
                        }
                    }
                }

                // Immediately clean up when connection is lost
                Self::cleanup_client(
                    &client_id_for_cleanup,
                    &clients,
                    &proxy_listeners,
                    &proxy_connections,
                )
                .await;
                log_info!(
                    "Client {} disconnected",
                    format_uuid(&client_id_for_cleanup, "client")
                );
            })
        };

        // Handle outgoing messages to client
        let write_task = {
            tokio::spawn(async move {
                while let Some(message) = rx.recv().await {
                    let frame = Frame::new(message);
                    match frame.serialize() {
                        Ok(data) => {
                            if let Err(e) = stream_write.write_all(&data).await {
                                error!("Error writing to client: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Error serializing message: {}", e);
                            break;
                        }
                    }
                }
            })
        };

        // Wait for either task to complete
        tokio::select! {
            _ = read_task => {},
            _ = write_task => {},
        }

        // Additional cleanup in case read_task didn't handle it
        Self::cleanup_client(
            &client_id_clone,
            &self.clients,
            &self.proxy_listeners,
            &self.proxy_connections,
        )
        .await;
        log_info!(
            "Client {} connection closed",
            format_uuid(&client_id_clone, "client")
        );

        Ok(())
    }

    /// Clean up all resources associated with a client
    async fn cleanup_client(
        client_id: &str,
        clients: &Arc<RwLock<HashMap<String, ClientConnection>>>,
        proxy_listeners: &Arc<RwLock<HashMap<u16, ProxyListenerInfo>>>,
        proxy_connections: &Arc<RwLock<HashMap<String, ProxyConnectionInfo>>>,
    ) {
        // Remove client first
        let client_removed = {
            let mut clients_guard = clients.write().await;
            clients_guard.remove(client_id).is_some()
        };

        if !client_removed {
            return; // Already cleaned up
        }

        // Clean up proxy listeners for this client
        let mut proxy_listeners_guard = proxy_listeners.write().await;
        let mut listeners_to_remove = Vec::new();

        for (port, listener_info) in proxy_listeners_guard.iter() {
            if listener_info.client_id == client_id {
                listeners_to_remove.push(*port);
                // Send cancel signal to stop the listener
                let _ = listener_info.cancel_tx.send(());
            }
        }

        for port in listeners_to_remove {
            proxy_listeners_guard.remove(&port);
            log_info!(
                "Cleaned up service listener on port {} for client {}",
                port,
                format_uuid(client_id, "client")
            );
        }
        drop(proxy_listeners_guard);

        // Clean up any active proxy connections for this client
        let mut proxy_connections_guard = proxy_connections.write().await;
        let mut connections_to_remove = Vec::new();

        // Find all connections belonging to this client
        for (connection_id, connection_info) in proxy_connections_guard.iter() {
            if connection_info.client_id == client_id {
                connections_to_remove.push(connection_id.clone());
            }
        }

        for connection_id in connections_to_remove {
            if let Some(_) = proxy_connections_guard.remove(&connection_id) {
                log_info!(
                    "Cleaned up proxy connection {} for client {}",
                    connection_id,
                    client_id
                );
                console_info!(
                    "Cleaned up connection {} for client {}",
                    format_uuid(&connection_id, "conn"),
                    format_uuid(client_id, "client")
                );
            }
        }
    }

    /// Processes messages received from a client
    async fn handle_client_message(
        message: Message,
        client_id: &str,
        clients: &Arc<RwLock<HashMap<String, ClientConnection>>>,
        proxy_listeners: &Arc<RwLock<HashMap<u16, ProxyListenerInfo>>>,
        proxy_connections: &Arc<RwLock<HashMap<String, ProxyConnectionInfo>>>,
        bind_host: &str,
    ) -> Result<()> {
        match message {
            Message::Data {
                connection_id,
                data,
            } => {
                // Forward data to proxy connection
                debug!(
                    "Received {} bytes from client for connection {}",
                    data.len(),
                    connection_id
                );

                let proxy_connections_guard = proxy_connections.read().await;
                if let Some(proxy_conn) = proxy_connections_guard.get(&connection_id) {
                    if let Err(e) = proxy_conn.sender.send(data) {
                        error!("Failed to forward data to proxy connection: {}", e);
                    }
                } else {
                    warn!("Proxy connection {} not found", connection_id);
                }
            }
            Message::ProxyConfig {
                local_ip,
                local_port,
                remote_port,
            } => {
                log_info!(
                    "Setting up proxy for client {}: {}:{} -> :{}",
                    client_id,
                    local_ip,
                    local_port,
                    remote_port
                );
                console_info!(
                    "Setting up service for client {}: {}",
                    format_uuid(client_id, "client"),
                    format_service_config(&local_ip, local_port, remote_port)
                );

                // Update client proxy info
                let proxy_id = Uuid::new_v4().to_string();
                let proxy_info = ProxyInfo {
                    local_ip: local_ip.clone(),
                    local_port,
                    remote_port,
                };

                {
                    let mut clients_guard = clients.write().await;
                    if let Some(client) = clients_guard.get_mut(client_id) {
                        client.proxies.insert(proxy_id.clone(), proxy_info);

                        // Send response
                        let response = Message::ProxyConfigResponse {
                            success: true,
                            proxy_id: Some(proxy_id.clone()),
                            error: None,
                        };
                        if let Err(e) = client.sender.send(response) {
                            error!("Failed to send proxy config response: {}", e);
                        }
                    }
                }

                // Start proxy listener if not already listening on this port
                let mut listeners = proxy_listeners.write().await;
                if !listeners.contains_key(&remote_port) {
                    let listen_addr = format!("{}:{}", bind_host, remote_port);
                    match TcpListener::bind(&listen_addr).await {
                        Ok(listener) => {
                            let listener = Arc::new(listener);

                            // Create cancel channel for this listener
                            let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();

                            let listener_info = ProxyListenerInfo {
                                listener: listener.clone(),
                                client_id: client_id.to_string(),
                                proxy_id: proxy_id.clone(),
                                cancel_tx,
                            };

                            listeners.insert(remote_port, listener_info);

                            // Start accepting connections for this proxy
                            let clients_clone = clients.clone();
                            let proxy_connections_clone = proxy_connections.clone();
                            let client_id_clone = client_id.to_string();
                            let proxy_id_clone = proxy_id.clone();

                            tokio::spawn(async move {
                                Self::handle_proxy_connections(
                                    listener,
                                    clients_clone,
                                    proxy_connections_clone,
                                    client_id_clone,
                                    proxy_id_clone,
                                    cancel_rx,
                                )
                                .await;
                            });

                            log_info!("Proxy listener started on {}", listen_addr);
                        }
                        Err(e) => {
                            error!(
                                "Failed to start proxy listener on port {}: {}",
                                remote_port, e
                            );

                            // Send error response
                            let clients_guard = clients.read().await;
                            if let Some(client) = clients_guard.get(client_id) {
                                let response = Message::ProxyConfigResponse {
                                    success: false,
                                    proxy_id: None,
                                    error: Some(format!(
                                        "Failed to bind port {}: {}",
                                        remote_port, e
                                    )),
                                };
                                let _ = client.sender.send(response);
                            }
                        }
                    }
                } else {
                    // Port already in use, check if it's by the same client
                    if let Some(existing_listener) = listeners.get(&remote_port) {
                        if existing_listener.client_id != client_id {
                            let clients_guard = clients.read().await;
                            if let Some(client) = clients_guard.get(client_id) {
                                let response = Message::ProxyConfigResponse {
                                    success: false,
                                    proxy_id: None,
                                    error: Some(format!(
                                        "Port {} already in use by another client",
                                        remote_port
                                    )),
                                };
                                let _ = client.sender.send(response);
                            }
                        }
                    }
                }
            }
            Message::Heartbeat { timestamp } => {
                debug!("Heartbeat from client {}: {}", client_id, timestamp);

                let clients_guard = clients.read().await;
                if let Some(client) = clients_guard.get(client_id) {
                    let response = Message::HeartbeatResponse { timestamp };
                    let _ = client.sender.send(response);
                }
            }
            _ => {
                warn!(
                    "Unexpected message from client {}: {:?}",
                    client_id, message
                );
            }
        }

        Ok(())
    }

    /// Handles incoming connections to a proxy port and forwards them to the appropriate client
    async fn handle_proxy_connections(
        listener: Arc<TcpListener>,
        clients: Arc<RwLock<HashMap<String, ClientConnection>>>,
        proxy_connections: Arc<RwLock<HashMap<String, ProxyConnectionInfo>>>,
        client_id: String,
        proxy_id: String,
        mut cancel_rx: mpsc::UnboundedReceiver<()>,
    ) {
        loop {
            tokio::select! {
                // Check for cancellation
                _ = cancel_rx.recv() => {
                    log_info!("Proxy listener for client {} cancelled", client_id);
                    break;
                }
                // Accept new connections
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            debug!("New proxy connection from {} for client {}", addr, client_id);

                            // Check if client still exists
                            let client_exists = {
                                let clients_guard = clients.read().await;
                                clients_guard.contains_key(&client_id)
                            };

                            if !client_exists {
                                log_info!("Client {} no longer exists, stopping proxy listener", format_uuid(&client_id, "client"));
                                drop(stream);
                                break; // Exit the loop instead of continuing
                            }

                            let connection_id = Uuid::new_v4().to_string();

                            // Notify client about new connection
                            {
                                let clients_guard = clients.read().await;
                                if let Some(client) = clients_guard.get(&client_id) {
                                    let message = Message::NewConnection {
                                        proxy_id: proxy_id.clone(),
                                        connection_id: connection_id.clone(),
                                    };
                                    if let Err(e) = client.sender.send(message) {
                                        error!("Failed to notify client about new connection: {}", e);
                                        continue;
                                    }
                                } else {
                                    warn!("Client {} not found for new connection", client_id);
                                    continue;
                                }
                            }

                            // Start forwarding data between the proxy connection and client
                            let clients_clone = clients.clone();
                            let proxy_connections_clone = proxy_connections.clone();
                            let client_id_clone = client_id.clone();
                            let connection_id_clone = connection_id.clone();

                            tokio::spawn(async move {
                                Self::handle_proxy_stream(
                                    stream,
                                    clients_clone,
                                    proxy_connections_clone,
                                    client_id_clone,
                                    connection_id_clone,
                                ).await;
                            });
                        }
                        Err(e) => {
                            error!("Error accepting proxy connection: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Handles bidirectional data forwarding for a single proxy connection
    async fn handle_proxy_stream(
        stream: TcpStream,
        clients: Arc<RwLock<HashMap<String, ClientConnection>>>,
        proxy_connections: Arc<RwLock<HashMap<String, ProxyConnectionInfo>>>,
        client_id: String,
        connection_id: String,
    ) {
        let (mut stream_read, mut stream_write) = stream.into_split();

        // Channel for receiving data from client
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

        // Store proxy connection info
        {
            let mut proxy_connections_guard = proxy_connections.write().await;
            proxy_connections_guard.insert(
                connection_id.clone(),
                ProxyConnectionInfo {
                    sender: tx,
                    client_id: client_id.clone(),
                },
            );
        }

        let connection_id_clone = connection_id.clone();
        let proxy_connections_clone = proxy_connections.clone();

        // Task to read from proxy and send to client
        let read_task = tokio::spawn(async move {
            let mut buffer = [0u8; 4096];

            loop {
                match stream_read.read(&mut buffer).await {
                    Ok(0) => {
                        // Connection closed
                        debug!("Proxy connection {} closed", connection_id);

                        // Notify client about connection close
                        let clients_guard = clients.read().await;
                        if let Some(client) = clients_guard.get(&client_id) {
                            let message = Message::CloseConnection {
                                connection_id: connection_id.clone(),
                            };
                            let _ = client.sender.send(message);
                        }
                        break;
                    }
                    Ok(n) => {
                        // Forward data to client
                        let data = buffer[..n].to_vec();
                        debug!("Forwarding {} bytes from proxy to client {}", n, client_id);

                        let clients_guard = clients.read().await;
                        if let Some(client) = clients_guard.get(&client_id) {
                            let message = Message::Data {
                                connection_id: connection_id.clone(),
                                data,
                            };
                            if let Err(e) = client.sender.send(message) {
                                error!("Failed to forward data to client: {}", e);
                                break;
                            }
                        } else {
                            warn!("Client {} not found for data forwarding", client_id);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading from proxy stream: {}", e);
                        break;
                    }
                }
            }
        });

        // Task to receive data from client and write to proxy
        let write_task = tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                debug!("Writing {} bytes to proxy connection", data.len());
                if let Err(e) = stream_write.write_all(&data).await {
                    error!("Error writing to proxy stream: {}", e);
                    break;
                }
            }
        });

        // Wait for either task to complete
        tokio::select! {
            _ = read_task => {},
            _ = write_task => {},
        }

        // Clean up proxy connection
        {
            let mut proxy_connections_guard = proxy_connections.write().await;
            proxy_connections_guard.remove(&connection_id_clone);
        }

        debug!("Proxy connection {} handler finished", connection_id_clone);
    }
}

impl Clone for Server {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            clients: self.clients.clone(),
            proxy_listeners: self.proxy_listeners.clone(),
            proxy_connections: self.proxy_connections.clone(),
        }
    }
}
