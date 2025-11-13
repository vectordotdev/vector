The `http_client` source now fails to start if VRL compilation errors occur in `query` parameters when
type is set to `vrl`, instead of silently logging a warning and continuing with invalid expressions.
This prevents unexpected behavior where malformed VRL would be sent as literal strings in HTTP requests.

authors: thomasqueirozb
