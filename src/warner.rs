#[cfg(feature = "warner")]
use azalea_protocol::{
	connect::Connection,
	packets::{
		handshake::ServerboundIntention, login::ServerboundHello, ClientIntention, PROTOCOL_VERSION,
	},
	resolver,
};

#[cfg(feature = "warner")]
pub async fn join_server(address: &str) -> anyhow::Result<()> {
	let address = resolver::resolve_address(&address.try_into().unwrap()).await?;
	let mut conn = Connection::new(&address).await?;

	// handshake
	conn.write(ServerboundIntention {
		// Azalea only supports the latest version of minecraft
		protocol_version: PROTOCOL_VERSION,
		hostname: address.ip().to_string(),
		port: address.port(),
		intention: ClientIntention::Login,
	})
	.await?;

	let mut conn = conn.login();

	// login
	conn.write(ServerboundHello {
		name: String::from("ServerSeekerV2"),
		profile_id: uuid::Uuid::nil(),
	})
	.await?;

	Ok(())
}
