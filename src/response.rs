use crate::config::PlayerTracking;
use crate::utils::MinecraftColorCodes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "software")]
pub enum ServerType {
	Java,
	Neoforge,
	Lexforge,
	Paper,
	Spigot,
	Bukkit,
	Purpur,
	Folia,
	Pufferfish,
	Velocity,
	Leaves,
	Waterfall,
	Bungeecord,
	Thermos,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Server {
	pub version: Version,
	pub favicon: Option<String>,
	pub players: Players,
	#[serde(rename = "description")]
	pub description_raw: Option<Value>,
	pub description_formatted: Option<String>,
	#[serde(rename = "preventsChatReports")]
	pub prevents_reports: Option<bool>,
	#[serde(rename = "enforcesSecureChat")]
	pub enforces_secure_chat: Option<bool>,
	#[serde(rename = "isModded")]
	pub modded: Option<bool>,
	#[serde(rename = "forgeData")]
	pub forge_data: Option<ForgeData>,
}

#[allow(dead_code)]
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
pub struct Version {
	pub name: String,
	pub protocol: i32,
}

#[allow(dead_code)]
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
pub struct Players {
	pub max: i32,
	pub online: i32,
	pub sample: Option<Vec<Player>>,
}

#[allow(dead_code)]
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
pub struct Player {
	pub id: String,
	pub name: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
pub struct ForgeData {
	pub mods: Vec<Mod>,
}

#[allow(dead_code)]
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
pub struct Mod {
	#[serde(rename = "modId")]
	pub id: String,
	#[serde(rename = "modmarker")]
	pub marker: String,
}

impl Server {
	pub fn get_type(&self) -> ServerType {
		// Neoforge sends an "isModded" field
		if self.modded.is_some() {
			return ServerType::Neoforge;
		}

		// Forge sends a "forgeData" object
		if self.forge_data.is_some() {
			return ServerType::Lexforge;
		}

		match self.version.name.split(' ').nth(0) {
			Some("Paper") => ServerType::Paper,
			Some("Spigot") => ServerType::Spigot,
			Some("Bukkit") => ServerType::Bukkit,
			Some("Purpur") => ServerType::Purpur,
			Some("Folia") => ServerType::Folia,
			Some("Pufferfish") => ServerType::Pufferfish,
			Some("Velocity") => ServerType::Velocity,
			Some("Leaves") => ServerType::Leaves,
			Some("Waterfall") => ServerType::Waterfall,
			Some("Bungeecord") => ServerType::Bungeecord,
			_ => ServerType::Java,
		}
	}

    #[rustfmt::skip]
	pub fn build_server_description(&self, value: &Value) -> String {
		let mut output = String::new();

		match value {
			Value::String(s) => output.push_str(s),
			Value::Array(array) => {
				for value in array {
					output.push_str(&self.build_server_description(value));
				}
			}
			Value::Object(object) => {
				for (key, value) in object {
					match key.as_str() {
						"obfuscated" => {
							if let Some(b) = value.as_bool() && b {
								output.push_str("§k")
							}
						},
						"bold" => {
							if let Some(b) = value.as_bool() && b {
								output.push_str("§l")
							}
						},
						"strikethrough" => {
							if let Some(b) = value.as_bool() && b {
								output.push_str("§m")
							}
						},
						"underline" => {
							if let Some(b) = value.as_bool() && b {
								output.push_str("§n")
							}
						},
						"italic" => {
							if let Some(b) = value.as_bool() && b {
								output.push_str("§o")
							}
						},
						"color" => {
							if let Some(c) = value.as_str() {
								let color = MinecraftColorCodes::from(c.to_string());
								output.push_str(format!("§{}", color.get_code().to_string()).as_str())
							}
						},
						_ => (),
					}
				}

				// MiniMOTD can put the "extra" field before the text field, this causes some servers
				// using it to format incorrectly unless we specifically add the text AFTER
				// all other format codes but BEFORE the extra field
				if object.contains_key("text") {
					let text = object.get("text").unwrap();
					output.push_str(text.as_str().unwrap());
				}
				if object.contains_key("extra") {
					if let Some(extra) = object.get("extra") {
					output.push_str(&self.build_server_description(extra));
					}
				}
			}
			_ => {}
		}

		output
	}

	// Has the user opted out of scanning?
	pub fn check_opt_out(&self) -> bool {
		match &self.description_formatted {
			Some(description) => String::from(description).contains("§b§d§f§d§b"),
			None => false,
		}
	}
}

#[allow(dead_code)]
impl Players {
	pub fn player_track_check(&self, players: PlayerTracking) {
		if let Some(sample) = &self.sample {
			for p in sample {
				if players.players.contains(&p.name) {
					todo!()
				}
			}
		}
	}
}
