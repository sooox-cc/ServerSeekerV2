use diesel::prelude::*;

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::schema::servers)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[allow(dead_code)]
pub struct Server {
    pub id: i32,
    pub address: String,
    pub port: bigdecimal::BigDecimal,
    pub firstseen: i32,
    pub lastseen: i32,
    pub country: String,
    pub asn: String,
    pub reversedns: String,
    pub organization: String,
    pub version: String,
    pub protocol: i32,
    pub fmlnetworkversion: i32,
    pub motd: String,
    pub icon: String,
    pub timesseen: i32,
    pub preventsreports: bool,
    pub enforcesecure: bool,
    pub whitelist: bool,
    pub cracked: bool,
    pub maxplayers: i32,
    pub onlineplayers: i32,
    pub software: String,
}