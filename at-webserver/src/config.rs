use crate::models::ConnectionType;
use std::process::Command;
use log::info;

#[derive(Debug, Clone)]
pub struct Config {
    pub at_config: AtConfig,
    pub notification_config: NotificationConfig,
    pub websocket_port: u16,
}

#[derive(Debug, Clone)]
pub struct AtConfig {
    pub connection_type: ConnectionType,
    pub network: NetworkConfig,
    pub serial: SerialConfig,
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub host: String,
    pub port: u16,
    pub timeout: u64,
}

#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub port: String,
    pub baudrate: u32,
    pub timeout: u64,
}

#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub wechat_webhook: Option<String>,
    pub log_file: String,
    pub notify_sms: bool,
    pub notify_call: bool,
    pub notify_memory_full: bool,
    pub notify_signal: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            at_config: AtConfig {
                connection_type: ConnectionType::Network,
                network: NetworkConfig {
                    host: "192.168.8.1".to_string(),
                    port: 20249,
                    timeout: 10,
                },
                serial: SerialConfig {
                    port: "/dev/ttyUSB2".to_string(),
                    baudrate: 115200,
                    timeout: 10,
                },
            },
            notification_config: NotificationConfig {
                wechat_webhook: None,
                log_file: "/var/log/at-notifications.log".to_string(),
                notify_sms: true,
                notify_call: true,
                notify_memory_full: true,
                notify_signal: true,
            },
            websocket_port: 8765,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut config = Config::default();
        
        // 1. Try to load from UCI (OpenWrt)
        // We capture any errors silently and fall back to defaults
        if let Ok(output) = Command::new("uci").args(&["get", "at-webserver.config.connection_type"]).output() {
            if output.status.success() {
                let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if val == "SERIAL" {
                    config.at_config.connection_type = ConnectionType::Serial;
                } else {
                     config.at_config.connection_type = ConnectionType::Network;
                }
            }
        }
        
        // Load other UCI values (simplified for brevity, normally we'd do this for all fields)
        if let Ok(output) = Command::new("uci").args(&["get", "at-webserver.config.network_host"]).output() {
            if output.status.success() {
                config.at_config.network.host = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }

        // 2. Override with Environment Variables (Local Debugging Priority)
        // These allow running locally without UCI:
        // export AT_CONNECTION_TYPE=SERIAL
        // export AT_SERIAL_PORT=/dev/pts/4
        
        if let Ok(val) = std::env::var("AT_CONNECTION_TYPE") {
            match val.as_str() {
                "SERIAL" => config.at_config.connection_type = ConnectionType::Serial,
                "NETWORK" => config.at_config.connection_type = ConnectionType::Network,
                _ => {}
            }
        }

        if let Ok(val) = std::env::var("AT_NETWORK_HOST") { config.at_config.network.host = val; }
        if let Ok(val) = std::env::var("AT_NETWORK_PORT") { 
            if let Ok(p) = val.parse() { config.at_config.network.port = p; }
        }
        
        if let Ok(val) = std::env::var("AT_SERIAL_PORT") { config.at_config.serial.port = val; }
        if let Ok(val) = std::env::var("AT_SERIAL_BAUDRATE") { 
             if let Ok(b) = val.parse() { config.at_config.serial.baudrate = b; }
        }
        
        if let Ok(val) = std::env::var("AT_LOG_FILE") { config.notification_config.log_file = val; }

        info!("Loaded configuration: {:?}", config);
        config
    }
}
