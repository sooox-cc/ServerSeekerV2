use std::io::{Error, ErrorKind};
use std::str::FromStr;
use serde_json::Value;

pub struct Server {
    pub version: Option<String>,
    pub protocol: Option<i64>,
    pub icon: Option<String>,
    pub motd: Option<String>,
    pub prevents_reports: Option<bool>,
    pub enforces_secure_chat: Option<bool>,
    pub mods: Option<Vec<Mod>>,
    pub players: Option<Vec<Player>>,
    pub online_players: Option<i32>,
    pub max_players: Option<i32>,
}

pub struct Player {
    pub username: String,
    // Replace this with a UUID type
    pub uuid: String
}

pub struct Mod {
    pub mod_id: String,
    pub mod_name: String,
}

// I no longer care about trying to make the parsing code good,
// there is no proper standard for how servers should respond so this is what you get
pub fn parse_response(response: &str) -> Result<Server, Error> {
    let json = match Value::from_str(response) {
        Ok(json) => json,
        Err(err) => return Err(Error::new(ErrorKind::InvalidData, format!("{:?}", err)))
    };

    let mut version: Option<String> = None;
    let mut protocol: Option<i64> = None;
    let mut icon: Option<String> = None;
    let mut prevents_reports: Option<bool> = None;
    let mut enforces_secure_chat: Option<bool> = None;

    if let Some(value) = json.get("version") {
        if let Some(name) = value.get("name") {
            version = name.as_str().map(|s| s.to_string());
        }

        if let Some(value) = value.get("protocol") {
            protocol = value.as_i64()
        }
    }

    if let Some(value) = json.get("icon") {
        icon = value.as_str().map(|s| s.to_string())
    }

    if let Some(value) = json.get("preventsChatReports") {
        prevents_reports = value.as_bool();
    }

    if let Some(value) = json.get("enforceSecureChat") {
        enforces_secure_chat = value.as_bool();
    }

    Ok(Server {
        version,
        protocol,
        icon,
        motd: None,
        prevents_reports,
        enforces_secure_chat,
        mods: None,
        players: None,
        online_players: None,
        max_players: None,
    })
}