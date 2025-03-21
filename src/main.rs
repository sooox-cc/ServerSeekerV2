mod database;
mod config;
mod ping;
mod response;
mod colors;

use colors::{GREEN, RED, RESET, YELLOW};
use config::{load_config, Config};
use database::{connect, fetch_servers, update_server};
use indicatif::{ProgressIterator, ProgressStyle};
use ping::ping_server;
use response::parse_response;
use sqlx::pool::PoolConnection;
use sqlx::Postgres;
use std::time::Duration;
use tokio::task;

#[tokio::main]
async fn main() {
    // Handle config file
    let config_file: String = std::env::args().nth(1).unwrap_or("config.toml".to_string());
    println!("{GREEN}[INFO] Using config file {}{RESET}", config_file);
    let config: Config = load_config(config_file);

    run(config).await;
}

async fn run(config: Config) {
    // Create database URL
    let database_url = format!("postgresql://{}:{}@{}:{}/{}",
                               config.database.user,
                               config.database.password,
                               config.database.url,
                               config.database.port,
                               config.database.table);

    let port_start = config.rescanner.port_range_start;
    let port_end = config.rescanner.port_range_end;
    let total_ports = config.rescanner.total_ports();

    if total_ports > 10 {
        println!("{RED}[WARN] Large amount of ports! Scans will take exponentially longer for each port to scan!{RESET}");
    }

    if !config.rescanner.repeat {
        println!("{YELLOW}[WARN] Repeat is not enabled in config file! Will only scan once!{RESET}");
    }

    let pool = connect(database_url.as_str()).await;
    println!("{GREEN}[INFO] Scanning port range {} - {} ({} port(s) per host){RESET}",
             port_start,
             port_end,
             total_ports);

    loop {
        let servers = match fetch_servers(&pool).await {
            Ok(servers) => {
                println!("{GREEN}[INFO] Found {} servers to rescan!{RESET}", servers.len());
                servers
            },
            Err(_) => {
                println!("{RED}[ERROR] Failed to fetch servers! Waiting 10 seconds and retrying...{RESET}");
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };

        for address in servers.iter().progress_with_style(ProgressStyle::default_bar().progress_chars("##-")) {
            for port in port_start..=port_end {
                if let Some(conn) = pool.try_acquire() {
                    spawn_task(address.to_string(), port, conn).await;
                }
            }
        };

        println!("{GREEN}[INFO] Finished pinging all servers{RESET}");
        if !config.rescanner.repeat {
            println!("{GREEN}[INFO] Exiting...{RESET}");
            std::process::exit(0);
        }
    }
}

async fn spawn_task(address: String, port: u16, conn: PoolConnection<Postgres>) {
    task::spawn(async move {
        if let Ok(server) = ping_server(address.as_str(), port).await {
            if let Ok(response) = parse_response(server.as_str()) {
                // Update server in database
                match update_server(response, address.as_str(), conn).await {
                    Ok(_) => (),
                    Err(e) => println!("{RED}{e}{RESET}")
                }
            }
        }
    });
}