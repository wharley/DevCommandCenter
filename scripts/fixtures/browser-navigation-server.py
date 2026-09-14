#!/usr/bin/env python3
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import sys


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        redirects = {
            "/start": "/main",
            "/frame-redirect": "/frame-final",
        }
        if path in redirects:
            self.send_response(302)
            self.send_header("Location", redirects[path])
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if path.startswith("/frame"):
            html = "<!doctype html><title>Embedded frame</title><p>Auxiliary content</p>"
        else:
            html = """<!doctype html><title>Main document</title>
<h1>Navigation fixture</h1><p id="result">Ready</p>
<button id="spa" onclick="history.pushState({}, '', '/profile');
document.querySelector('#result').textContent='Profile'">Open profile</button>
<button id="frame" onclick="const f=document.createElement('iframe');
f.src='/frame-redirect';document.body.append(f)">Load auxiliary frame</button>
<a href="/next">Next document</a><iframe src="/frame-initial"></iframe>"""
        body = html.encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


if __name__ == "__main__":
    server = ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1]) if len(sys.argv) > 1 else 0), Handler)
    print(server.server_address[1], flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
