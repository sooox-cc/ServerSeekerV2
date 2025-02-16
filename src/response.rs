use std::io::{Error, ErrorKind};
use std::str::FromStr;
use diesel::row::NamedRow;
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

// Lifetimes *should* solve this mess
// Recursive JSON array/object parser
fn parse_motd(motd: &Value, mut current_motd: String) -> Result<String, Error> {
    let mut current_motd = current_motd.clone();

    if let Some(object) = motd.as_object() {
        if object.contains_key("color") {
            &current_motd.push_str(&object["color"].to_string());
        }

        if object.contains_key("bold") {
            &current_motd.push_str("§l");
        }

        if object.contains_key("underlined") {
            &current_motd.push_str("§n");
        }

        if object.contains_key("text") {
            if let Some(text) = object["text"].as_str() {
                current_motd.push_str(text);
            }
        }

        if object.contains_key("extra") {
            match object.get("extra") {
                Some(extra) => parse_motd(extra, current_motd.clone())?,
                None => Err(Error::new(ErrorKind::InvalidData, "Extra array malformed"))?,
            };
        }

        Ok(current_motd)
    } else {
        if let Some(array) = motd.as_array() {
            for value in array {
                if value.is_object() || value.is_array() {
                    parse_motd(value, current_motd.clone())?;
                } else {
                    &current_motd.push_str(value.as_str().unwrap());
                }
            }
        }

        Ok(current_motd)
    }
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
    let mut motd: Option<String> = None;
    let mut prevents_reports: Option<bool> = None;
    let mut enforces_secure_chat: Option<bool> = None;
    let mut mods: Option<Vec<Mod>> = None;
    let mut players: Option<Vec<Player>> = None;

    // TODO! I think I can make this all better
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

    if let Some(value) = json.get("description") {
        motd = match parse_motd(value, "".to_string()) {
            Ok(description) => Some(description),
            Err(_) => None,
        };
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
        motd,
        prevents_reports,
        enforces_secure_chat,
        mods: None,
        players: None,
    })
}