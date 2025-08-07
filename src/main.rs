use anyhow::Result;
use clap::{Parser, Subcommand};

mod config;
mod protocol;
mod server;
mod client;
mod crypto;
mod utils;
mod proxy;
mod logging;

use config::{Config, ServerConfig, ClientConfig};
use server::Server;
use client::Client;
use logging::init_logger;

#[derive(Parser)]
#[command(name = "sowback")]
#[command(about = "High-performance tunnel proxy similar to frp")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the server (listen mode)
    Listen {
        /// Configuration file path
        #[arg(short, long)]
        config: Option<String>,
        
        /// Listen address (default: 0.0.0.0:7000)
        address: Option<String>,
        
        /// Bind address for services (default: 0.0.0.0)
        #[arg(long)]
        bind: Option<String>,
        
        /// Authentication token (required)
        #[arg(long)]
        token: Option<String>,
        
        /// Server name for identification
        #[arg(long)]
        name: Option<String>,
        
        /// Log file path
        #[arg(long)]
        log: Option<String>,
        
        /// Verbose logging
        #[arg(short, long)]
        verbose: bool,
    },
    /// Connect to server (client mode)
    Connect {
        /// Configuration file path
        #[arg(short, long)]
        config: Option<String>,
        
        /// Server addresses (can specify multiple)
        servers: Vec<String>,
        
        /// Authentication token (required)
        #[arg(long)]
        token: Option<String>,
        
        /// Service configurations: local_ip:local_port:remote_port
        #[arg(short, long, action = clap::ArgAction::Append)]
        service: Vec<String>,
        
        /// Client name for identification
        #[arg(long)]
        name: Option<String>,
        
        /// Log file path
        #[arg(long)]
        log: Option<String>,
        
        /// Verbose logging
        #[arg(short, long)]
        verbose: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing based on verbose mode from CLI args
    let verbose = match &cli.command {
        Commands::Listen { verbose, .. } => *verbose,
        Commands::Connect { verbose, .. } => *verbose,
    };
    
    if verbose {
        // In verbose mode, allow tracing output
        tracing_subscriber::fmt()
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .init();
    } else {
        // In normal mode, suppress tracing output by setting a high level filter
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .init();
    }

    match cli.command {
        Commands::Listen { config, address, bind, token, name, log, verbose } => {
            // Initialize logging system
            init_logger(log.clone(), verbose);
            
            let mut server_config = if let Some(config_path) = config {
                Config::from_file(&config_path)?.server.unwrap_or_default()
            } else {
                ServerConfig::default()
            };

            // Override with command line arguments
            if let Some(addr) = address {
                server_config.listen_addr = addr;
            }
            if let Some(bind_addr) = bind {
                server_config.bind_addr = bind_addr;
            }
            if let Some(auth_token) = token {
                server_config.token = auth_token;
            } else if server_config.token.is_empty() {
                return Err(anyhow::anyhow!("Token is required. Please provide --token"));
            }
            if let Some(name_str) = name {
                server_config.name = Some(name_str);
            }
            
            if let Some(log_file) = log {
                server_config.log_file = Some(log_file);
            }

            let display_name = server_config.name.as_deref().unwrap_or("server");
            info!("Server '{}' listening on {}. Services will bind on {}.", 
                  display_name, server_config.listen_addr, server_config.bind_addr);

            let server = Server::new(server_config);
            server.run().await?;
        },
        Commands::Connect { config, servers, token, service, name, log, verbose } => {
            // Initialize logging system
            init_logger(log.clone(), verbose);
            
            let mut client_config = if let Some(config_path) = config {
                Config::from_file(&config_path)?.client.unwrap_or_default()
            } else {
                ClientConfig::default()
            };

            // Override with command line arguments
            if !servers.is_empty() {
                client_config.servers = servers;
            }
            if let Some(auth_token) = token {
                client_config.token = auth_token;
            } else if client_config.token.is_empty() {
                return Err(anyhow::anyhow!("Token is required. Please provide --token"));
            }
            if !service.is_empty() {
                client_config.services = service;
            }
            if let Some(client_name) = name {
                client_config.name = Some(client_name);
            }

            let display_name = client_config.name.as_deref().unwrap_or("client");
            info!("Client '{}' connecting to servers: {:?}", display_name, client_config.servers);
            
            let client = Client::new(client_config);
            client.run().await?;
        },
    }

    Ok(())
}
