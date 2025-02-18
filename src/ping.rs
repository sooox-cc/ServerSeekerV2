use crate::response::{parse_response, Server};
use std::io::{Error, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::str::FromStr;
use std::time::Duration;

const REQUEST: [u8; 9] = [
    6, // Size: Amount of bytes in the message
    0, // ID: Has to be 0
    0, // Protocol Version: Can be anything as long as it's a valid varint
    0, // Server address
    0, 0, // Port: Can be anything (Notchian servers don't this)
    1, // Next state: 1 for status, 2 for login. Therefore, has to be 1
    1, // Size
    0, // ID
];

pub async fn ping_server(address: &str, port: u16) -> Result<Server, Error> {
    let address = format!("{}:{}", address, port);
    let socket = SocketAddr::from_str(address.as_str()).unwrap();

    match TcpStream::connect_timeout(&socket, Duration::from_secs(3)) {
        Ok(mut stream) => {
            stream.write(&REQUEST)?;
            let mut buff: [u8; 1024] = [0; 1024];

            // Read the first buffer, getting the initial bytes needed
            let mut total_read = stream.read(&mut buff)?;
            // Grab the final buffer size
            let buff_size = decode(&buff.to_vec());
            // and use that to calculate the total amount of bytes needed (in the entire packet)
            let bytes_needed = buff_size.0 + (buff_size.1 as usize);
            let mut out_buff = vec![];
            out_buff.extend_from_slice(&buff[..total_read]);
            // Just incase, we can also calculate the final size of the json
            let json_bytes = decode(&(buff[(buff_size.1+1).into()..]));

            // Repeat until we read everything
            while total_read < bytes_needed {
                let read = stream.read(&mut buff)?;
                out_buff.extend_from_slice(&buff[..read]);
                total_read += read;
            }

            let response: String = String::from_utf8_lossy(
                &out_buff[
                    (buff_size.1 + 1 + json_bytes.1).into()
                        ..]
            ).to_string();

            Ok(parse_response(response.as_str())?)
        }
        Err(_) => {
            Err(Error::new(ErrorKind::NetworkUnreachable, "Server did not respond."))
        }
    }
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