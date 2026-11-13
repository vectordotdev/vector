AWS components now preserve request-body size hints, end-of-stream state, and trailers. This avoids unnecessary HTTP/1.1 chunked encoding for known-length bodies without an explicit `Content-Length` header and removes redundant empty HTTP/2 DATA frames, saving nine bytes per affected HTTP/2 request.

authors: thomasqueirozb
