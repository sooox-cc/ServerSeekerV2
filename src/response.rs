use serde::Deserialize;

#[derive(Debug)]
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
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Server {
	pub version: Version,
	pub favicon: Option<String>,
	pub description: Option<Description>,
	#[serde(rename = "preventsChatReports")]
	pub prevents_reports: Option<bool>,
	#[serde(rename = "enforcesSecureChat")]
	pub enforces_secure_chat: Option<bool>,
	pub players: Option<Players>,
	#[serde(rename = "isModded")]
	pub modded: Option<bool>,
	#[serde(rename = "forgeData")]
	pub forge_data: Option<ForgeData>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Version {
	pub name: String,
	pub protocol: i32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum Description {
	Plain(String),
	Complex { text: String },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Players {
	pub max: i32,
	pub online: i32,
	pub sample: Option<Vec<Player>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Player {
	pub id: String,
	pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ForgeData {
	pub mods: Vec<Mod>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Mod {
	#[serde(rename = "modId")]
	pub id: String,
	#[serde(rename = "modmarker")]
	pub marker: String,
}

impl Server {
	pub fn get_type(&self) -> ServerType {
		// Neoforge sends a "isModded" field
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

	pub fn check_opt_out(&self) -> bool {
		match &self.description {
			Some(description) => String::from(description.clone()).contains("§b§d§f§d§b"),
			None => false,
		}
	}
}

impl From<Description> for String {
	fn from(value: Description) -> Self {
		match value {
			Description::Plain(s) => s,
			Description::Complex { text } => text,
		}
	}
}

pub fn parse_response(response: String) -> Result<Server, serde_json::Error> {
	serde_json::from_str(&response)
}
