#!/usr/bin/env python3
"""A TCP listener that records every connection attempt; the hostile pages must never reach it."""
import socketserver
import sys


class Handler(socketserver.BaseRequestHandler):
    def handle(self):
        first = self.request.recv(200).split(b"\r\n", 1)[0].decode("latin-1", "replace")
        with open(sys.argv[2], "a", encoding="utf-8") as log:
            log.write(first or "connect\n")
            log.write("\n")


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True


with Server(("127.0.0.1", int(sys.argv[1])), Handler) as server:
    server.serve_forever()
