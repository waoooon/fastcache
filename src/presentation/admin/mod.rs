pub mod socket_client;
pub mod socket_server;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum AdminRequest {
    Purge { path: String },
    PurgePrefix { prefix: String },
    PurgeAll,
    Stats,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AdminResponse {
    Ok { message: String },
    Stats { entry_count: u64 },
    Error { message: String },
}
