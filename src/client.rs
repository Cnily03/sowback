use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{interval, timeout, Duration};
use uuid::Uuid;
use anyhow::Result;

use crate::config::{ClientConfig, ServiceConfig};
use crate::protocol::{Message, Frame};
use crate::crypto::CryptoContext;
use crate::utils::FrameReader;
use crate::logging::{format_uuid, format_service_config};
use crate::{log_info, log_warn, log_error, log_debug, info, warn, error, debug};

/// Main client structure that manages connections to multiple servers
pub struct Client {
    config: ClientConfig,
    client_id: String,
    connections: Arc<Mutex<HashMap<String, ServerConnection>>>,
    local_connections: Arc<Mutex<HashMap<String, LocalConnection>>>,
}

/// Represents a connection to a server with its communication channel
struct ServerConnection {
    server_addr: String,
    sender: mpsc::UnboundedSender<Message>,
    crypto: Option<Arc<CryptoContext>>,
    connected: bool,
}

struct LocalConnection {
    sender: mpsc::UnboundedSender<Vec<u8>>,
}

impl Client {
    /// Creates a new client instance with the given configuration
    pub fn new(config: ClientConfig) -> Self {
        Self {
            config,
            client_id: Uuid::new_v4().to_string(),
            connections: Arc::new(Mutex::new(HashMap::new())),
            local_connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Starts the client and maintains connections to all configured servers
    pub async fn run(&self) -> Result<()> {
        log_info!(&format!("Starting client with ID: {}", self.client_id));
        info!("sowback client started, ID: {}", format_uuid(&self.client_id, "client"));

        // Parse service configurations
        let mut service_configs = Vec::new();
        for service_str in &self.config.services {
            match ServiceConfig::parse(service_str) {
                Ok(config) => service_configs.push(config),
                Err(e) => {
                    error!("Invalid service configuration '{}': {}", service_str, e);
                    continue;
                }
            }
        }

        // Connect to all servers
        let mut tasks = Vec::new();
        
        for server_addr in &self.config.servers {
            let client = self.clone();
            let server_addr = server_addr.clone();
            let service_configs = service_configs.clone();
            
            let task = tokio::spawn(async move {
                client.connect_to_server(server_addr, service_configs).await
            });
            
            tasks.push(task);
        }

        // Wait for all tasks to complete
        for task in tasks {
            if let Err(e) = task.await? {
                error!("Server connection error: {}", e);
            }
        }

        Ok(())
    }

    /// Maintains connection to a single server with automatic reconnection on failure
    async fn connect_to_server(
        &self,
        server_addr: String,
        service_configs: Vec<ServiceConfig>,
    ) -> Result<()> {
        loop {
            log_info!(&format!("Connecting to server: {}", server_addr));
            
            match self.try_connect_to_server(&server_addr, &service_configs).await {
                Ok(_) => {
                    log_info!(&format!("Connection to {} closed", server_addr));
                }
                Err(e) => {
                    error!("Connection to {} failed: {}", server_addr, e);
                }
            }

            // Wait before reconnecting
            log_info!(&format!("Reconnecting to {} in {} seconds", server_addr, self.config.reconnect_interval));
            tokio::time::sleep(Duration::from_secs(self.config.reconnect_interval)).await;
        }
    }

    /// Attempts to establish a connection to a server and handle the session
    async fn try_connect_to_server(
        &self,
        server_addr: &str,
        service_configs: &[ServiceConfig],
    ) -> Result<()> {
        let mut stream = TcpStream::connect(server_addr).await?;
        info!("Connected to server: {}", server_addr);

        // Send authentication
        let auth_message = Message::new_auth(&self.config.token, &self.client_id);
        let auth_frame = Frame::new(auth_message);
        stream.write_all(&auth_frame.serialize()?).await?;

        // Read authentication response
        let mut frame_reader = FrameReader::new();
        let mut buffer = [0u8; 4096];
        
        let n = timeout(Duration::from_secs(30), stream.read(&mut buffer)).await??;
        if n == 0 {
            return Err(anyhow::anyhow!("Connection closed during auth"));
        }

        frame_reader.feed_data(&buffer[..n]);
        
        let frame = match frame_reader.try_read_frame()? {
            Some(frame) => frame,
            None => return Err(anyhow::anyhow!("Incomplete auth response")),
        };

        let crypto = match frame.message {
            Message::AuthResponse { success, session_key, error } => {
                if !success {
                    return Err(anyhow::anyhow!("Authentication failed: {}", 
                                             error.unwrap_or_else(|| "Unknown error".to_string())));
                }

                let session_key = session_key.ok_or_else(|| anyhow::anyhow!("No session key provided"))?;
                let crypto = Arc::new(CryptoContext::new(&session_key)?);
                log_info!(&format!("Authentication successful for server: {}", server_addr));
                crypto
            }
            _ => return Err(anyhow::anyhow!("Expected auth response")),
        };

        // Send service configurations
        for service_config in service_configs {
            let service_str = format!("{}:{}:{}", service_config.local_ip, service_config.local_port, service_config.remote_port);
            
            let service_message = Message::ProxyConfig {
                local_ip: service_config.local_ip.clone(),
                local_port: service_config.local_port,
                remote_port: service_config.remote_port,
            };
            let service_frame = Frame::new(service_message);
            stream.write_all(&service_frame.serialize()?).await?;

            log_info!(&format!("Sent service config '{}': {}:{} -> :{}", 
                  service_str, service_config.local_ip, service_config.local_port, service_config.remote_port));
            info!("Registered service '{}': {}", 
                service_str, 
                format_service_config(&service_config.local_ip, service_config.local_port, service_config.remote_port)
            );
        }

        // Create connection channels
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        
        // Store connection
        {
            let mut connections = self.connections.lock().await;
            connections.insert(server_addr.to_string(), ServerConnection {
                server_addr: server_addr.to_string(),
                sender: tx,
                crypto: Some(crypto.clone()),
                connected: true,
            });
        }

        // Start heartbeat task
        let heartbeat_tx = {
            let connections = self.connections.clone();
            let server_addr = server_addr.to_string();
            let heartbeat_interval = self.config.heartbeat_interval;
            
            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(heartbeat_interval));
                
                loop {
                    interval.tick().await;
                    
                    let connections_guard = connections.lock().await;
                    if let Some(conn) = connections_guard.get(&server_addr) {
                        if conn.connected {
                            let heartbeat = Message::new_heartbeat();
                            if let Err(e) = conn.sender.send(heartbeat) {
                                error!("Failed to send heartbeat: {}", e);
                                break;
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            })
        };

        // Convert service_configs to owned data
        let service_configs_owned: Vec<ServiceConfig> = service_configs.to_vec();

        // Handle incoming messages
        let (mut stream_read, mut stream_write) = stream.into_split();
        
        let read_task = {
            let connections = self.connections.clone();
            let local_connections = self.local_connections.clone();
            let server_addr = server_addr.to_string();
            
            tokio::spawn(async move {
                let mut frame_reader = FrameReader::new();
                let mut buffer = [0u8; 4096];

                loop {
                    match stream_read.read(&mut buffer).await {
                        Ok(0) => break,
                        Ok(n) => {
                            frame_reader.feed_data(&buffer[..n]);
                            
                            while let Some(frame) = frame_reader.try_read_frame().unwrap_or(None) {
                                Self::handle_server_message(
                                    frame.message, 
                                    &connections, 
                                    &local_connections,
                                    &service_configs_owned,
                                    &server_addr
                                ).await;
                            }
                        }
                        Err(e) => {
                            error!("Error reading from server {}: {}", server_addr, e);
                            break;
                        }
                    }
                }

                // Mark connection as disconnected
                let mut connections_guard = connections.lock().await;
                if let Some(conn) = connections_guard.get_mut(&server_addr) {
                    conn.connected = false;
                }
            })
        };

        // Handle outgoing messages
        let write_task = {
            tokio::spawn(async move {
                while let Some(message) = rx.recv().await {
                    let frame = Frame::new(message);
                    match frame.serialize() {
                        Ok(data) => {
                            if let Err(e) = stream_write.write_all(&data).await {
                                error!("Error writing to server: {}", e);
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

        // Wait for tasks to complete
        tokio::select! {
            _ = read_task => {},
            _ = write_task => {},
            _ = heartbeat_tx => {},
        }

        // Clean up connection
        {
            let mut connections = self.connections.lock().await;
            connections.remove(server_addr);
        }

        Ok(())
    }

    /// Processes messages received from a server
    async fn handle_server_message(
        message: Message,
        connections: &Arc<Mutex<HashMap<String, ServerConnection>>>,
        local_connections: &Arc<Mutex<HashMap<String, LocalConnection>>>,
        service_configs: &[ServiceConfig],
        server_addr: &str,
    ) {
        match message {
            Message::ProxyConfigResponse { success, proxy_id, error } => {
                if success {
                    if let Some(id) = proxy_id {
                        log_info!(&format!("Service configuration accepted by {}: {}", server_addr, id));
                    } else {
                        log_info!(&format!("Service configuration accepted by {}", server_addr));
                    }
                } else {
                    error!("Service configuration rejected by {}: {}", 
                           server_addr, error.unwrap_or_else(|| "Unknown error".to_string()));
                }
            }
            Message::HeartbeatResponse { timestamp } => {
                debug!("Heartbeat response from {}: {}", server_addr, timestamp);
            }
            Message::NewConnection { proxy_id, connection_id } => {
                log_info!(&format!("New connection request from {}: proxy={}, conn={}", 
                      server_addr, proxy_id, connection_id));
                info!("New connection: proxy={}, conn={}", 
                    format_uuid(&proxy_id, "proxy"), 
                    format_uuid(&connection_id, "conn")
                );
                
                // Find the corresponding service config
                if let Some(service_config) = service_configs.first() {
                    // Establish local connection
                    let local_addr = format!("{}:{}", service_config.local_ip, service_config.local_port);
                    
                    match TcpStream::connect(&local_addr).await {
                        Ok(local_stream) => {
                            log_info!(&format!("Connected to local service at {}", local_addr));
                            
                            // Send success response
                            let connections_guard = connections.lock().await;
                            if let Some(conn) = connections_guard.get(server_addr) {
                                let response = Message::ConnectionResponse {
                                    connection_id: connection_id.clone(),
                                    success: true,
                                    error: None,
                                };
                                let _ = conn.sender.send(response);
                            }
                            
                            // Start handling the local connection
                            let connections_clone = connections.clone();
                            let local_connections_clone = local_connections.clone();
                            let server_addr_clone = server_addr.to_string();
                            let connection_id_clone = connection_id.clone();
                            
                            tokio::spawn(async move {
                                Self::handle_local_connection(
                                    local_stream,
                                    connections_clone,
                                    local_connections_clone,
                                    server_addr_clone,
                                    connection_id_clone,
                                ).await;
                            });
                        }
                        Err(e) => {
                            error!("Failed to connect to local service {}: {}", local_addr, e);
                            
                            // Send error response
                            let connections_guard = connections.lock().await;
                            if let Some(conn) = connections_guard.get(server_addr) {
                                let response = Message::ConnectionResponse {
                                    connection_id,
                                    success: false,
                                    error: Some(format!("Failed to connect to local service: {}", e)),
                                };
                                let _ = conn.sender.send(response);
                            }
                        }
                    }
                }
            }
            Message::Data { connection_id, data } => {
                debug!("Data from {}: conn={}, len={}", server_addr, connection_id, data.len());
                
                // Forward data to local connection
                let local_connections_guard = local_connections.lock().await;
                if let Some(local_conn) = local_connections_guard.get(&connection_id) {
                    if let Err(e) = local_conn.sender.send(data) {
                        error!("Failed to forward data to local connection: {}", e);
                    }
                } else {
                    warn!("Local connection {} not found", connection_id);
                }
            }
            Message::CloseConnection { connection_id } => {
                log_info!(&format!("Close connection from {}: {}", server_addr, connection_id));
                
                // Remove local connection
                let mut local_connections_guard = local_connections.lock().await;
                local_connections_guard.remove(&connection_id);
            }
            _ => {
                warn!("Unexpected message from server {}: {:?}", server_addr, message);
            }
        }
    }

    /// Handles a new connection from the local service and forwards data to the server
    async fn handle_local_connection(
        stream: TcpStream,
        connections: Arc<Mutex<HashMap<String, ServerConnection>>>,
        local_connections: Arc<Mutex<HashMap<String, LocalConnection>>>,
        server_addr: String,
        connection_id: String,
    ) {
        let (mut stream_read, mut stream_write) = stream.into_split();
        
        // Channel for receiving data from server
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        
        // Store local connection info
        {
            let mut local_connections_guard = local_connections.lock().await;
            local_connections_guard.insert(connection_id.clone(), LocalConnection {
                sender: tx,
            });
        }
        
        let connection_id_clone = connection_id.clone();
        let connections_clone = connections.clone();
        let local_connections_clone = local_connections.clone();

        // Task to read from local service and send to server
        let read_task = tokio::spawn(async move {
            let mut buffer = [0u8; 4096];
            
            loop {
                match stream_read.read(&mut buffer).await {
                    Ok(0) => {
                        // Connection closed
                        debug!("Local connection {} closed", connection_id);
                        
                        // Notify server about connection close
                        let connections_guard = connections.lock().await;
                        if let Some(conn) = connections_guard.get(&server_addr) {
                            let message = Message::CloseConnection {
                                connection_id: connection_id.clone(),
                            };
                            let _ = conn.sender.send(message);
                        }
                        break;
                    }
                    Ok(n) => {
                        // Forward data to server
                        let data = buffer[..n].to_vec();
                        debug!("Forwarding {} bytes from local service to server", n);
                        
                        let connections_guard = connections.lock().await;
                        if let Some(conn) = connections_guard.get(&server_addr) {
                            let message = Message::Data {
                                connection_id: connection_id.clone(),
                                data,
                            };
                            if let Err(e) = conn.sender.send(message) {
                                error!("Failed to forward data to server: {}", e);
                                break;
                            }
                        } else {
                            warn!("Server connection not found for data forwarding");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading from local stream: {}", e);
                        break;
                    }
                }
            }
        });

        // Task to receive data from server and write to local service
        let write_task = tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                debug!("Writing {} bytes to local connection", data.len());
                if let Err(e) = stream_write.write_all(&data).await {
                    error!("Error writing to local stream: {}", e);
                    break;
                }
            }
        });

        // Wait for either task to complete
        tokio::select! {
            _ = read_task => {},
            _ = write_task => {},
        }

        // Clean up local connection
        {
            let mut local_connections_guard = local_connections.lock().await;
            local_connections_guard.remove(&connection_id_clone);
        }
        
        debug!("Local connection {} handler finished", connection_id_clone);
    }
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            client_id: self.client_id.clone(),
            connections: self.connections.clone(),
            local_connections: self.local_connections.clone(),
        }
    }
}
