use crate::infrastructure::config::DEFAULT_SOCKET_PATH;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "app")]
#[command(about = "A simple CDN server with local and remote origin support")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start the CDN server
    Serve {
        /// Path to config file
        #[arg(short, long, default_value = "config.yaml")]
        config: PathBuf,
    },

    /// Purge cache entries
    Purge {
        /// Specific path to purge
        #[arg(group = "target")]
        path: Option<String>,

        /// Purge all entries matching this prefix
        #[arg(long, group = "target")]
        prefix: Option<String>,

        /// Purge all cache entries
        #[arg(long, group = "target")]
        all: bool,

        /// Path to socket file
        #[arg(short, long, default_value = DEFAULT_SOCKET_PATH)]
        socket: PathBuf,
    },

    /// Show cache statistics
    Stats {
        /// Path to socket file
        #[arg(short, long, default_value = DEFAULT_SOCKET_PATH)]
        socket: PathBuf,
    },
}
