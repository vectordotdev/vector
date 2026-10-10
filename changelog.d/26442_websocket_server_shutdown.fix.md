Fix the `websocket_server` sink leaving its listener and client connections running after shutdown, which could prevent the port from being reused during configuration reloads.

authors: U-S-jun
