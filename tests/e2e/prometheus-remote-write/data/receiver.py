import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


stats = {"requests": 0, "bytes": 0}


def write_stats():
    with open("/tmp/stats.json", "w", encoding="utf-8") as stats_file:
        json.dump(stats, stats_file)


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("content-length", "0"))
        if length:
            self.rfile.read(length)

        stats["requests"] += 1
        stats["bytes"] += length
        write_stats()

        self.send_response(204)
        self.end_headers()

    def do_GET(self):
        if self.path != "/stats":
            self.send_response(404)
            self.end_headers()
            return

        body = json.dumps(stats).encode("utf-8")
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        return


write_stats()
ThreadingHTTPServer(("0.0.0.0", 8080), Handler).serve_forever()
