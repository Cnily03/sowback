use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error};

/// Bidirectional data forwarding between two TCP streams
pub async fn forward_data(mut stream1: TcpStream, mut stream2: TcpStream) -> Result<()> {
    let (mut r1, mut w1) = stream1.split();
    let (mut r2, mut w2) = stream2.split();

    let forward1 = async {
        let mut buffer = [0u8; 4096];
        loop {
            match r1.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    if let Err(e) = w2.write_all(&buffer[..n]).await {
                        error!("Error writing to stream2: {}", e);
                        break;
                    }
                    debug!("Forwarded {} bytes from stream1 to stream2", n);
                }
                Err(e) => {
                    error!("Error reading from stream1: {}", e);
                    break;
                }
            }
        }
    };

    let forward2 = async {
        let mut buffer = [0u8; 4096];
        loop {
            match r2.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    if let Err(e) = w1.write_all(&buffer[..n]).await {
                        error!("Error writing to stream1: {}", e);
                        break;
                    }
                    debug!("Forwarded {} bytes from stream2 to stream1", n);
                }
                Err(e) => {
                    error!("Error reading from stream2: {}", e);
                    break;
                }
            }
        }
    };

    tokio::select! {
        _ = forward1 => {},
        _ = forward2 => {},
    }

    Ok(())
}
