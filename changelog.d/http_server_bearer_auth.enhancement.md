Added bearer authentication strategy to HTTP server sources. The `http_server`, `heroku_logs`, and `websocket_server` components now support `strategy = "bearer"` in their `auth` configuration, allowing token-based authentication via the `Authorization: Bearer <token>` header.

authors: steveduan-IDME
