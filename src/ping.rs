use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;
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

pub async fn ping_server(host: &(String, u16)) -> anyhow::Result<String> {
	let address = format!("{}:{}", host.0, host.1);
	let socket = SocketAddr::from_str(address.as_str())?;

	// Connect and create buffer
	let mut stream =
		tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(&socket)).await??;
	let mut buffer = [0; 1024];

	// Send payload
	stream.write_all(&PAYLOAD).await?;
	let mut total_read = stream.read(&mut buffer).await?;

	// Decode
	let (varint, length) = decode(&buffer);
	let bytes_needed = varint + length as usize;
	let mut output = vec![];
	output.extend_from_slice(&buffer[..total_read]);
	let json = decode(&(buffer[(length + 1).into()..]));

	// Read everything
	while total_read < bytes_needed {
		let read = stream.read(&mut buffer).await?;
		output.extend_from_slice(&buffer[..read]);
		total_read += read;
	}

	// Explicitly shutdown stream
	stream.shutdown().await?;

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
