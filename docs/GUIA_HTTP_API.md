# Guia HTTP API do DCC

O binário `dccd-http` expõe uma fachada REST sobre o daemon local do Dev Command Center.

## Arranque

Por omissão, o servidor lê a configuração em `~/.dcc/http-config.json` e variáveis de ambiente:

- `DCC_HTTP_ENABLED`
- `DCC_HTTP_HOST`
- `DCC_HTTP_PORT`
- `DCC_HTTP_API_KEY`
- `DCC_HTTP_DB_PATH`
- `DCC_HTTP_CORS_ORIGINS`

Exemplo de arranque:

```bash
DCC_HTTP_API_KEY="dev-key" \
DCC_HTTP_DB_PATH="$HOME/.local/share/com.devcommandcenter.app/database.sqlite" \
cargo run --manifest-path src-tauri/Cargo.toml --bin dccd-http
```

## Autenticação

Os endpoints protegidos exigem o header:

```http
X-API-Key: <token>
```

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

## Notas de contrato

- A API REST é uma fachada sobre o mesmo armazenamento SQLite usado pelo daemon.
- Se o daemon não responder, `GET /health` retorna `503`.
- `GET /openapi.json` descreve os endpoints suportados pela versão atual.
