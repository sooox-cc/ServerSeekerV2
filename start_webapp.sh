#!/bin/bash

echo "🎮 ServerSeekerV2 Web Dashboard"
echo "================================"
echo ""
echo "Starting the web application..."
echo ""

cd webapp

# Check if PostgreSQL container is running
if ! docker ps | grep -q postgres; then
    echo "⚠️  PostgreSQL container not running. Starting it..."
    docker start postgres
    sleep 2
fi

# Start the web server
echo "🚀 Starting web server on http://127.0.0.1:3000"
echo ""
echo "Features available:"
echo "✅ View all discovered Minecraft servers"  
echo "✅ Track server visit status and notes"
echo "✅ Add ratings and detailed server information"
echo "✅ Filter servers by software, players, country, etc."
echo "✅ Export and manage server database"
echo ""
echo "📖 Note: Scanning is done via command-line (see SCAN_GUIDE.md)"
echo "   Web interface is for viewing and managing discovered servers"
echo ""
echo "Press Ctrl+C to stop the server"
echo ""

cargo run
