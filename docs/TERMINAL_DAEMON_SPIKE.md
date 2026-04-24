# Spike: daemon de terminal (Superset-style) no Tauri

Este documento resume uma possível evolução arquitetural, **não implementada**. Serve para alinhar requisitos se sessões PTY precisarem de sobreviver ao reinício do processo do app (como no [artigo da Superset](https://superset.sh/blog/terminal-daemon-deep-dive)).

## Estado atual (DCC)

- PTY e leitor de saída vivem no processo Tauri (`src-tauri/src/main.rs`).
- O front recebe `terminal-output` em lotes (~30fps ou flush em leituras parciais) para reduzir pressão no IPC.
- Sessões por `paneId` persistem na memória do processo; fechar o app encerra os PTY.

## Direção do spike

1. **Processo sidecar** (binário Rust separado ou serviço) que:
   - Mantém mapa `ptyId → sessão` e sockets/pipes para o app.
   - Opcional: emulador headless (xterm.js em Node ou parser de estado mínimo) para snapshots ao reconectar.

2. **Protocolo** entre app e sidecar:
   - Mensagens versionadas (ex.: NDJSON ou bincode + framing).
   - Dois canais ou filas independentes: **stream de dados** vs **comandos/RPC**, para evitar *head-of-line blocking* quando há flood de stdout.

3. **Persistência**: metadados e histórico em disco para *cold restore* se o sidecar cair (reboot, `kill -9`).

4. **Tauri**: o binário principal torna-se cliente; `invoke` ou socket local substitui chamadas diretas ao PTY no mesmo processo.

## Critérios para priorizar

- Atualização frequente do app sem perder agentes a correr.
- Multi-janela partilhando as mesmas sessões.
- Requisitos de auditoria/replay de output offline.

Enquanto isso não for obrigatório, o modelo **PTY no processo Tauri + batching** permanece mais simples de operar e distribuir.
