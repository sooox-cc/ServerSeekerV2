diesel::table! {
    servers (id) {
        id -> Integer,
        address -> Text,
        port -> Numeric,
        firstseen -> Integer,
        lastseen -> Integer,
        country -> Text,
        asn -> Text,
        reversedns -> Text,
        organization -> Text,
        version -> Text,
        protocol -> Integer,
        fmlnetworkversion -> Integer,
        motd -> Text,
        icon -> Text,
        timesseen -> Integer,
        preventsreports -> Bool,
        enforcesecure -> Bool,
        whitelist -> Bool,
        cracked -> Bool,
        maxplayers -> Integer,
        onlineplayers -> Integer,
        #[sql_name = "type"]
        software -> Text,
    }
}