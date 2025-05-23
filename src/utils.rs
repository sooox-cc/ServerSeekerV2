use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunError {
	#[error("Failed to parse address")]
	AddressParseError(#[from] std::net::AddrParseError),
	#[error("I/O error")]
	IOError(#[from] std::io::Error),
	#[error("Malformed response")]
	MalformedResponse,
	#[error("Error while parsing response")]
	ParseResponse(#[from] serde_json::Error),
	#[error("Connection timed out")]
	TimedOut(#[from] tokio::time::error::Elapsed),
	#[error("Server opted out of scanning")]
	ServerOptOut,
	#[error("Error while updating server in database")]
	DatabaseError(#[from] sqlx::Error),
}

impl Into<usize> for RunError {
	fn into(self) -> usize {
		use RunError::*;

		match self {
			AddressParseError(_) => 0,
			IOError(_) => 1,
			MalformedResponse => 2,
			ParseResponse(_) => 3,
			TimedOut(_) => 4,
			ServerOptOut => 5,
			DatabaseError(_) => 6,
		}
	}
}

pub enum MinecraftColorCodes {
	Black,
	DarkBlue,
	DarkGreen,
	DarkAqua,
	DarkRed,
	DarkPurple,
	Gold,
	Gray,
	DarkGray,
	Blue,
	Green,
	Aqua,
	Red,
	LightPurple,
	Yellow,
	White,
	Reset,
	UnknownValue,
}

impl From<String> for MinecraftColorCodes {
	fn from(s: String) -> Self {
		use MinecraftColorCodes::*;

		match s.as_str() {
			"black" => Black,
			"dark_blue" => DarkBlue,
			"dark_green" => DarkGreen,
			"dark_aqua" => DarkAqua,
			"dark_red" => DarkRed,
			"dark_purple" | "purple" => DarkPurple,
			"gold" => Gold,
			"gray" | "grey" => Gray,
			"dark_gray" | "dark_grey" => DarkGray,
			"blue" => Blue,
			"green" => Green,
			"aqua" => Aqua,
			"red" => Red,
			"pink" | "light_purple" => LightPurple,
			"yellow" => Yellow,
			"white" => White,
			"reset" => Reset,
			_ => UnknownValue,
		}
	}
}

impl MinecraftColorCodes {
	pub fn get_code(&self) -> char {
		use MinecraftColorCodes::*;

		match self {
			Black => '0',
			DarkBlue => '1',
			DarkGreen => '2',
			DarkAqua => '3',
			DarkRed => '4',
			DarkPurple => '5',
			Gold => '6',
			Gray => '7',
			DarkGray => '8',
			Blue => '9',
			Green => 'a',
			Aqua => 'b',
			Red => 'c',
			LightPurple => 'd',
			Yellow => 'e',
			White => 'f',
			Reset => 'r',
			// TODO: Currently its only servers that respond with hex values as colors that don't match
			// Maybe theres a way with color averaging to fix this?
			UnknownValue => 'r',
		}
	}
}
