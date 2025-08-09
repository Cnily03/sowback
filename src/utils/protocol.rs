use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::utils::crypto::{sha256_with_salt, MAGIC_SALT};

/// Messages exchanged between client and server
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub enum Message {
    /// Client authentication request
    Auth {
        enc_token: Vec<u8>,
        client_id: String,
        /// client name
        name: Option<String>,
    },
    /// Server authentication response
    AuthResponse {
        success: bool,
        session_key: Option<Vec<u8>>,
        /// server name
        name: Option<String>,
        error: Option<String>,
    },
    /// Client proxy configuration
    ProxyConfig {
        local_ip: String,
        local_port: u16,
        remote_port: u16,
    },
    /// Server proxy configuration response
    ProxyConfigResponse {
        success: bool,
        proxy_id: Option<String>,
        error: Option<String>,
    },
    /// Heartbeat message
    Heartbeat { timestamp: u64 },
    /// Heartbeat response
    HeartbeatResponse { timestamp: u64 },
    /// New connection request from server to client
    NewConnection {
        proxy_id: String,
        connection_id: String,
    },
    /// Connection response from client
    ConnectionResponse {
        connection_id: String,
        success: bool,
        error: Option<String>,
    },
    /// Data transfer
    Data {
        connection_id: String,
        data: Vec<u8>,
    },
    /// Close connection
    CloseConnection { connection_id: String },
    /// Error message
    Error { message: String },
}

impl Message {
    /// Creates a new authentication message
    pub fn new_auth(token: &str, client_id: &str, name: Option<String>) -> Self {
        Message::Auth {
            enc_token: sha256_with_salt(token.as_bytes(), MAGIC_SALT),
            client_id: client_id.to_string(),
            name,
        }
    }

    /// Creates a new heartbeat message with current timestamp
    pub fn new_heartbeat() -> Self {
        Message::Heartbeat {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Creates a new data message for forwarding payload
    pub fn new_data(connection_id: &str, data: Vec<u8>) -> Self {
        Message::Data {
            connection_id: connection_id.to_string(),
            data,
        }
    }

    /// Creates a new close connection message
    pub fn new_close_connection(connection_id: &str) -> Self {
        Message::CloseConnection {
            connection_id: connection_id.to_string(),
        }
    }
}

/// Frame format for message serialization
#[derive(Debug)]
pub struct Frame {
    pub length: u32,
    pub message: Message,
}

impl Frame {
    /// Creates a new frame containing the specified message
    pub fn new(message: Message) -> Self {
        Self {
            length: 0, // Will be set during serialization
            message,
        }
    }

    /// Serializes the frame into bytes for network transmission
    pub fn serialize(&self) -> Result<Vec<u8>, anyhow::Error> {
        let config = bincode::config::standard();
        let message_data = bincode::encode_to_vec(&self.message, config)
            .map_err(|e| anyhow::anyhow!("Serialization error: {:?}", e))?;
        let length = message_data.len() as u32;

        let mut result = Vec::new();
        result.extend_from_slice(&length.to_be_bytes());
        result.extend_from_slice(&message_data);

        Ok(result)
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), anyhow::Error> {
        if data.len() < 4 {
            return Err(anyhow::anyhow!("Insufficient data for length field"));
        }

        let length = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;

        if data.len() < 4 + length {
            return Err(anyhow::anyhow!("Insufficient data for message"));
        }

        let message_data = &data[4..4 + length];
        let config = bincode::config::standard();
        let (message, _): (Message, usize) = bincode::decode_from_slice(message_data, config)
            .map_err(|e| anyhow::anyhow!("Deserialization error: {:?}", e))?;

        Ok((
            Frame {
                length: length as u32,
                message,
            },
            4 + length,
        ))
    }
}
