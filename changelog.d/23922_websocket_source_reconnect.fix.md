Fixed the `websocket` source entering a "zombie" state when the `connect_timeout_secs` threshold was reached with multiple sources running. The connection timeout is now applied per connect attempt with indefinite retries, rather than as a total timeout limit.

authors: benjamin-awd
