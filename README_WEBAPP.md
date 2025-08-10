# ServerSeekerV2 Web Dashboard

A complete web application for tracking and managing Minecraft server discoveries. This web interface provides a user-friendly way to view, organize, and manage servers found through ServerSeekerV2's command-line scanning tools.

## Features

### Server Discovery & Management
- **View all discovered servers** with detailed information (software, version, players, etc.)
- **Track visit status** - mark servers as visited/skipped/whitelisted
- **Add notes and ratings** (1-5 stars) to servers you've explored
- **Advanced filtering** by software type, player count, country, visit status

### Server Database Management
- **Comprehensive server database** - View all servers discovered through command-line scanning
- **Real-time statistics** showing total servers, visited counts, and discovery progress
- **Data import/export** capabilities for server collections

### Web Interface
- **Modern responsive design** using Tailwind CSS
- **Real-time updates** using Alpine.js for dynamic interactions  
- **Easy server management** with copy-to-clipboard functionality
- **Statistics dashboard** with visual metrics

## Prerequisites

- ✅ PostgreSQL running in Docker (port 5433)
- ✅ ServerSeekerV2 scanner compiled and working (for command-line scanning)
- ✅ Rust and Cargo installed

## Quick Start

### 1. Start the Web Application
```bash
cd /path/to/ServerSeekerV2
./start_webapp.sh
```

### 2. Access the Dashboard
Open your browser and go to: **http://127.0.0.1:3000**

### 3. Managing Your Server Database
- Use the filters to find servers you're interested in
- Click **"Mark Visited"** after joining a server
- Add notes and ratings to track your experiences
- Export server data for external use

## Technical Architecture

### Backend (Rust + Axum)
```
webapp/src/main.rs - Web API server
├── GET  /api/servers - List servers with filtering
├── POST /api/servers/:ip/:port/visit - Mark server as visited
├── PUT  /api/servers/:ip/:port/visit - Update visit details
└── GET  /api/stats - Get discovery statistics
```

### Database Schema
```sql
servers        - Main server data (from ServerSeekerV2)
server_visits  - Visit tracking with notes/ratings
countries      - Geographic data for servers
```

### Frontend (HTML + Alpine.js)
```
webapp/static/index.html - Single-page application
├── Server list with real-time filtering
├── Statistics dashboard
├── Visit tracking modal
└── Server management tools
```

## Usage Examples

### Viewing Discovered Servers
1. **Browse servers**: View all servers discovered through command-line scans
2. **Filter results**: Use dropdowns to filter by software, players, etc.
3. **Copy server addresses**: Click "📋 Copy" to get the IP:port

### Tracking Server Visits
1. **Mark as visited**: Click "Mark Visited" after joining a server
2. **Add details**: Click "Edit Visit" to add notes and ratings
3. **Filter visited**: Use "Visited Only" filter to see visit history

### Managing Your Collection
- **View stats**: Dashboard shows total discovered vs visited servers
- **Export data**: All data stored in PostgreSQL for easy export
- **Search & filter**: Find specific types of servers quickly

## Current Statistics (Example)

After initial setup and range scanning:
- **156 total servers discovered**
- **6 different software types** (Paper, Purpur, Java, Lexforge, etc.)
- **Multiple countries/regions** represented
- **Continuous updates** from command-line scanning operations

## 🔧 Development & Customization

### API Endpoints for Custom Integrations
```python
import requests

# Get server statistics  
stats = requests.get("http://127.0.0.1:3000/api/stats").json()

# Get filtered server list
servers = requests.get("http://127.0.0.1:3000/api/servers?software=Paper").json()

# Mark server as visited with notes
requests.post("http://127.0.0.1:3000/api/servers/1.2.3.4/25565/visit", 
              json={"notes": "Great server!", "rating": 5})
```

### Extending the Web Dashboard
The web interface can be customized to:
- Add new filtering options
- Implement custom server categorization
- Create data visualization tools
- Export server data in different formats

## Future Enhancements

Potential features to add:
- **Discord notifications** for new server discoveries
- **Advanced analytics** and server trend tracking
- **Automated join testing** and server validation
- **Geographic mapping** of server locations
- **Server categorization** and tagging system
