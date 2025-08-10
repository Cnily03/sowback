use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

/// Main configuration structure that can contain either server or client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: Option<ServerConfig>,
    pub client: Option<ClientConfig>,
}

/// Configuration for server mode operation
/// ```bash
/// sowback listen
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    // Specify a server name for human to identify (not unique)
    pub name: Option<String>,
    /// Port or address
    pub listen_addr: String,
    /// Host to bind the server
    pub bind_host: String,
    /// For authentication and cryptography
    pub token: String,
    /// Maximum number of clients
    pub max_clients: usize,
    /// Log file path
    pub log_file: Option<String>,
}

/// Configuration for client mode operation
/// ```bash
/// sowback connect
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Specify a client name for human to identify (not unique)
    pub name: Option<String>,
    /// List of server addresses to connect to
    pub servers: Vec<String>,
    /// For authentication and cryptography
    pub token: String,
    /// List of services to proxy to all servers
    pub services: Vec<ServiceConfig>,
    /// Interval to reconnect to servers
    pub reconnect_interval: u64,
    /// Interval for sending heartbeat messages
    pub heartbeat_interval: u64,
    /// Log file path
    pub log_file: Option<String>,
}

// --- Default configuration ---

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: None,
            listen_addr: "0.0.0.0:7000".to_string(),
            bind_host: "0.0.0.0".to_string(),
            token: "".to_string(), // No default token - must be provided
            max_clients: 100,
            log_file: None,
        }
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            name: None,
            servers: vec![],       // must be provided at least one server
            token: "".to_string(), // No default token - must be provided
            services: vec![],
            reconnect_interval: 5,
            heartbeat_interval: 30,
            log_file: None,
        }
    }
}

impl Config {
    /// Loads configuration from a TOML file
    pub fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

// ------------------------------------------------

/// Configuration for a single service to be forwarded.
/// - Related to cli option `--service`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_port: u16,
}

impl ServiceConfig {
    /// Parses a service configuration string in the format "local_ip:local_port:remote_port"
    pub fn parse_cli(service_str: &str) -> Result<Self> {
        // [local_ip]:[local_port]:[remote_port]
        let parts: Vec<&str> = service_str.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!(
                "Invalid service format. Expected: local_ip:local_port:remote_port"
            ));
        }

        let name = service_str.to_string();
        Ok(ServiceConfig {
            name,
            local_ip: parts[0].to_string(),
            local_port: parts[1].parse()?,
            remote_port: parts[2].parse()?,
        })
    }
}
