-- ServerSeekerV2 Database Schema

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Main servers table
CREATE TABLE IF NOT EXISTS servers (
    address INET NOT NULL,
    port INTEGER NOT NULL,
    software TEXT,
    version TEXT,
    protocol INTEGER,
    icon TEXT,
    description_raw JSONB,
    description_formatted TEXT,
    prevents_chat_reports BOOLEAN,
    enforces_secure_chat BOOLEAN,
    first_seen INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    online_players INTEGER,
    max_players INTEGER,
    country TEXT,
    asn TEXT,
    PRIMARY KEY (address, port)
);

-- Players table
CREATE TABLE IF NOT EXISTS players (
    address INET NOT NULL,
    port INTEGER NOT NULL,
    uuid UUID NOT NULL,
    name TEXT NOT NULL,
    first_seen INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    PRIMARY KEY (address, port, uuid)
);

-- Mods table
CREATE TABLE IF NOT EXISTS mods (
    address INET NOT NULL,
    port INTEGER NOT NULL,
    id TEXT NOT NULL,
    mod_marker TEXT,
    PRIMARY KEY (address, port, id)
);

-- Countries table for geolocation data
CREATE TABLE IF NOT EXISTS countries (
    network CIDR NOT NULL,
    country_code TEXT NOT NULL,
    asn TEXT NOT NULL,
    PRIMARY KEY (network)
);

-- Server visits table with enhanced status tracking
CREATE TYPE visit_status AS ENUM ('visited', 'skipped', 'whitelisted');

CREATE TABLE IF NOT EXISTS server_visits (
    address INET NOT NULL,
    port INTEGER NOT NULL,
    status visit_status NOT NULL DEFAULT 'visited',
    visited_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    notes TEXT,
    rating INTEGER CHECK (rating >= 1 AND rating <= 5),
    PRIMARY KEY (address, port),
    FOREIGN KEY (address, port) REFERENCES servers(address, port) ON DELETE CASCADE
);

-- Create indexes for better performance
CREATE INDEX IF NOT EXISTS idx_servers_last_seen ON servers(last_seen);
CREATE INDEX IF NOT EXISTS idx_servers_country ON servers(country);
CREATE INDEX IF NOT EXISTS idx_servers_software ON servers(software);
CREATE INDEX IF NOT EXISTS idx_players_name ON players(name);
CREATE INDEX IF NOT EXISTS idx_players_uuid ON players(uuid);
CREATE INDEX IF NOT EXISTS idx_countries_network ON countries USING GIST(network inet_ops);
CREATE INDEX IF NOT EXISTS idx_server_visits_status ON server_visits(status);
CREATE INDEX IF NOT EXISTS idx_server_visits_visited_at ON server_visits(visited_at);