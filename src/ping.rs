use crate::utils::RunError;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::debug;

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

pub async fn ping_server((address, port): (&str, u16)) -> Result<String, RunError> {
	let socket = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from_str(address)?, port));

	let mut stream = TcpStream::connect(&socket).await?;
	stream.write_all(&PAYLOAD).await?;
	let mut buffer = [0; 1024];

	// The index is used to point to the position at the start of the string.
	// It gets increased by the amount of bytes read to decode the packet ID, Packet length
	// And string length
	let mut index = 0;

	// Returns how many bytes were read from the stream into the buffer
	let total_read_bytes = stream.read(&mut buffer).await?;

	if total_read_bytes == 0 {
		debug!("Total read bytes is 0! {buffer:?}");
		return Err(RunError::MalformedResponse);
	}

	// Decode Packet length
	index += decode_varint(&buffer).1;

	// Decode Packet ID
	// Technically since Packet ID should always be 0 and will never take more than 1 byte to encode
	// We could ignore it entirely and just advance the index by 1
	let (packet_id, packet_id_bytes) = decode_varint(&buffer[index as usize..]);
	index += packet_id_bytes;
	if packet_id != 0 {
		debug!("Received invalid packet id from {}: {}", address, packet_id);
	}

	// Decode the string length
	let (string_length, string_length_bytes) = decode_varint(&buffer[index as usize..]);
	index += string_length_bytes;
	// Max size for a 3 byte varint
	if string_length > 32767 {
		debug!(
			"Received abnormally large string length from {}: {}",
			address, string_length
		);
	}

	// WARNING: Don't allocate vec size based on what the server says it needs from the varint.
	// Allocate size based on what the server *actually* sends back, some servers can crash the
	// program by attempting to allocate insane amounts of memory this way.
	//
	// Adds everything we have read so far minus the packet ID and packet length to a new vec

	// Error checking
	if index as usize > total_read_bytes {
		debug!("Index: {index} is bigger than total read bytes: {total_read_bytes}");
		return Err(RunError::MalformedResponse);
	}

	let mut output = Vec::from(&buffer[index as usize..total_read_bytes]);
	let string_length = string_length + index as usize;

	if total_read_bytes > string_length {
		debug!(
			"Total read bytes: {total_read_bytes} is larger than string length: {string_length}"
		);
		return Err(RunError::MalformedResponse);
	}

	// Read the rest of the servers JSON
	stream
		// Takes everything after the end of the data we already have in the buffer
		// Up until the end of the strings length
		.take((string_length - total_read_bytes) as _)
		.read_to_end(&mut output)
		.await?;

	Ok(String::from_utf8_lossy(&output).to_string())
}

// returns the decoded varint and how many bytes were read
fn decode_varint(bytes: &[u8]) -> (usize, u8) {
	let mut value: usize = 0;
	let mut count: u8 = 0;

	for b in bytes {
		value |= ((b & 0x7F) as usize) << count;

		// right shift 7 times, if resulting value is 0 it means this is the end of the varint
		if (b >> 7) != 1 {
			break;
		}

		count += 7;
	}

	(value, (count / 7) + 1)
}
