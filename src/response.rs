use serde_json::Value;
use std::str::FromStr;

#[derive(Debug)]
pub struct Server {
    pub address: String,
    pub version: Option<String>,
    pub protocol: Option<i64>,
    pub icon: Option<String>,
    pub motd: Option<String>,
    pub prevents_reports: Option<bool>,
    pub enforces_secure_chat: Option<bool>,
    pub online_players: Option<i64>,
    pub max_players: Option<i64>,
    pub mods: Vec<Mod>,
    pub players: Vec<Player>,
}

#[derive(Debug)]
pub struct Player {
    pub username: String,
    // TODO! Replace this with a UUID type
    pub uuid: String
}

#[derive(Debug)]
pub struct Mod {
    pub mod_id: String,
    pub mod_name: String,
}

// I no longer care about trying to make the parsing code good,
// there is no proper standard for how servers should respond so this is what you get
pub fn parse_response(response: String, address: String) -> anyhow::Result<Server> {
    let json = Value::from_str(response.as_str())?;

    let mut version: Option<String> = None;
    let mut protocol: Option<i64> = None;
    let mut online_players: Option<i64> = None;
    let mut max_players: Option<i64> = None;
    let mut players: Vec<Player> = vec![];
    let mut mods: Vec<Mod> = vec![];

    let icon: Option<String> = if let Some(value) = json["favicon"].as_str() {
        Some(value.to_string())
    } else { None };

    let prevents_reports: Option<bool> = if let Some(value) = json["preventsChatReports"].as_bool() {
        Some(value)
    } else { None };

    let enforces_secure_chat: Option<bool> = if let Some(value) = json["enforceSecureChat"].as_bool() {
        Some(value)
    } else { None };

    // Version object
    if let Some(value) = json.get("version") {
        if let Some(name) = value["name"].as_str() {
            version = Some(name.to_string());
        }

        if let Some(proto) = value["protocol"].as_i64() {
            protocol = Some(proto);
        }
    }

    // Players object
    if let Some(value) = json.get("players") {
        if let Some(online) = value["online"].as_i64() {
            online_players = Some(online);
        }

        if let Some(max) = value["max"].as_i64() {
            max_players = Some(max)
        }

        if let Some(player_sample) = value["sample"].as_array() {
            for player in player_sample {
                players.push(Player {
                    username: player["username"].as_str().unwrap_or("").to_string(),
                    uuid: player["uuid"].as_str().unwrap_or("").to_string(),
                });
            }
        }
    }

    if let Some(mods_array) = json["mods"].as_array() {
        for entry in mods_array {
            mods.push(Mod {
                mod_id: entry[""].as_str().unwrap_or("").to_string(),
                mod_name: entry[""].as_str().unwrap_or("").to_string(),
            })
        }
    }

    Ok(Server {
        address,
        version,
        protocol,
        icon,
        motd: None,
        prevents_reports,
        enforces_secure_chat,
        mods,
        players,
        online_players,
        max_players
    })
}