use serde_json::Value;
use std::str::FromStr;

#[derive(Debug)]
pub struct Server {
    pub address: String,
    pub port: i32,
    pub version: Option<String>,
    pub protocol: Option<i32>,
    pub motd: Option<String>,
    pub icon: Option<String>,
    pub prevents_reports: Option<bool>,
    pub enforces_secure_chat: Option<bool>,
    pub online_players: Option<i32>,
    pub max_players: Option<i32>,
    pub software: Option<String>,
    pub mods: Vec<Mod>,
    pub players: Vec<Player>,
}

#[derive(Debug)]
pub struct Player {
    pub username: Option<String>,
    // TODO! Replace this with a UUID type
    pub uuid: Option<String>
}

#[derive(Debug)]
pub struct Mod {
    pub mod_id: Option<String>,
    pub mod_name: Option<String>,
}

// I no longer care about trying to make the parsing code good,
// there is no proper standard for how servers should respond so this is what you get
pub fn parse_response(response: String, host: (&str, u16)) -> anyhow::Result<Server> {
    let json = Value::from_str(response.as_str())?;

    let mut version: Option<String> = None;
    let mut protocol: Option<i32> = None;
    let mut online_players: Option<i32> = None;
    let mut max_players: Option<i32> = None;
    let mut players: Vec<Player> = vec![];
    let mut mods: Vec<Mod> = vec![];

    let icon = json["icon"].as_str().map(String::from);
    let prevents_reports = json["preventsChatReports"].as_bool();
    let enforces_secure_chat = json["enforcesChat"].as_bool();

    // Version object
    if let Some(value) = json.get("version") {
        version = value["name"].as_str().map(String::from);

        if let Some(value) = value["protocol"].as_i64() {
            protocol = Some(value as i32)
        }
    }

    // Players object
    if let Some(value) = json.get("players") {
        if let Some(online) = value["online"].as_i64() {
            online_players = Some(online as i32);
        }

        if let Some(max) = value["max"].as_i64() {
            max_players = Some(max as i32)
        }

        if let Some(player_sample) = value["sample"].as_array() {
            player_sample.iter().for_each(|player| {
                players.push(Player {
                    username: player["name"].as_str().map(String::from),
                    uuid: player["id"].as_str().map(String::from)
                })
            })
        }
    }

    if let Some(mods_array) = json["mods"].as_array() {
        mods_array.iter().for_each(|m| {
            mods.push(Mod {
                mod_name: m["modid"].as_str().map(String::from),
                mod_id: m["version"].as_str().map(String::from)
            })
        })
    }

    Ok(Server {
        address: host.0.to_string(),
        port: host.1 as i32,
        version,
        protocol,
        motd: None,
        icon,
        prevents_reports,
        enforces_secure_chat,
        online_players,
        max_players,
        software: None,
        mods,
        players
    })
}