#!/usr/bin/env python3
"""HTTP receiver used by the vector#24125 repro.

Returns 200 until FAIL_AFTER seconds have elapsed since startup, then 429.
Env vars:
  PORT        listen port (default 8001)
  FAIL_AFTER  seconds before flipping to 429; default infinity (always 200)
"""
import http.server
import os
import socketserver
import time

PORT = int(os.environ.get("PORT", "8001"))
FAIL_AFTER = float(os.environ.get("FAIL_AFTER", "inf"))
START = time.time()


class Handler(http.server.BaseHTTPRequestHandler):
    def _h(self):
        try:
            ln = int(self.headers.get("Content-Length", "0") or "0")
            if ln > 0:
                self.rfile.read(ln)
        except Exception:
            pass
        code = 429 if (time.time() - START) > FAIL_AFTER else 200
        self.send_response(code)
        self.send_header("Content-Length", "0")
        self.end_headers()

    do_GET = do_POST = do_PUT = _h

    def log_message(self, *a, **kw):
        pass


print(f"receiver listening on {PORT} (fail after {FAIL_AFTER}s)", flush=True)
with socketserver.ThreadingTCPServer(("", PORT), Handler) as srv:
    srv.serve_forever()
