"""Local HTTP destination for the Vector debugging guide. Uses only Python's standard library."""

import argparse
import json
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, HTTPServer
import zlib


class DebuggingHTTPRequestHandler(BaseHTTPRequestHandler):
    def send_json(self, status, message):
        body = json.dumps({"message": message}).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if self.path not in ("/logs", "/set_status"):
            self.send_json(HTTPStatus.NOT_FOUND, "Use /logs or /set_status.")
            return

        try:
            content_length = int(self.headers.get("Content-Length", "0"))
            if content_length < 0:
                raise ValueError
        except ValueError:
            self.send_json(HTTPStatus.BAD_REQUEST, "Invalid Content-Length.")
            return

        body = self.rfile.read(content_length)
        encoding = self.headers.get("Content-Encoding", "identity")
        print(f"\nReceived POST {self.path}", flush=True)
        print(self.headers, end="", flush=True)

        if encoding == "deflate":
            try:
                body = zlib.decompress(body)
            except zlib.error:
                self.send_json(HTTPStatus.BAD_REQUEST, "Invalid zlib-compressed data.")
                return
        elif encoding != "identity":
            message = f"Unsupported compression: {encoding}. Use zlib or no compression."
            print(message, flush=True)
            self.send_json(HTTPStatus.BAD_REQUEST, message)
            return

        if (
            self.path == "/logs"
            and self.server.max_bytes is not None
            and len(body) > self.server.max_bytes
        ):
            message = (
                f"Payload is {len(body)} bytes; limit is {self.server.max_bytes} bytes "
                "after decompression."
            )
            print(message, flush=True)
            self.send_json(HTTPStatus.REQUEST_ENTITY_TOO_LARGE, message)
            return

        try:
            data = json.loads(body)
        except (json.JSONDecodeError, UnicodeDecodeError):
            self.send_json(HTTPStatus.BAD_REQUEST, "Expected a JSON payload.")
            return

        if self.path == "/set_status":
            status = data.get("status") if isinstance(data, dict) else None
            if type(status) is not int or not 200 <= status <= 599:
                self.send_json(
                    HTTPStatus.BAD_REQUEST,
                    "Send a JSON object with an integer status between 200 and 599.",
                )
                return
            self.server.response_code = status
            self.send_json(HTTPStatus.OK, f"Response code updated to {status}.")
            return

        print("JSON payload:", json.dumps(data, indent=2), flush=True)
        self.send_json(self.server.response_code, f"Responding with {self.server.response_code}.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--max-bytes",
        type=int,
        help="Reject /logs payloads larger than this many bytes after decompression.",
    )
    args = parser.parse_args()
    if args.max_bytes is not None and args.max_bytes <= 0:
        parser.error("--max-bytes must be positive")

    with HTTPServer(("127.0.0.1", 8000), DebuggingHTTPRequestHandler) as server:
        server.response_code = HTTPStatus.OK
        server.max_bytes = args.max_bytes
        print("Serving on http://127.0.0.1:8000", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            print("\nServer shutting down.", flush=True)
