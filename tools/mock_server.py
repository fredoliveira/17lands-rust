#!/usr/bin/env python3
"""Mock 17Lands API server for local development (never POST to the live API in dev).

Captures every POST the client makes (decompressing gzip), appending one JSON line
`{"endpoint": "...", "body": <parsed json>}` per request to the output file. Answers the
startup version-check GET so upstream-compatible clients proceed, and 200s everything else.

Usage: mock_server.py <port> <output.jsonl>
"""
import gzip
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer


def make_handler(out_path):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass  # silence

        def do_GET(self):
            # verify_version() expects a min_version it can compare against; 0.0.0 always passes.
            body = json.dumps({"min_version": "0.0.0"}).encode("utf8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            length = int(self.headers.get("Content-Length", 0))
            raw = self.rfile.read(length)
            if self.headers.get("Content-Encoding") == "gzip":
                raw = gzip.decompress(raw)
            # endpoint without leading slash, to match the Rust RecordingSubmitter.
            endpoint = self.path.lstrip("/")
            record = {"endpoint": endpoint, "body": json.loads(raw.decode("utf8"))}
            with open(out_path, "a") as f:
                f.write(json.dumps(record) + "\n")
            resp = b"{}"
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(resp)))
            self.end_headers()
            self.wfile.write(resp)

    return Handler


def main():
    port = int(sys.argv[1])
    out_path = sys.argv[2]
    # truncate the output file
    open(out_path, "w").close()
    server = HTTPServer(("127.0.0.1", port), make_handler(out_path))
    server.serve_forever()


if __name__ == "__main__":
    main()
