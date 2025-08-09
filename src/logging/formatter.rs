use colored::*;

/// Formats a UUID for display with color coding based on its purpose
pub fn format_uuid(uuid: &str, purpose: &str) -> String {
    let short_uuid = &uuid[..8];
    match purpose {
        "conn" => short_uuid.yellow().to_string(),
        "proxy" => short_uuid.green().to_string(),
        "client" | "server" => short_uuid.blue().to_string(),
        _ => short_uuid.normal().to_string(),
    }
}

/// Formats client identification information with optional name and IP address
pub fn format_client_info(name: Option<&str>, addr: &str) -> String {
    match name {
        Some(n) if !n.is_empty() => format!("{} ({})", n.cyan(), addr),
        _ => addr.to_string(),
    }
}

/// Formats service configuration for display with color coding
pub fn format_service_config(local_ip: &str, local_port: u16, remote_port: u16) -> String {
    format!(
        "{}:{} -> :{}",
        local_ip.magenta(),
        local_port.to_string().magenta(),
        remote_port.to_string().green()
    )
}
