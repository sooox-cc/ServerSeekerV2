mod database;
mod config;
mod ping;
mod response;
mod models;
mod schema;

use config::{load_config, Config};
use database::connect;
use diesel::prelude::*;
use std::io::{Read, Write};
use std::str::FromStr;
use crate::ping::ping_server;

#[tokio::main]
async fn main() {
    // Handle config file
    let config_file: String = std::env::args().nth(1).unwrap_or("config.toml".to_string());
    let config: &Config = &load_config(config_file);

    // Create database URL
    let database_url = format!("postgresql://{}:{}@{}/postgres",
                               config.database.user,
                               config.database.password,
                               config.database.url);

    // let address: &str = "192.168.1.220";
    let address: &str = "51.161.87.128";
    // let address: &str = "127.0.0.1";
    let port: u16 = 25565;
    let server = ping_server(&address, &port).await;

    if let Ok(server) = server {
        println!("Server {:?} responded! Version: {:?}, MOTD: {:?}, Chat Reports: {:?}", &address, &server.version, &server.motd, &server.prevents_reports);
    }
}