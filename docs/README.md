# Sowback Protocol Documentation

## Overview

Sowback uses a custom protocol for communication between clients and servers. All communication happens over TCP connections, with messages serialized using bincode and optionally encrypted using AES-256-GCM.

## Connection Establishment

### 1. TCP Connection
Client initiates a TCP connection to the server's listen address (default: port 7000).

### 2. Authentication Flow

#### Client → Server: Auth Request
```rust
Message::Auth {
    token: String,        // Authentication token
    client_id: String,    // Unique client identifier (UUID)
}
```

#### Server → Client: Auth Response
```rust
Message::AuthResponse {
    success: bool,           // Authentication result
    session_key: Option<Vec<u8>>, // Derived session key for encryption
    error: Option<String>,   // Error message if authentication failed
}
```

### 3. Session Key Derivation
- Server derives a unique session key using HKDF-SHA256
- Input: authentication token + client ID
- Output: 32-byte AES-256 key
- This ensures each client has a unique encryption key

## Proxy Configuration

### Client → Server: Service Config
```rust
Message::ProxyConfig {
    local_ip: String,     // Local IP to connect to (e.g., "127.0.0.1")
    local_port: u16,      // Local port to connect to (e.g., 80)
    remote_port: u16,     // Remote port to bind on server (e.g., 8080)
}
```

### Server → Client: Service Config Response
```rust
Message::ProxyConfigResponse {
    success: bool,                // Configuration result
    proxy_id: Option<String>,     // Unique proxy identifier if successful
    error: Option<String>,        // Error message if failed
}
```

## Data Transfer

### New Connection Flow

#### Server → Client: New Connection
When someone connects to the server's bind port:
```rust
Message::NewConnection {
    proxy_id: String,       // Which proxy this connection is for
    connection_id: String,  // Unique identifier for this connection
}
```

#### Client → Server: Connection Response
```rust
Message::ConnectionResponse {
    connection_id: String,  // Same ID from NewConnection
    success: bool,          // Whether local connection was established
    error: Option<String>,  // Error message if failed
}
```

### Data Forwarding

#### Bidirectional Data Transfer
```rust
Message::Data {
    connection_id: String,  // Which connection this data belongs to
    data: Vec<u8>,         // Raw data bytes
}
```

### Connection Closure

#### Connection Close
```rust
Message::CloseConnection {
    connection_id: String,  // Connection to close
}
```

## Heartbeat/Keepalive

### Client → Server: Heartbeat
```rust
Message::Heartbeat {
    timestamp: u64,  // Unix timestamp
}
```

### Server → Client: Heartbeat Response
```rust
Message::HeartbeatResponse {
    timestamp: u64,  // Same timestamp from request
}
```

## Frame Format

All messages are wrapped in frames for reliable transport:

```
+------------------+------------------------+
| Length (4 bytes) | Message Data (N bytes) |
+------------------+------------------------+
```

- **Length**: Big-endian u32 indicating message data length
- **Message Data**: Bincode-serialized Message enum

## Error Handling

### Error Message
```rust
Message::Error {
    message: String,  // Human-readable error description
}
```

## Security Features

### Encryption
- Each client gets a unique AES-256-GCM session key
- Keys derived from shared token + client ID using HKDF-SHA256
- All data messages can be encrypted (optional)

### Authentication
- Token-based authentication required for all connections
- Failed authentication results in immediate connection termination

## Example Flow

1. **Client connects to server**
2. **Authentication:**
   - Client sends `Auth` with token and client ID
   - Server derives session key and responds with `AuthResponse`
3. **Service setup:**
   - Client sends `ProxyConfig` for each local service
   - Server binds to remote ports and responds with `ProxyConfigResponse`
4. **Runtime:**
   - Server listens on configured bind ports
   - When external connection arrives, server sends `NewConnection` to client
   - Client establishes local connection and responds with `ConnectionResponse`
   - Data flows bidirectionally using `Data` messages
   - Connection cleanup with `CloseConnection`
5. **Keepalive:**
   - Client periodically sends `Heartbeat`
   - Server responds with `HeartbeatResponse`

## Configuration Example

### Server (listen mode)
```bash
# Basic server startup
sowback listen 0.0.0.0:7000 --bind 0.0.0.0 --token your-secret

# Server with name and logging
sowback listen 0.0.0.0:7000 --bind 0.0.0.0 --token your-secret --name "main-server" --log /var/log/sowback.log

# Verbose logging
sowback listen 0.0.0.0:7000 --bind 0.0.0.0 --token your-secret --verbose
```

### Client (connect mode)
```bash
# Basic client connection with service configuration
sowback connect 1.2.3.4:7000 --token your-secret --service 127.0.0.1:80:8080

# Client with multiple services
sowback connect 1.2.3.4:7000 --token your-secret \
  --service 127.0.0.1:80:8080 \
  --service 127.0.0.1:3306:3306

# Client with name and logging
sowback connect 1.2.3.4:7000 --token your-secret \
  --service 127.0.0.1:80:8080 \
  --name "web-client" \
  --log /var/log/sowback-client.log \
  --verbose
```

This would:
1. Client connects to server at `1.2.3.4:7000`
2. Server binds proxy listener at `0.0.0.0:8080`
3. Connections to `1.2.3.4:8080` get forwarded to client's `127.0.0.1:80`

## Logging Features

### Log Output Modes

#### Normal Mode (default)
- Console output with colored timestamps and level indicators
- Format: `YYYY-MM-DD HH:MM:SS [LEVEL] message`
- Colors: INFO (green), WARN (yellow), ERROR (red), DEBUG (blue)

#### Verbose Mode (`--verbose`)
- Detailed console output with additional context
- Format: `YYYY-MM-DD HH:MM:SS [LEVEL] message details={json}`
- Shows internal details like connection IDs, proxy IDs, etc.

#### File Logging (`--log <file>`)
- JSON-formatted logs written to specified file
- Each log entry is a single JSON object per line
- Includes timestamp, level, message, and detailed context
- Example:
```json
{"timestamp":"2025-08-07T17:59:16.127916Z","level":"INFO","message":"Server ready, listening on 0.0.0.0:7000"}
{"timestamp":"2025-08-07T17:59:20.123456Z","level":"INFO","message":"Client abc12345 authenticated","details":{"client_id":"abc12345-1234-5678-9abc-123456789abc","connection_count":1}}
```

### UUID Color Coding
- **Connection IDs**: Yellow (conn=abc12345)
- **Proxy IDs**: Green (proxy=def67890)  
- **Client/Server IDs**: Blue (client=ghi13579)

### Service Configuration Display
- Format: `local_ip:local_port -> :remote_port`
- Example: `127.0.0.1:80 -> :8080`
- Local parts in magenta, remote parts in green

## Configuration Files

### Server Configuration (TOML)
```toml
[server]
listen_addr = "0.0.0.0:7000"
bind_host = "0.0.0.0"
token = "your-secret-token"
max_clients = 100
name = "main-server"
log_file = "/var/log/sowback-server.log"
```

### Client Configuration (TOML)
```toml
[client]
servers = ["1.2.3.4:7000", "backup.example.com:7000"]
token = "your-secret-token"
services = ["127.0.0.1:80:8080", "127.0.0.1:3306:3306"]
reconnect_interval = 5
heartbeat_interval = 30
name = "web-client"
log_file = "/var/log/sowback-client.log"
```

### Using Configuration Files
```bash
# Server with config file
sowback listen --config /etc/sowback/server.toml

# Client with config file
sowback connect --config /etc/sowback/client.toml

# Override config file settings with CLI arguments
sowback listen --config /etc/sowback/server.toml --token new-token --verbose
```

## Advanced Configuration

### Performance Tuning
```toml
# High-performance server configuration
[server]
listen_addr = "0.0.0.0:7000"
bind_host = "0.0.0.0"
token = "your-secret-token"
max_clients = 1000
worker_threads = 8
tcp_nodelay = true
keep_alive = true
buffer_size = 65536
connection_timeout = 300
heartbeat_timeout = 60
```

```toml
# High-performance client configuration
[client]
servers = ["1.2.3.4:7000"]
token = "your-secret-token"
services = ["127.0.0.1:80:8080"]
connection_pool_size = 10
reconnect_interval = 5
heartbeat_interval = 30
max_retry_attempts = 5
connection_timeout = 30
```

### Load Balancing and High Availability
```bash
# Multiple server endpoints for failover
sowback connect --server 1.2.3.4:7000,backup.example.com:7000,fallback.example.com:7000 \
  --token secret --service 127.0.0.1:80:8080

# Multiple services on one client
sowback connect --server 1.2.3.4:7000 --token secret \
  --service 127.0.0.1:80:8080 \
  --service 127.0.0.1:3306:3306 \
  --service 127.0.0.1:6379:6379
```

## Troubleshooting

### Common Issues

#### 1. Connection Refused
**Symptoms:**
- Client cannot connect to server
- Error: "Connection refused"

**Solutions:**
```bash
# Check if server is running
netstat -tlnp | grep :7000

# Test basic connectivity
telnet 1.2.3.4 7000

# Check firewall settings
sudo ufw status
sudo iptables -L

# Verify server configuration
sowback listen --token test-token --verbose
```

#### 2. Authentication Failed
**Symptoms:**
- Client connects but gets authentication error
- Error: "Invalid token"

**Solutions:**
```bash
# Verify tokens match exactly (check for whitespace)
echo -n "your-token" | xxd
echo -n "your-token" | wc -c

# Test with simple token
sowback listen --token "simple-test-token"
sowback connect --server localhost:7000 --token "simple-test-token" --service 127.0.0.1:80:8080
```

#### 3. Service Not Reachable
**Symptoms:**
- Connection established but service unreachable
- Timeout when accessing exposed service

**Solutions:**
```bash
# Check if local service is running
netstat -tlnp | grep :80
curl -v http://127.0.0.1:80

# Verify service binding
lsof -i :80
ss -tlnp | grep :80

# Test with simple HTTP server
python3 -m http.server 8000
sowback connect --server 1.2.3.4:7000 --token secret --service 127.0.0.1:8000:8080
```

#### 4. High Latency or Connection Drops
**Symptoms:**
- Slow response times
- Frequent disconnections
- Data transfer errors

**Solutions:**
```bash
# Monitor network conditions
ping 1.2.3.4
mtr 1.2.3.4

# Adjust configuration for poor networks
sowback connect --server 1.2.3.4:7000 --token secret \
  --service 127.0.0.1:80:8080 \
  --heartbeat-interval 10 \
  --reconnect-interval 2

# Enable debug logging
sowback connect --server 1.2.3.4:7000 --token secret \
  --service 127.0.0.1:80:8080 --verbose --log /tmp/debug.log
```

### Debug Commands

#### Network Diagnostics
```bash
# Test connectivity
nc -zv 1.2.3.4 7000

# Monitor connections
watch 'netstat -an | grep 7000'

# Check bandwidth
iperf3 -c 1.2.3.4 -p 5001

# DNS resolution
nslookup server.example.com
dig server.example.com
```

#### Process Monitoring
```bash
# Monitor sowback processes
ps aux | grep sowback
pstree -p $(pgrep sowback)

# Check resource usage
top -p $(pgrep sowback)
htop -p $(pgrep sowback)

# Monitor file descriptors
lsof -p $(pgrep sowback)
```

#### Log Analysis
```bash
# Follow live logs
tail -f /var/log/sowback.log

# Search for specific events
grep "Connection established" /var/log/sowback.log
grep "ERROR\|WARN" /var/log/sowback.log

# Analyze JSON logs
jq '.message' /var/log/sowback.log | sort | uniq -c
jq 'select(.level == "ERROR")' /var/log/sowback.log

# Filter by time range
jq 'select(.timestamp > "2025-01-01T00:00:00Z")' /var/log/sowback.log
```

### Performance Monitoring

#### System Metrics
```bash
# CPU and memory usage
pidstat -u -r -p $(pgrep sowback) 1

# Network I/O
iftop -i eth0
nethogs -p $(pgrep sowback)

# Disk I/O (for logging)
iotop -p $(pgrep sowback)
```

#### Connection Statistics
```bash
# Active connections
ss -tuln | grep sowback
netstat -an | grep :7000 | wc -l

# Connection states
ss -s
netstat -s
```

### Production Deployment

#### Systemd Service
```ini
# /etc/systemd/system/sowback-server.service
[Unit]
Description=Sowback Tunnel Server
After=network.target

[Service]
Type=simple
User=sowback
Group=sowback
ExecStart=/usr/local/bin/sowback listen --config /etc/sowback/server.toml
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

```ini
# /etc/systemd/system/sowback-client.service
[Unit]
Description=Sowback Tunnel Client
After=network.target

[Service]
Type=simple
User=sowback
Group=sowback
ExecStart=/usr/local/bin/sowback connect --config /etc/sowback/client.toml
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

#### Log Rotation
```bash
# /etc/logrotate.d/sowback
/var/log/sowback*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 0644 sowback sowback
    postrotate
        systemctl reload sowback-server sowback-client
    endscript
}
```

#### Security Hardening
```bash
# Create dedicated user
sudo useradd -r -s /bin/false sowback

# Set file permissions
sudo chown sowback:sowback /usr/local/bin/sowback
sudo chmod 755 /usr/local/bin/sowback

# Secure configuration files
sudo chown root:sowback /etc/sowback/server.toml
sudo chmod 640 /etc/sowback/server.toml

# Network security
sudo ufw allow from trusted-network to any port 7000
sudo ufw deny 7000
```
