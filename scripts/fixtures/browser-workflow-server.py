#!/usr/bin/env python3
"""Local Browser consent/login fixture. No real accounts or external traffic.

Run with Python 3 and open http://127.0.0.1:47831/ through a DCC agent request.
The initial 403 and HttpOnly cookie are intentional. Stop with Ctrl-C.
"""
from http.server import BaseHTTPRequestHandler, HTTPServer


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_GET(self):
        signed_in = "dcc_fixture_login=yes" in self.headers.get("Cookie", "")
        if self.path == "/sign-in":
            self.send_response(303)
            self.send_header("Set-Cookie", "dcc_fixture_login=yes; HttpOnly; SameSite=Strict; Path=/; Max-Age=3600")
            self.send_header("Location", "/")
            self.end_headers()
            return

        self.send_response(200 if signed_in else 403)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.end_headers()
        html = '''<!doctype html><title>DCC Browser workflow fixture</title>
<body style="font:20px system-ui;padding:32px;background:#f6f8fa;color:#112">
<h1>Teste do Browser DCC</h1>'''
        if not signed_in:
            html += '''<p>Acesso restrito — fixture local, sem contas reais.</p>
<a href="/sign-in" style="display:inline-block;padding:16px;background:#bde">Entrar na conta de teste</a>'''
        else:
            html += '''<p>Sessão de teste autenticada</p>
<label>Nome <input aria-label="Nome" id="name"></label>
<div role="button" tabindex="0" aria-label="Salvar"
 onclick="document.querySelector('#result').textContent='Validado: '+document.querySelector('#name').value"
 style="padding:16px;background:#bde;margin-top:20px">Salvar</div>
<p id="result">Aguardando validação</p><div style="height:1000px"></div>
<p>Fim da página de teste</p>'''
        self.wfile.write(html.encode())


if __name__ == "__main__":
    try:
        HTTPServer(("127.0.0.1", 47831), Handler).serve_forever()
    except KeyboardInterrupt:
        pass
