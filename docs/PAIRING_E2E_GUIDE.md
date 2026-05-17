# Guia E2E — pareamento celular ↔ desktop

Status: 2026-05-16 · Pré-requisitos: macOS, rust toolchain, yarn, Node, repo
clonado em `~/projetos/DevCommandCenter` e landing em `~/projetos/dev-command-center`.

Este guia te leva do build até validar todas as defesas (lockout, rate limit,
revogação, replay, session expiry).

---

## 1. Preparação

### 1.1 Rebuildar os binários Rust

Garante que os endpoints novos (`/auth/pair-init`, `/auth/pair`,
`/auth/devices`, middleware signed-request) estão no binário e que o schema
SQLite tem as colunas `failed_attempts` + `locked_at`.

```bash
cd ~/projetos/DevCommandCenter/src-tauri
cargo build --bin dccd-http
cargo test --lib                    # 26 verdes
```

Se o app desktop já estiver aberto, mate-o (Cmd+Q) — vamos rodar em modo dev.

### 1.2 Iniciar a landing Next.js

A página `/m/pair` lê o fragmento do QR e fala com o backend.

```bash
cd ~/projetos/dev-command-center
yarn install                        # primeira vez
yarn dev                            # → http://localhost:3000
```

Verifique no browser:

- `http://localhost:3000/m/pair` → tela "QR não foi reconhecido" (esperado
  sem fragmento)
- `http://localhost:3000/m` → tela "Nenhum dispositivo pareado"

### 1.3 Apontar o QR do desktop para a landing local

Por default o QR codifica `https://www.devcommandcenter.com/...`. Para teste
local, exporte a variável antes de iniciar o app desktop:

```bash
cd ~/projetos/DevCommandCenter
export VITE_PAIR_LANDING_URL=http://localhost:3000
yarn tauri dev
```

> ⚠️ Para teste em celular real, troque `localhost` pelo IP LAN do mac:
> `export VITE_PAIR_LANDING_URL=http://192.168.x.x:3000`. Veja o IP com
> `ipconfig getifaddr en0`.

---

## 2. Cenário A — Test rápido pelo browser do desktop

Esse cenário valida tudo (crypto, signed requests, revogação) sem precisar
de celular real. O navegador do mac vai fazer o papel do celular.

### 2.1 Iniciar o pareamento

1. No app DCC → ⚙️ **Settings** → **Connections**
2. Role até **Dispositivos pareados**
3. Clique **Parear novo dispositivo**
4. Modal abre com QR code grande e PIN de 6 dígitos (ex: `483 251`)
5. Anote o PIN. Countdown deve estar contando de 60s

### 2.2 Copiar o link do QR

Clique no botão **Copiar link do pareamento**. Vai pro clipboard algo como:

```
http://localhost:3000/m/pair#be=http%3A%2F%2F127.0.0.1%3A9876&nonce=ABC123...
```

### 2.3 Abrir no browser (modo anônimo recomendado)

1. Cole o link numa janela anônima (Cmd+Shift+N)
2. A página `/m/pair` deve carregar com:
   - Backend em `127.0.0.1:9876` no topo
   - Campo "Nome do dispositivo" pré-preenchido com `Mac`
   - Input de PIN com 6 quadradinhos

### 2.4 Completar o pareamento

1. Digite o PIN exibido no modal do desktop
2. Clique **Parear**
3. Sucesso → "Pareado ✓ Mac foi vinculado…"
4. Redirect automático para `/m?device=<uuid>`

No desktop, simultaneamente:

- **Toast verde**: "Novo dispositivo pareado — Mozilla/5.0…"
- Modal pode ser fechado (Fechar)
- Lista de dispositivos agora mostra "Mac" com último uso "agora"

### 2.5 Verificar a sessão mobile

Na aba anônima em `/m`:

- Card "Dispositivo" com nome, backend, data
- Card "Status" puxando de `/api/v1/status` via signed-fetch
- Métricas: Panes / Trabalhando / Aguardando

Abra o devtools → Network e veja os headers em qualquer request:

```
X-Device-Id:  <uuid>
X-Timestamp:  2026-05-16T14:23:01.234Z
X-Signature:  <base64 ECDSA-DER>
```

---

## 3. Cenário B — Test em celular real

Pré-requisito: o celular precisa estar na **mesma rede WiFi** do mac, e o
`dccd-http` precisa estar bindado em uma interface de rede (não só em
loopback). Por default ele só escuta 127.0.0.1.

### 3.1 Permitir bind em todas as interfaces

```bash
# parar instância atual
pkill -f dccd-http

# subir em 0.0.0.0
DCC_HTTP_HOST=0.0.0.0 \
DCC_HTTP_API_KEY=dev-key-change-me \
~/projetos/DevCommandCenter/target/debug/dccd-http &
```

### 3.2 Ajustar o `defaultBackendUrl` no Settings

Atualmente o componente passa `defaultBackendUrl="http://127.0.0.1:9876"`.
Para teste em celular, edite temporariamente em
`apps/desktop/src/features/settings/SettingsDialog.tsx`:

```tsx
<PairedDevicesPanel defaultBackendUrl="http://192.168.x.x:9876" />
```

> Use o IP que `ipconfig getifaddr en0` retornar.

E também `VITE_PAIR_LANDING_URL=http://192.168.x.x:3000` antes do
`yarn tauri dev`.

### 3.3 Escanear pelo celular

1. App nativo de câmera do iPhone/Android reconhece o QR
2. Toque na URL que aparece
3. Safari/Chrome abre `/m/pair`
4. Digite o PIN do desktop
5. Pareado ✓

---

## 4. Verificar as defesas

### 4.1 PIN incorreto → lockout depois de 5 tentativas

1. Gere um novo pareamento (Settings → Parear novo dispositivo)
2. Anote o nonce no link copiado
3. Pelo `curl` ou pelo browser, envie 5 PINs errados:

```bash
# extraia o nonce do link copiado (parte depois de nonce=)
NONCE="cole-aqui"
for i in 1 2 3 4 5; do
  curl -s -X POST http://localhost:9876/auth/pair \
    -H "Content-Type: application/json" \
    -d "{\"nonce\":\"$NONCE\",\"pin\":\"000000\",\"publicKeySpki\":\"MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE+++++\",\"deviceName\":\"attacker\"}" \
    | python3 -m json.tool
  echo
done
```

Esperado:

- Tentativas 1-4: `{"error":{"code":"bad_request","message":"invalid pairing credentials"}}`
- Tentativa 5: `{"error":{"code":"bad_request","message":"too many invalid PIN attempts"}}`
- Mesmo o PIN correto agora retorna `too many invalid PIN attempts`

No desktop, deve aparecer um **toast amarelo de warning**:
> "Tentativa de brute-force detectada — Nonce travado para IP …"

### 4.2 Rate limit `/auth/pair` (20 req / 60s por IP)

```bash
for i in $(seq 1 25); do
  curl -s -X POST http://localhost:9876/auth/pair \
    -H "Content-Type: application/json" \
    -d '{"nonce":"x","pin":"0","publicKeySpki":"x","deviceName":"x"}' \
    -o /dev/null -w "%{http_code}\n"
done
```

Esperado:

- Primeiras 20: `400` (credenciais inválidas — mas a request passou)
- 21+: `400` com mensagem `rate limit exceeded for /auth/pair`

Espere 60s e tente de novo — volta a aceitar.

### 4.3 Revogação imediata

1. No celular (ou aba anônima), com sessão `/m` aberta, deixe o card
   "Status" carregando
2. No desktop, abra Settings → Dispositivos pareados
3. Clique **Revogar** na linha do device

No celular, a próxima request:

- Card "Status" mostra `HTTP 403`
- Mensagem do middleware: `Device has been revoked`

### 4.4 Tampering de body (integridade da assinatura)

```bash
# pega um device pareado e tenta forjar
curl -v -X POST http://localhost:9876/api/v1/status \
  -H "X-Device-Id: id-do-device" \
  -H "X-Timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  -H "X-Signature: AAAAAAAA" \
  -H "Content-Type: application/json" \
  -d '{"forged":true}'
```

Esperado: `403 Forbidden` com `Signature verification failed`.

### 4.5 Replay com timestamp velho

Pegue uma assinatura legítima do DevTools do `/m` (qualquer request) e
re-envie com `X-Timestamp` mais que 60s no passado:

```bash
OLD_TS="2025-01-01T00:00:00Z"
curl -v -X GET http://localhost:9876/api/v1/status \
  -H "X-Device-Id: id-real" \
  -H "X-Timestamp: $OLD_TS" \
  -H "X-Signature: sig-real"
```

Esperado: `403` com `Timestamp outside the replay window`.

### 4.6 Limite de devices

1. Pareie 10 devices (pode ser 10 abas anônimas do browser)
2. Tente parear o 11º
3. Resposta: `device limit reached; revoke an existing device first`
4. Revogue qualquer um → o próximo pareamento volta a funcionar

### 4.7 Session expiry (30 dias)

Difícil de testar sem mocar a data. Para forçar manualmente:

```bash
sqlite3 ~/Library/Application\ Support/com.devcommandcenter.app/database.sqlite <<SQL
UPDATE paired_devices
SET created_at = datetime('now','-31 days')
WHERE device_id = 'id-do-device';
SQL
```

Próxima request do celular: `403 Device session expired; please re-pair`.

---

## 5. Audit log

Em Settings → Dispositivos pareados → **Ver log de auditoria de pareamentos →**

Deve listar (do mais recente pro mais antigo):

| Evento | Ícone | Quando aparece |
|---|---|---|
| Pareamento concluído | ✓ verde | sucesso em `/auth/pair` |
| Dispositivo revogado | ✕ âmbar | usuário clicou Revogar |
| PIN incorreto | ⚠ âmbar | PIN errado submetido |
| Nonce travado por brute-force | ⚠ vermelho | 5 PINs errados consecutivos |

Cada linha mostra device-id (8 chars iniciais), IP e user-agent.

---

## 6. Troubleshooting

### "QR não foi reconhecido" no celular

- O link tinha `#nonce=...` mas o parser não achou
- Verifique se a URL ficou completa (sem corte ao copiar)
- Confirme que `parsePairFragment` aceita o formato (`be` ou `backend`, `nonce` ou `n`)

### "Não consegui falar com o Desktop"

- Backend URL aponta para `127.0.0.1` mas você está em outro device
- Use IP LAN: `export VITE_PAIR_LANDING_URL=http://192.168.x.x:3000` + ajuste
  `defaultBackendUrl` no `SettingsDialog.tsx`

### "O backend está em HTTP" (banner amarelo no celular)

- Landing está em HTTPS (produção) mas backend é HTTP
- Solução: abra o link direto pelo backend (`http://LAN-IP:9876/m/pair#…`),
  ou exponha o backend via Tailscale Serve

### Toast de "Novo dispositivo pareado" não aparece

- O watcher (poll de 3s) precisa do app desktop rodando — confira que
  `yarn tauri dev` está vivo
- O evento é emitido só para entradas com id maior que o MAX(id) inicial,
  então um pair que aconteceu DURANTE o startup não dispara

### `cargo test` falha com "no such table"

- Schema mudou — rode `cargo clean` + `cargo build --bin dccd-http` para
  refazer o schema migration

### `404 Not Found` em `/auth/pair-init`

- O `dccd-http` que está respondendo é uma versão antiga (do app oficial
  instalado em `/Applications/`)
- Mate todos: `pkill -f dccd-http`
- Suba a versão de dev pela porta certa

---

## 7. Checklist de validação

Use isso pra confirmar que tudo está OK:

- [ ] `cargo test --lib` retorna `26 passed`
- [ ] `yarn vite:build` na raiz do DCC passa sem erros
- [ ] `yarn build` na landing passa sem erros TS
- [ ] Settings mostra a seção "Dispositivos pareados"
- [ ] Modal de pareamento mostra QR + PIN de 6 dígitos + countdown
- [ ] Browser anônimo no `/m/pair` consegue parear com PIN correto
- [ ] Toast verde aparece no desktop após pareamento
- [ ] Lista de devices reflete o novo paramento
- [ ] `/m` mostra status do daemon via signed-fetch
- [ ] PIN errado x5 trava o nonce + dispara toast amarelo
- [ ] Revogar pelo desktop deixa o celular instantaneamente em `403`
- [ ] Limite de 10 devices é respeitado
- [ ] Audit log mostra todos os eventos com ícones distintos

Se todos os 13 estão ✓: o ciclo end-to-end está saudável. 🎉
