# Guia HTTP API do DCC

O binário `dccd-http` expõe uma fachada REST sobre o daemon local do Dev Command Center.

## Arranque

Por omissão, o servidor lê a configuração em `~/.dcc/http-config.json` e variáveis de ambiente:

- `DCC_HTTP_ENABLED`
- `DCC_HTTP_HOST`
- `DCC_HTTP_PORT`
- `DCC_HTTP_API_KEY`
- `DCC_HTTP_AUTH_MODE` (`local`, `remote`, `mixed`)
- `DCC_HTTP_BEARER_TOKEN`
- `DCC_HTTP_BEARER_TOKEN_EXPIRES_AT`
- `DCC_HTTP_BEARER_TOKEN_TTL_SECONDS`
- `DCC_HTTP_BEARER_TOKEN_PREVIOUS`
- `DCC_HTTP_BEARER_TOKEN_PREVIOUS_EXPIRES_AT`
- `DCC_HTTP_BEARER_TOKEN_GRACE_SECONDS`
- `DCC_HTTP_DB_PATH`
- `DCC_HTTP_CORS_ORIGINS`

Exemplo de arranque:

```bash
DCC_HTTP_API_KEY="dev-key" \
DCC_HTTP_DB_PATH="$HOME/.local/share/com.devcommandcenter.app/database.sqlite" \
cargo run --manifest-path src-tauri/Cargo.toml --bin dccd-http
```

## Autenticação

Os endpoints protegidos exigem um dos headers, de acordo com o modo configurado:

```http
X-API-Key: <token>
```

```http
Authorization: Bearer <token>
```

Modos:

- `local`: aceita apenas `X-API-Key`
- `remote`: aceita apenas `Authorization: Bearer`
- `mixed`: aceita ambos durante transição

## Endpoints públicos

- `GET /`
- `GET /health`
- `GET /openapi.json`

## Endpoints REST

- `GET /api/v1/status`
- `GET /api/v1/tasks`
- `POST /api/v1/tasks/{taskId}/run?projectId=<id>`
- `POST /api/v1/tasks/{taskId}/attach?projectId=<id>`
- `DELETE /api/v1/tasks/{taskId}/attach?projectId=<id>`
- `GET /api/v1/processes?projectId=<id>`
- `POST /api/v1/processes/{processId}/start?projectId=<id>`
- `POST /api/v1/processes/{processId}/stop?projectId=<id>`
- `POST /api/v1/processes/{processId}/restart?projectId=<id>`
- `GET /api/v1/combs?projectId=<id>`
- `GET /api/v1/panes?projectId=<id>&combId=<id>`
- `POST /api/v1/diffs/bundle`
- `POST /api/v1/auth/bearer/rotate`
- `POST /rpc` para compatibilidade com o contrato RPC existente

## Exemplos

Listar tasks:

```bash
curl -s \
  -H "X-API-Key: dev-key" \
  http://127.0.0.1:9876/api/v1/tasks
```

Executar uma task:

```bash
curl -s \
  -X POST \
  -H "X-API-Key: dev-key" \
  "http://127.0.0.1:9876/api/v1/tasks/build/run?projectId=my-project"
```

Listar processos:

```bash
curl -s \
  -H "X-API-Key: dev-key" \
  "http://127.0.0.1:9876/api/v1/processes?projectId=my-project"
```

Bundling de diffs:

```bash
curl -s \
  -X POST \
  -H "Content-Type: application/json" \
  -H "X-API-Key: dev-key" \
  -d '{"worktreePaths":["/tmp/worktree-a"],"combIds":["comb-1"]}' \
  http://127.0.0.1:9876/api/v1/diffs/bundle
```

Rotação do bearer token:

```bash
curl -s \
  -X POST \
  -H "X-API-Key: dev-key" \
  -H "Content-Type: application/json" \
  -d '{"ttlSeconds":7200,"graceSeconds":300}' \
  http://127.0.0.1:9876/api/v1/auth/bearer/rotate
```

## Notas de contrato

- A API REST é uma fachada sobre o mesmo armazenamento SQLite usado pelo daemon.
- Se o daemon não responder, `GET /health` retorna `503`.
- O `GET /openapi.json` adapta a segurança ao modo ativo (`local`, `remote` ou `mixed`).
- `GET /openapi.json` descreve os endpoints suportados pela versão atual.
