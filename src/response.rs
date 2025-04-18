use serde::Deserialize;

#[derive(Debug)]
pub enum ServerType {
	JAVA,
	NEOFORGE,
	LEXFORGE,
	PAPER,
	SPIGOT,
	BUKKIT,
	PURPUR,
	FOLIA,
	PUFFERFISH,
	VELOCITY,
	LEAVES,
	WATERFALL,
	BUNGEECORD,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Server {
	pub version: Version,
	pub favicon: Option<String>,
	pub description: Option<String>,
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
pub struct ForgeData {
	pub mods: Vec<Mod>,
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
pub struct Version {
	pub name: String,
	pub protocol: i32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Player {
	pub id: String,
	pub name: String,
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
			return ServerType::NEOFORGE;
		}

		// Forge sends a "forgeData" object
		if self.forge_data.is_some() {
			return ServerType::LEXFORGE;
		}

		match self.version.name.split(' ').nth(0) {
			Some("Paper") => ServerType::PAPER,
			Some("Spigot") => ServerType::SPIGOT,
			Some("Bukkit") => ServerType::BUKKIT,
			Some("Purpur") => ServerType::PURPUR,
			Some("Folia") => ServerType::FOLIA,
			Some("Pufferfish") => ServerType::PUFFERFISH,
			Some("Velocity") => ServerType::VELOCITY,
			Some("Leaves") => ServerType::LEAVES,
			Some("Waterfall") => ServerType::WATERFALL,
			Some("Bungeecord") => ServerType::BUNGEECORD,
			Some(_) => ServerType::JAVA,
			None => ServerType::JAVA,
		}
	}
}

pub fn parse_response(response: String) -> Result<Server, serde_json::Error> {
	Ok(serde_json::from_str(&response)?)
}
