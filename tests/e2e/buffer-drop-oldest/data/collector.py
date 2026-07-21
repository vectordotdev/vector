import json
import os
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


DELAY_SECONDS = float(os.environ.get("DELAY_SECONDS", "2"))
IDS = []
LOCK = threading.Lock()


def collect_ids(value):
    if isinstance(value, dict):
        if isinstance(value.get("id"), int):
            yield value["id"]
        for child in value.values():
            yield from collect_ids(child)
    elif isinstance(value, list):
        for child in value:
            yield from collect_ids(child)


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self.send_response(200)
            self.end_headers()
            return

        if self.path == "/ids":
            with LOCK:
                body = json.dumps({"ids": IDS}).encode()
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return

        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        if self.path == "/reset":
            with LOCK:
                IDS.clear()
            self.send_response(200)
            self.end_headers()
            return

        length = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(length)
        try:
            payload = json.loads(body)
        except json.JSONDecodeError:
            payload = {}

        with LOCK:
            IDS.extend(collect_ids(payload))

        time.sleep(DELAY_SECONDS)
        self.send_response(200)
        self.end_headers()

    def log_message(self, format, *args):
        return


ThreadingHTTPServer(("0.0.0.0", 8081), Handler).serve_forever()
