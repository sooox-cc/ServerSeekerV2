# ServerSeekerV2 Scanning Guide

This guide provides instructions on how to configure and run various types of scans with ServerSeekerV2.

**Note:** All scanning functionality in ServerSeekerV2 is command-line only. The web application provides a read-only interface for viewing and managing discovered servers but does not include scanning capabilities.

## Scanning Modes

ServerSeekerV2 has four main scanning modes, which can be specified with the `--mode` command-line argument:

*   **`--mode discovery`:** Scans for new servers on the internet using `masscan`. This mode is highly configurable for both IP and port ranges.
*   **`--mode range-scanner`:** Scans for new servers in the subnets of servers you have already found. This is a more targeted way to find new servers.
*   **`--mode rescanner`:** Rescans all the servers currently in your database to update their information.
*   **`--mode geo-update`:** Updates geolocation information for existing servers in the database without rescanning the servers themselves.

## Configuration Files

There are two main configuration files that control how the scanner operates:

*   **`config.toml`**: The main configuration file for the application. It controls database connections, scanner settings (like timeouts and worker threads), and the path to the `masscan.conf` file.
*   **`masscan.conf`**: The configuration file for `masscan`. This is where you define the IP addresses and ports to scan when in `discovery` mode.

## How to Run Different Types of Scans

### 1. Discovery Scan (Finding New Servers)

This is the most powerful and flexible scanning mode. It uses `masscan` to scan the internet for new servers.

**Configuration:**

1.  **`config.toml`**:
    *   Ensure the `[database]` section is correctly configured with your PostgreSQL connection details.
    *   In the `[scanner]` section, you can adjust settings like `scan_workers` and `scan_timeout`.
    *   The `[masscan]` section should have the correct path to your `masscan.conf` file.

2.  **`masscan.conf`**:
    *   **`range`**: This is where you define the IP addresses to scan.
        *   **Specific IPs**: `range = 1.1.1.1, 2.2.2.2, 3.3.3.3`
        *   **IP Range**: `range = 192.168.1.0-192.168.1.255`
        *   **CIDR Notation**: `range = 10.0.0.0/8`
    *   **`ports`**: This is where you define the ports to scan.
        *   **Specific Port**: `ports = 25565`
        *   **Port Range**: `ports = 25565-25570`
        *   **Multiple Ports/Ranges**: `ports = 80,443,25565-25570`
    *   **`rate`**: The packet rate for the scan. A higher rate is faster but may be less reliable.

**To Run:**

```bash
./target/release/serverseekerv2 --mode discovery
```

### 2. Range Scan (Targeted Discovery)

This mode finds new servers by scanning the subnets of servers you have already discovered.

**Configuration:**

*   **`config.toml`**:
    *   The `[database]` section must be correctly configured, as this is where the scanner gets the list of known servers.
    *   The `[scanner]` section's `port_range_start` and `port_range_end` values determine which ports are scanned in the subnets.
*   **`masscan.conf`**: This file is **not** used by the range scanner.

**To Run:**

```bash
./target/release/serverseekerv2 --mode range-scanner
```

### 3. Rescan (Updating Existing Servers)

This mode updates the information for all the servers currently in your database.

**Configuration:**

*   **`config.toml`**:
    *   The `[database]` section must be correctly configured.
    *   The `[scanner]` section's `port_range_start` and `port_range_end` values determine which ports are rescanned for each server.
*   **`masscan.conf`**: This file is **not** used by the rescanner.

**To Run:**

```bash
./target/release/serverseekerv2 --mode rescanner
```

### 4. Geo Update (Update Location Information)

This mode updates the geographic location information (country, region, ISP, etc.) for servers already in your database without rescanning the servers themselves. This is useful when you want to refresh location data or when the geolocation database has been updated.

**Configuration:**

*   **`config.toml`**:
    *   The `[database]` section must be correctly configured to access the existing server database.
    *   Geolocation settings in the configuration determine the data source and accuracy.
*   **`masscan.conf`**: This file is **not** used by the geo-update mode.

**To Run:**

```bash
./target/release/serverseekerv2 --mode geo-update
```

**Note:** This mode processes existing servers in batches and updates their geographic information. It's particularly useful when:
- Setting up geolocation for the first time on an existing database
- Updating location data after importing servers from another source
- Refreshing geographic information periodically

## Summary of Configuration File Usage

| Mode              | `config.toml` | `masscan.conf` |
| ----------------- | :-----------: | :------------: |
| `discovery`       |      Yes      |      Yes       |
| `range-scanner`   |      Yes      |       No       |
| `rescanner`       |      Yes      |       No       |
| `geo-update`      |      Yes      |       No       |
