use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
pub struct Masscan {
	pub ip: String,
	pub ports: Vec<Port>,
}

#[derive(Deserialize)]
pub struct Port {
	port: u16,
}

pub fn start_masscan(masscan_config: String) {
	if cfg!(target_os = "windows") {
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
	}
	.wait()
	.expect("failed to wait on child");
}

pub fn parse_output(masscan_output: String) -> Vec<(String, u16)> {
	let file = std::fs::read_to_string(masscan_output).expect("failed to read masscan");
	let output = serde_json::from_str::<Vec<Masscan>>(&file).expect("failed to read masscan");

	output
		.into_iter()
		.flat_map(|entry| {
			let ip = entry.ip;
			entry.ports.into_iter().map(move |p| (ip.clone(), p.port))
		})
		.collect()
}
