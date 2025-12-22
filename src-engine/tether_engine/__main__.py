"""Entry point for tether-engine server.

Usage:
    python -m tether_engine [port]

    port: Optional port number (default: 8765)
"""

import sys
import uvicorn


def main():
    """Start the Tether engine server."""
    # Parse port from command line arguments
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765

    print(f"Starting Tether engine server on port {port}...")

    # Import the app from server module
    from tether_engine.server import app

    # Run the server
    uvicorn.run(app, host="127.0.0.1", port=port, log_level="info")


if __name__ == "__main__":
    main()
