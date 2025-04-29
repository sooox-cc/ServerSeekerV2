use std::fmt::Debug;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const PAYLOAD: [u8; 9] = [
	6, // Size: Amount of bytes in the message
	0, // ID: Has to be 0
	0, // Protocol Version: Can be anything as long as it's a valid varint
	0, // Server address
	0, 0, // Port: Can be anything (Notchian servers don't this)
	1, // Next state: 1 for status, 2 for login. Therefore, has to be 1
	1, // Size
	0, // ID
];

#[derive(Debug, Error)]
pub enum PingServerError {
	#[error("Failed to parse address")]
	AddressParseError(#[from] std::net::AddrParseError),
	#[error("I/O error")]
	IOError(#[from] std::io::Error),
	#[error("Malformed response")]
	MalformedResponse,
}

pub async fn ping_server((address, port): &(String, u16)) -> Result<String, PingServerError> {
	let socket = SocketAddr::V4(SocketAddrV4::new(
		Ipv4Addr::from_str(address.as_str())?,
		*port,
	));

	// TODO: Rewrite ALL of this below
	// Connect and create buffer
	let mut stream = TcpStream::connect(&socket).await?;
	let mut buffer = [0; 1024];

	// Send payload
	stream.write_all(&PAYLOAD).await?;
	let total_read = stream.read(&mut buffer).await?;

	// Decode
	let (varint, length) = decode(&buffer);
	let bytes_needed = varint + length as usize;
	if bytes_needed < 3 || total_read > bytes_needed {
		return Err(PingServerError::MalformedResponse);
	}

	let mut output = Vec::with_capacity(bytes_needed);
	output.extend_from_slice(&buffer[..total_read]);
	let json = decode(&(buffer[(length + 1).into()..]));

	// Read everything
	stream
		.take((bytes_needed - total_read) as u64)
		.read_to_end(&mut output)
		.await?;

	if output.len() < (length + 1 + json.1).into() {
		return Err(PingServerError::MalformedResponse);
	}

	Ok(String::from_utf8_lossy(&output[(length + 1 + json.1).into()..]).to_string())
}

fn decode(bytes: &[u8]) -> (usize, u8) {
	let mut val: usize = 0;
	let mut count: u8 = 0;

	for b in bytes {
		val |= ((b & 0x7f) as usize) << count;

		if (b >> 7) != 1 {
			break;
		}

		count += 7;
	}

	(val, (count / 7) + 1)
}
