mod application;
mod domain;
mod infrastructure;
mod presentation;

use crate::presentation::admin::{socket_client, AdminResponse};
use crate::presentation::cli::{Cli, Command};
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Serve { config } => {
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer())
                .with(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive("app=info".parse()?),
                )
                .init();

            presentation::http::server::run(&config).await?;
        }

        Command::Purge {
            path,
            prefix,
            all,
            socket,
        } => {
            let response = if all {
                socket_client::purge_all(&socket).await?
            } else if let Some(prefix) = prefix {
                socket_client::purge_prefix(&socket, &prefix).await?
            } else if let Some(path) = path {
                socket_client::purge_path(&socket, &path).await?
            } else {
                socket_client::purge_all(&socket).await?
            };

            match response {
                AdminResponse::Ok { message } => println!("{}", message),
                AdminResponse::Error { message } => eprintln!("Error: {}", message),
                _ => {}
            }
        }

        Command::Stats { socket } => {
            let response = socket_client::stats(&socket).await?;

            match response {
                AdminResponse::Stats { entry_count } => {
                    println!("Cache entries: {}", entry_count);
                }
                AdminResponse::Error { message } => eprintln!("Error: {}", message),
                _ => {}
            }
        }
    }

    Ok(())
}
