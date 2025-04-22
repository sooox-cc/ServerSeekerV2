use serde::Deserialize;
use std::io::Read;
use std::process::Command;

// TODO: Better name for this struct
#[derive(Deserialize)]
pub struct MasscanServer {
	pub ip: String,
	pub ports: Vec<Port>,
}

#[derive(Deserialize)]
pub struct Port {
	port: u16,
}

pub fn start_masscan(masscan_config: String) {
	let mut output = if cfg!(target_os = "windows") {
		Command::new("cmd.exe")
			.arg("/c")
			.arg(format!("masscan -c {masscan_config}"))
			.spawn()
			.expect("failed to execute process")
	} else {
		Command::new("/bin/sh")
			.arg("-c")
			// Masscan needs to be run as root, nothing I can do about it
			// TODO! Change this to be a toggle in the config for security
			.arg(format!("sudo masscan -c {masscan_config}"))
			.spawn()
			.expect("failed to execute process")
	};

	output.wait().expect("failed to wait on child");
}

pub fn parse_output(masscan_output: String) -> Vec<(String, u16)> {
	let file = std::fs::read_to_string(masscan_output).expect("failed to read masscan");
	let output = serde_json::from_str::<Vec<MasscanServer>>(&file).expect("failed to read masscan");
	let mut servers = vec![];

	// TODO: I know this can be done better
	for server in output {
		for port in server.ports {
			servers.push((server.ip.clone(), port.port));
		}
	}

	servers
}
