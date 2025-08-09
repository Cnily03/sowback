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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub bind_addr: String,
    pub token: String,
    pub max_clients: usize,
    pub name: Option<String>,
    pub log_file: Option<String>,
}

/// Configuration for client mode operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub servers: Vec<String>,
    pub token: String,
    pub services: Vec<String>,
    pub reconnect_interval: u64,
    pub heartbeat_interval: u64,
    pub name: Option<String>,
    pub log_file: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:7000".to_string(),
            bind_addr: "0.0.0.0".to_string(),
            token: "".to_string(), // No default token - must be provided
            max_clients: 100,
            name: None,
            log_file: None,
        }
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            servers: vec!["127.0.0.1:7000".to_string()],
            token: "".to_string(), // No default token - must be provided
            services: vec![],
            reconnect_interval: 5,
            heartbeat_interval: 30,
            name: None,
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

/// Configuration for a single service to be forwarded
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_port: u16,
}

impl ServiceConfig {
    /// Parses a service configuration string in the format "local_ip:local_port:remote_port"
    pub fn parse(service_str: &str) -> Result<Self> {
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
