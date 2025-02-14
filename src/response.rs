use std::io::{Error, ErrorKind};
use std::str::FromStr;
use diesel::row::NamedRow;
use serde_json::Value;

pub struct Server {
    pub version: Option<String>,
    pub protocol: Option<i32>,
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

// None values should insert NULL into the database if no value is found
impl Default for Server {
    fn default() -> Self {
        Server {
            version: None,
            protocol: None,
            icon: None,
            motd: None,
            prevents_reports: None,
            enforces_secure_chat: None,
            mods: None,
            players: None,
        }
    }
}

// Lifetimes will solve this mess
fn parse_motd(motd: Value) -> Result<String, ErrorKind::InvalidData> {
    if motd.is_array() {
        for value in motd.as_array() {
            // Recursion
            if value.is_object() || value.is_array() {
                parse_motd(value.clone())?;
            } else {}
        }
    } else {
        if let Some(object) = motd.as_object() {
            for value in object {
                // Recursion
                if value.is_object() || value.is_array() {
                    parse_motd(value.clone())?;
                }

                if let Some(color) = value.get("color") {}

                if let Some(bold) = value.get("bold") {}

                if let Some(text) = value.get("text") {}
            }
        }
    }
}

// I no longer care about trying to make the parsing code good,
// there is no proper standard for how servers should respond so this is what you get
pub fn parse_response(response: &str) -> Result<Server, Error> {
    let json = Value::from_str(response)?;

    let mut version = ();
    let mut protocol = ();

    // TODO! I think I can make this all better
    if let Some(value) = json.get("version") {
        if let Some(name) = json.get("name") {
            version = version.as_str();
        }

        if let Some(value) = json.get("protocol") {
            protocol = value.as_i64()
        }
    }

    let protocol = json["version"]["protocol"].as_i64();
    let icon = json["favicon"].to_string();
    let prevents_reports = json["preventsReports"].as_bool();
    let enforces_secure_chat = json["enforceSecure"].as_bool();

    let motd: String;
    if json["description"].is_object() {
        // motd = parse_motd(json.get("description").clone(), &mut "".to_string());
    } else {
        motd = json.get("description").unwrap().to_string();
    }

    println!("{}", motd);

    Ok(Server {
        version: None,
        protocol: None,
        icon: None,
        motd: None,
        prevents_reports: None,
        enforces_secure_chat,
        mods: None,
        players: None,
    })
}