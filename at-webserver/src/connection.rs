use async_trait::async_trait;
use anyhow::{Result, Context};
use log::info;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tokio_serial::SerialPortBuilderExt;

#[async_trait]
pub trait ATConnection: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
    async fn send(&mut self, data: &[u8]) -> Result<()>;
    async fn receive(&mut self, buffer: &mut [u8]) -> Result<usize>;
    fn is_connected(&self) -> bool;
}

pub struct NetworkATConnection {
    host: String,
    port: u16,
    timeout_secs: u64,
    stream: Option<TcpStream>,
}

impl NetworkATConnection {
    pub fn new(host: String, port: u16, timeout_secs: u64) -> Self {
        Self {
            host,
            port,
            timeout_secs,
            stream: None,
        }
    }
}

#[async_trait]
impl ATConnection for NetworkATConnection {
    async fn connect(&mut self) -> Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        info!("Connecting to network AT server at {}", addr);
        match timeout(Duration::from_secs(self.timeout_secs), TcpStream::connect(&addr)).await {
            Ok(result) => {
                self.stream = Some(result.context("Failed to connect to network AT server")?);
                info!("Connected to network AT server");
                Ok(())
            }
            Err(_) => {
                anyhow::bail!("Connection timed out");
            }
        }
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(mut stream) = self.stream.take() {
            let _ = stream.shutdown().await;
        }
        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        if let Some(stream) = &mut self.stream {
            stream.write_all(data).await.context("Failed to write to stream")?;
            stream.flush().await.context("Failed to flush stream")?;
            Ok(())
        } else {
            anyhow::bail!("Not connected");
        }
    }

    async fn receive(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if let Some(stream) = &mut self.stream {
            stream.read(buffer).await.context("Failed to read from stream")
        } else {
            anyhow::bail!("Not connected");
        }
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }
}

pub struct SerialATConnection {
    port: String,
    baudrate: u32,
    stream: Option<tokio_serial::SerialStream>,
}

impl SerialATConnection {
    pub fn new(port: String, baudrate: u32) -> Self {
        Self {
            port,
            baudrate,
            stream: None,
        }
    }
}

#[async_trait]
impl ATConnection for SerialATConnection {
    async fn connect(&mut self) -> Result<()> {
        info!("Opening serial port {} at {}", self.port, self.baudrate);
        let port = tokio_serial::new(&self.port, self.baudrate)
            .open_native_async()
            .context("Failed to open serial port")?;
        self.stream = Some(port);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.stream = None;
        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        if let Some(stream) = &mut self.stream {
            stream.write_all(data).await.context("Failed to write to serial")?;
            stream.flush().await.context("Failed to flush serial")?;
            Ok(())
        } else {
            anyhow::bail!("Not connected");
        }
    }

    async fn receive(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if let Some(stream) = &mut self.stream {
             stream.read(buffer).await.context("Failed to read from serial")
        } else {
            anyhow::bail!("Not connected");
        }
    }

     fn is_connected(&self) -> bool {
        self.stream.is_some()
    }
}

/// UBUS AT connection: sends AT commands via `ubus call at-daemon sendat`.
///
/// Instead of owning a persistent TTY or TCP socket, each command is executed
/// as a standalone ubus RPC.  `ubus-at-daemon` owns the actual serial port
/// and serialises concurrent access, so both QModem (with `--use-ubus`) and
/// this implementation can share the same modem TTY.
///
/// The implementation mimics a stream-oriented connection:
/// - `send()` accumulates the command until the trailing `\n`, then calls ubus
///   and stores the response in an internal buffer.
/// - `receive()` reads from that buffer, blocking with `tokio::sync::Notify`
///   when no response is available yet (matches the behaviour of the serial
///   and network back-ends so the shared `process_loop` in the actor works
///   without special-casing).
use tokio::sync::Notify;
use std::sync::Arc;

pub struct UbusATConnection {
    port: String,
    timeout_secs: u64,
    /// Buffer for accumulating the AT command across `send()` calls.
    pending_cmd: Vec<u8>,
    /// Full response from the last ubus call.
    response: Vec<u8>,
    response_pos: usize,
    /// Signalled when a new response is ready.
    notify: Arc<Notify>,
}

impl UbusATConnection {
    pub fn new(port: String, timeout_secs: u64) -> Self {
        Self {
            port,
            timeout_secs,
            pending_cmd: Vec::new(),
            response: Vec::new(),
            response_pos: 0,
            notify: Arc::new(Notify::new()),
        }
    }

    /// Execute a single AT command via `ubus call at-daemon sendat` and return
    /// the raw response bytes (including trailing `\r\n`).
    async fn call_ubus(&self, cmd: &str) -> Result<Vec<u8>> {
        let payload = serde_json::json!({
            "at_port": self.port,
            "at_cmd": cmd,
            "timeout": self.timeout_secs as i32,
        });

        info!("UBUS AT => {}", cmd);

        let output = timeout(
            Duration::from_secs(self.timeout_secs + 5),
            Command::new("ubus")
                .args(["call", "at-daemon", "sendat", &payload.to_string()])
                .output(),
        )
        .await
        .context("ubus call timed out")?
        .context("ubus call failed")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            info!("UBUS call stderr: {}", stderr);
            return Ok(b"ERROR\r\n".to_vec());
        }

        let resp_json: serde_json::Value =
            serde_json::from_slice(&output.stdout).context("Failed to parse ubus JSON response")?;

        // Check for timeout first
        if let Some(status) = resp_json["status"].as_str() {
            if status == "timeout" {
                info!("UBUS AT command timed out");
                return Ok(b"ERROR: timeout\r\n".to_vec());
            }
        }

        // Extract the response text
        if let Some(resp_str) = resp_json["response"].as_str() {
            info!("UBUS AT <= {}", resp_str.trim());
            Ok(resp_str.as_bytes().to_vec())
        } else {
            info!("UBUS AT response missing 'response' field: {}", resp_json);
            Ok(b"ERROR\r\n".to_vec())
        }
    }
}

#[async_trait]
impl ATConnection for UbusATConnection {
    async fn connect(&mut self) -> Result<()> {
        // Verify ubus-at-daemon is reachable
        let output = Command::new("ubus")
            .args(["call", "at-daemon", "list"])
            .output()
            .await
            .context("ubus-at-daemon not reachable — is the daemon running?")?;

        if !output.status.success() {
            anyhow::bail!("ubus-at-daemon responded with error status");
        }

        info!("Connected to ubus-at-daemon for port {}", self.port);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.pending_cmd.clear();
        self.response.clear();
        self.response_pos = 0;
        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.pending_cmd.extend_from_slice(data);

        // The caller always sends the command text first, then "\r\n".
        // We fire the ubus call when we see the trailing newline.
        if self.pending_cmd.last() == Some(&b'\n') {
            let full = String::from_utf8_lossy(&self.pending_cmd)
                .trim()
                .trim_end_matches('\r')
                .trim_end_matches('\n')
                .to_string();
            self.pending_cmd.clear();

            if !full.is_empty() {
                let resp = self.call_ubus(&full).await?;
                self.response = resp;
                self.response_pos = 0;
                self.notify.notify_one();
            }
        }

        Ok(())
    }

    async fn receive(&mut self, buffer: &mut [u8]) -> Result<usize> {
        // Wait until a response is available (the send() method signals notify).
        if self.response_pos >= self.response.len() {
            self.notify.notified().await;
        }

        let available = self.response.len().saturating_sub(self.response_pos);
        if available == 0 {
            return Ok(0);
        }

        let to_copy = std::cmp::min(buffer.len(), available);
        buffer[..to_copy].copy_from_slice(&self.response[self.response_pos..self.response_pos + to_copy]);
        self.response_pos += to_copy;
        Ok(to_copy)
    }

    fn is_connected(&self) -> bool {
        true // ubus-at-daemon is a system service; once connected it stays "up"
    }
}
