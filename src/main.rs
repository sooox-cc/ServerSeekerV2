use indicatif::ParallelProgressIterator;
mod database;
mod config;
mod ping;
mod response;
mod colors;

use std::error::Error;
use std::future::Future;
use std::sync::Arc;
use colors::{GREEN, RED, RESET, YELLOW};
use config::{load_config, Config};
use database::{connect, fetch_servers};
use std::time::Duration;
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use rayon::prelude::*;

#[tokio::main]
async fn main() {
    // Handle config file
    let config_file: String = std::env::args().nth(1).unwrap_or("config.toml".to_string());
    println!("{GREEN}[INFO] Using config file {}{RESET}", config_file);
    let config: Config = load_config(config_file);

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

        let style = ProgressStyle::with_template("[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7}").unwrap().progress_chars("##-");
        let progress_bar = Arc::new(ProgressBar::new(servers.len() as u64).with_style(style));

        let servers = servers
            .par_iter()
            .map(|s| (port_start..=port_end).map(|p| ping::ping_server((s, p), progress_bar.clone())).collect::<Vec<_>>())
            .flatten()
            .collect::<Vec<_>>();

        futures::future::join_all(servers).await;

        println!("{GREEN}[INFO] Finished pinging all servers{RESET}");
        if !config.rescanner.repeat {
            println!("{GREEN}[INFO] Exiting...{RESET}");
            std::process::exit(0);
        }
    }
}