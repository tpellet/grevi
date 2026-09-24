#!/usr/bin/env python3
"""A classifier.dev stand-in on 127.0.0.1 that answers every POST with one
status, so the ceiling's failure path can be timed without spending quota.

    python3 stub.py <port> <status> [retry_after_seconds]
"""
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, STATUS = int(sys.argv[1]), int(sys.argv[2])
RETRY_AFTER = sys.argv[3] if len(sys.argv) > 3 else None


class H(BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers.get("content-length", 0) or 0))
        body = b'{"error":"rate_limit","code":"rate_limit_exceeded"}'
        self.send_response(STATUS)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        if RETRY_AFTER:
            self.send_header("retry-after", RETRY_AFTER)
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        pass


ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
