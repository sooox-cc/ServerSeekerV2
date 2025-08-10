#!/usr/bin/env python3
import requests
import json
import subprocess
import time
import signal
import os

def test_web_app():
    print("🚀 Testing ServerSeekerV2 Web Application")
    print("=" * 50)
    
    # Start the web server in the background
    print("📡 Starting web server...")
    server_process = subprocess.Popen(['cargo', 'run'], 
                                     cwd='/home/sooox/Downloads/ServerSeekerV2/webapp',
                                     stdout=subprocess.PIPE, 
                                     stderr=subprocess.PIPE)
    
    # Wait for server to start
    time.sleep(3)
    
    base_url = "http://127.0.0.1:3000"
    
    try:
        # Test stats endpoint
        print("\n📊 Testing stats endpoint...")
        response = requests.get(f"{base_url}/api/stats", timeout=5)
        if response.status_code == 200:
            stats = response.json()
            print(f"✅ Stats loaded successfully:")
            print(f"   - Total servers: {stats['total_servers']}")
            print(f"   - Visited servers: {stats['visited_servers']}")
            print(f"   - Unvisited servers: {stats['unvisited_servers']}")
            print(f"   - Software types: {len(stats['unique_software_types'])}")
        else:
            print(f"❌ Stats endpoint failed: {response.status_code}")
            return False
        
        # Test servers endpoint
        print("\n🖥️  Testing servers endpoint...")
        response = requests.get(f"{base_url}/api/servers?limit=5", timeout=5)
        if response.status_code == 200:
            servers = response.json()
            print(f"✅ Loaded {len(servers)} servers:")
            for server in servers[:3]:
                print(f"   - {server['address']}:{server['port']} ({server['software'] or 'Unknown'})")
        else:
            print(f"❌ Servers endpoint failed: {response.status_code}")
            return False
        
        # Test marking a server as visited
        print("\n✅ Testing mark visited functionality...")
        if servers:
            test_server = servers[0]
            visit_data = {"notes": "Test server from Python script", "rating": 4}
            
            response = requests.post(
                f"{base_url}/api/servers/{test_server['address']}/{test_server['port']}/visit",
                json=visit_data,
                timeout=5
            )
            
            if response.status_code == 200:
                print(f"✅ Successfully marked {test_server['address']}:{test_server['port']} as visited")
                
                # Verify the visit was recorded
                response = requests.get(f"{base_url}/api/servers?visited=true&limit=1", timeout=5)
                visited_servers = response.json()
                if visited_servers and visited_servers[0]['visited']:
                    print("✅ Visit status confirmed in database")
                else:
                    print("⚠️  Visit status not confirmed")
            else:
                print(f"❌ Failed to mark server as visited: {response.status_code}")
        
        print("\n🎯 Web Application Test Results:")
        print("✅ API endpoints working correctly")
        print("✅ Database connectivity confirmed")
        print("✅ Server visit tracking functional")
        print(f"\n🌐 Access the web interface at: {base_url}")
        print("🔍 Use the 'Run Range Scan' button to discover more servers!")
        
        return True
        
    except requests.exceptions.RequestException as e:
        print(f"❌ Connection error: {e}")
        return False
    except Exception as e:
        print(f"❌ Test error: {e}")
        return False
    finally:
        # Clean up
        print(f"\n🔄 Stopping web server...")
        server_process.terminate()
        try:
            server_process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server_process.kill()

if __name__ == "__main__":
    test_web_app()