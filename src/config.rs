use serde::Deserialize;
use std::fs::File;
use std::io::{ErrorKind, Read};

#[derive(Deserialize)]
pub struct Config {
    pub database: Database,
    pub rescanner: Rescanner
}

#[derive(Deserialize)]
pub struct Database {
    pub url: String,
    pub user: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct Rescanner {
    pub rescan_delay: u16,
    pub speed: u16,
    pub repeat: bool,
    pub port_range: String
}

impl Default for Rescanner {
    fn default() -> Self {
        Self {
            rescan_delay: 60,
            speed: 1000,
            repeat: true,
            port_range: "25560 - 25570".to_string()
        }
    }
}

pub fn load_config(path: String) -> Config {
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(error) => match error.kind() {
            ErrorKind::NotFound => panic!("{} not found!", &path),
            ErrorKind::PermissionDenied => panic!("File permissions invalid for {}!", &path),
            _ => panic!("Unknown config error!"),
        }
    };

    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap_or_default();

    toml::from_str(&contents).unwrap()
}