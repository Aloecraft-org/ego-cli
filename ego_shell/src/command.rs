use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    Echo { text: String },
    DelayedEcho { text: String, delay_secs: u64 },
    Status,
    Exit,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Output(String),
    Status(StatusInfo),
    Error(String),
    Exit(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    pub uptime_secs: u64,
    pub commands_processed: u64,
    pub platform: String,
    pub timestamp: u64,
}