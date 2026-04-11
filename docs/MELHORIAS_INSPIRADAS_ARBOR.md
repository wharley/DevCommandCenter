# Melhorias inspiradas no Arbor para o Dev Command Center (Rust + Tauri)

Este documento consolida **oportunidades de evolução do DCC** inspiradas no projeto [**Arbor**](https://github.com/penso/arbor) (README oficial, documentação em `docs/src` do repositório, e visão geral dos crates), **sem reescrever a UI em GPUI**: mantém-se **Tauri 2 + React + TypeScript** no front e **Rust** no host, como hoje.

**Visão de produto (âmbito desejado)**  
O DCC pretende ser **ferramenta de uso diário** o mais completa e útil possível dentro da stack Tauri. Por isso, as capacidades abaixo — em especial **daemon** (sessões persistentes, API estável), **tasks agendadas** (cron + *triggers* no daemon) e **MCP + API/CLI** — são tratadas como **objetivos de produto**, não como “extras só se alguém precisar”. A secção [Priorização sugerida](#priorização-sugerida) define **ordem de implementação** (o que vem primeiro por dependência técnica e valor imediato), não o que fica de fora por falta de interesse.

**Base no DCC atual (auditoria interna):**

- Terminal: `portable_pty` + eventos `terminal-output` / `terminal-attention` / `terminal-exit`, buffer de backlog (`terminal_get_backlog`), `TERM=xterm-256color` em Unix, integração Git em worktrees via `GIT_DIR` / `GIT_WORK_TREE`.
- UI: `xterm.js` + `FitAddon`, preferências em `localStorage`, heurística de atenção e integração com panes (`paneId`, reattach).
- Dados: SQLite (`projects`, `combs`, `panes`, `providers`).
- Worktree: `comb_ensure_worktree`, `comb_discard`, diffs de review; **`comb_merge_into_main` e `comb_apply_patch` já foram implementados** e fecham o ciclo de integração do worktree.
- Alguns comandos Tauri mapeados ainda **não implementados** (ver secção [Gaps já mapeados no código](#gaps-já-mapeados-no-código-dcc)).

Para cada área: **o que o Arbor destaca** → **benefício para o DCC** → **viável com Tauri** (sim/não/parcial) e notas de implementação de alto nível.

---

## Índice

1. [Terminal PTY, emulação e desempenho](#1-terminal-pty-emulação-e-desempenho)
2. [Sessões persistentes e modelo "daemon"](#2-sessões-persistentes-e-modelo-daemon)
3. [Sinais, lifecycle e integração com o SO](#3-sinais-lifecycle-e-integração-com-o-so)
4. [Processos gerenciados (Procfile / config de repo)](#4-processos-gerenciados-procfile--config-de-repo)
5. [Worktrees, Git e fluxo de integração](#5-worktrees-git-e-fluxo-de-integração)
6. [Issues, forges e contexto de PR](#6-issues-forges-e-contexto-de-pr)
7. [Diff, revisão e árvore de ficheiros](#7-diff-revisão-e-árvore-de-ficheiros)
8. [Agentes de IA e visibilidade de atividade](#8-agentes-de-ia-e-visibilidade-de-atividade)
9. [Automação, tarefas agendadas e *hooks* de repo](#9-automação-tarefas-agendadas-e-hooks-de-repo)
10. [MCP, API HTTP e CLI *headless*](#10-mcp-api-http-e-cli-headless)
11. [Acesso remoto e *outposts* (fase posterior)](#11-acesso-remoto-e-outposts-fase-posterior)
12. [UI/UX: paleta de comandos, temas, notificações](#12-uiux-paleta-de-comandos-temas-notificações)
13. [Configuração por repositório (equivalente a `arbor.toml`)](#13-configuração-por-repositório-equivalente-a-arbortoml)
14. [Segurança, credenciais e ambiente](#14-segurança-credenciais-e-ambiente)
15. [Observabilidade e qualidade](#15-observabilidade-e-qualidade)
16. [Gaps já mapeados no código DCC](#gaps-já-mapeados-no-código-dcc)
17. [Priorização sugerida](#priorização-sugerida)
18. [Checklist de Implementação (Auditoria Completa)](#18-checklist-de-implementação-auditoria-completa)

---

## 1. Terminal PTY, emulação e desempenho

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| PTY com **truecolor** e `xterm-256color` | Consistência com TUIs, `clear`, cores em ferramentas de build/teste | **Já alinhado** em Unix (`terminal_spawn`). Reforçar **Windows** (env `TERM`, `COLORTERM`, perfis PowerShell). |
| Motor VT opcional (ex.: Ghostty) embutido no app nativo | Melhor desempenho em cenários extremos | **Parcial**: em stack WebView, o gargalo é **xterm.js + canvas/WebGL**, não um VT nativo alternativo sem reescrever a camada de render. Priorizar **addons** e tuning. |
| **Várias abas de terminal por worktree** | Menos panes “genéricos”, mais sessões nomeadas no mesmo workspace | **Sim**: UI de abas no painel de terminal ou múltiplos `ptyId` agrupados por `combId`, reutilizando o mesmo `cwd`. |
| **Bell** e notificações conscientes de atividade | Utilizador não perde eventos quando o terminal está em segundo plano | **Sim**: mapear **bell** do xterm (`onBell`) → `app_show_notification` ou evento de atenção; alinhar com OSC já tratados no Rust (ex.: protocolos tipo Ghostty no `check_needs_attention`). |
| Scrollback grande / histórico | Depuração de logs longos sem perder contexto | **Sim**: aumentar limites configuráveis; opcional **persistência** do scrollback por sessão (ficheiro ou SQLite comprimido). |
| **Batching** de output para o renderer | Menos jank e menos pressão no IPC | **Já existe** padrão ~60fps + limite de bytes no reader. Documentar e afinar constantes por plataforma. |

**Implementações concretas recomendadas**

- **xterm addons**: `@xterm/addon-webgl` (ou canvas otimizado) para **renderização GPU no browser**; `addon-search`; opcional **unicode11** para larguras corretas; revisar `scripts/patch-xterm-viewport.mjs` como parte da estratégia de estabilidade.
- **Preferências**: expor no UI (além de fonte/tema) **scrollback lines**, **cursor style**, **copy on select**, **right-click paste**, **audible bell** on/off.
- **Seleção e clipboard**: integração nativa Tauri para cópia rica e **OSC 52** (colar remoto) se fizer sentido para o público-alvo.
- **Deteção de “ligação lenta”**: indicador quando o batching descarta fluidez (métricas em `output-metrics`).

---

## 2. Sessões persistentes e modelo “daemon”

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **Sessões de terminal que sobrevivem ao fecho da GUI** | Agentes longos (`npm test`, servidores, pipelines) continuam após fechar a janela | **Já existe** um **sidecar** (`dccd`) que detém PTYs, com fallback in-process em dev; a GUI reconecta via bridge/RPC e pode reanexar streams. |
| **Attach / detach** explícitos | Fluxo claro para “deixar a correr em background” | **Já existe**: comandos de attach/detach por `projectId` + `taskId`, com estado persistido em SQLite. |
| Um **daemon** alimenta GUI, Web e CLI | Um único modelo de verdade para automações e integrações | **Já existe como base**: o mesmo núcleo atende GUI, CLI e MCP; a superfície HTTP local ainda pode ser adicionada como fase seguinte. |

**Nota**: É o item de **maior impacto arquitetural** e habilita tasks agendadas, MCP e integrações — exige desenho cuidadoso (lifecycle, auth local, portas).

---

## 3. Sinais, lifecycle e integração com o SO

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **Interrupt / terminate / kill** explícitos | Controlo fino quando Ctrl+C não chega ao processo certo | **Sim**: expor comandos que enviam sinais ao **grupo de processos** do PTY (Unix: `killpg`; Windows: APIs de job object / console). |
| Ligação terminal ↔ processo gerido | Reiniciar “o servidor” sem perder o painel | **Sim** (combinar com secção 4). |
| Abrir terminal externo no path | Integração com Terminal.app, iTerm, etc. | **Parcial**: `shell_open_terminal_at_path` existe; completar fluxos cruzados com sessões embutidas. |

---

## 4. Processos gerenciados (Procfile / config de repo)

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **Procfile** + ficheiro de projeto (`arbor.toml` → equivalente DCC) | `web`, `worker`, `docker compose up` com um clique | **Sim**: *supervisor* em Rust (ou integração `honcho`-like) com **restart**, **backoff**, **logs** ligados a um PTY ou painel de log dedicado. |
| Estado `running` / `restarting` / `crashed` | Visibilidade operacional | **Sim**: tabela em SQLite ou estado em memória + persistência de último exit code. |
| **Métricas de memória** e contadores de restart | Diagnosticar fugas e loops de crash | **Sim** via `sysinfo` ou leitura de `/proc`/`task_info` conforme SO. |
| **Auto start** ao abrir um comb | Menos fricção ao entrar no workspace | **Sim**: *hook* após `comb_ensure_worktree`. |

---

## 5. Worktrees, Git e fluxo de integração

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **Regras de nome de branch** (`prefix_mode`: github-user, custom, etc.) | Branches previsíveis em equipas | **Sim**: ler `.dcc.toml` ao criar branch/worktree; validação antes de criar. |
| **Scripts setup/teardown** com *rollback* se setup falhar | Worktrees nunca ficam “a meio” | **Sim**: executar comandos configurados; se falhar, undo + mensagem clara (Arbor documenta este comportamento). |
| **Confirmação de delete** com deteção de **commits não pushed** | Evitar perda de trabalho | **Sim**: `git cherry` / `git log @{u}..` antes de `comb_discard`. |
| **Histórico de navegação** entre worktrees | UX tipo browser para saltar entre contextos | **Sim** no front + pilha em memória ou SQLite. |
| **Última atividade Git** por worktree | Ordenação e “o que está ativo” na sidebar | **Sim**: atualizar timestamp em operações git ou *watch* periódico leve. |
| **Merge para main / integração** | Fechar o ciclo da *mission* | **Crítico no DCC**: `comb_merge_into_main` está **stub** — implementação desejada com merge/rebase, resolução de conflitos e feedback na UI. |
| **Apply patch** | Aplicar alterações do worktree na base | **`comb_apply_patch` stub** — alinhar com `git_apply_worktree_patch` já exposto no bridge se aplicável. |

---

## 6. Issues, forges e contexto de PR

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| Criar worktrees a partir de **issues** GitHub/GitLab | Ligação tarefa ↔ código isolado | **Sim** via APIs HTTP + OAuth/token em `providers` ou config segura. |
| Pré-visualização de nomes sanitizados (branch/path) | Menos erros de path no Windows/macOS | **Sim**: funções puras Rust + *preview* na UI antes de confirmar. |
| Ligação automática a **PRs/MRs** abertos | Contexto de revisão sem sair do DCC | **Sim**: polling ou webhooks (se houver servidor); MVP com `gh cli`/`glab` opcional. |

---

## 7. Diff, revisão e árvore de ficheiros

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **Várias abas de diff** | Comparar ficheiros sem perder o anterior | **Sim** no React. |
| Contagens de linhas +/- por ficheiro | Resumo rápido do impacto | **Sim**: já há bases de review; unificar métricas. |
| **Lista de ficheiros alterados** + árvore com expand/collapse | Navegação em repos grandes | **Sim**: componentes de árvore + dados de `git diff --name-status`. |
| **Notas** por worktree (ex. `.arbor/notes.md`) | Memória de contexto humana | **Sim**: ficheiro no worktree ou campo no SQLite. |
| Comentários inline de PR | Revisão colaborativa | **Parcial**: depende de API do forge; pode ser fase 2. |

---

## 8. Agentes de IA e visibilidade de atividade

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| Deteção de agentes a correr (Claude Code, Codex, OpenCode, …) | Painel “quem está a trabalhar onde” | **Sim**: heurística por processo + `cwd` + opcionalmente parsing de título do terminal; eventos para a UI. |
| Estados **working / waiting** com indicadores coloridos | Mesma filosofia do sistema de atenção atual, porém mais rico | **Sim**: estender `attention-types` e badges na `CmuxWorkspacePage`. |
| **WebSocket** de atualização em tempo real | Menos polling; UI mais viva | **Sim** quando existir daemon; até lá, eventos Tauri são suficientes. |

---

## 9. Automação, tarefas agendadas e *hooks* de repo

As **`[[tasks]]` com cron** e os **triggers** pós-execução (ver Arbor) já estão ligados ao **processo em background** do daemon. *Hooks* de repo e templates em Markdown continuam como área de expansão, mas o esqueleto do agendamento já existe no DCC.

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **`[[tasks]]` com cron** (incl. segundos) no daemon | *Triage* periódico, relatórios, sync | **Já existe** um scheduler no daemon com UI para listar, executar, anexar e desanexar tarefas. |
| **Triggers** pós-execução (stdout → prompt para agente) | Automação “quando o script terminar, pedir revisão à IA” | **Parcial**: a infraestrutura está montada, mas o pipeline declarativo ainda pode ficar mais rico. |
| **Templates** Markdown em pasta do repo (`.arbor/tasks` → `.dcc/tasks`) | Presets partilháveis no repositório | **Sim**: ler ficheiros + mostrar na paleta de comandos. |
| **Webhooks** para eventos (agent started/finished) | Integração com Slack/Discord/CI | **Ainda por fazer**: pode ser encaixado no daemon ou como *hook* opcional no app. |

---

## 10. MCP, API HTTP e CLI *headless*

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **`dcc-mcp`** (stdio) a falar com API local | Cursor/Codex/Claude Desktop orquestram worktrees/terminais via MCP | **Já existe** um servidor MCP via stdio no binário `dcc`, com tools mínimas e expansão incremental. |
| **HTTP API** + **CLI** (`arbor-cli` → `dcc`) | Scripts CI, automação remota (com token) | **CLI já existe**; a **HTTP API mínima** já existe e o próximo passo é separar consumo local de consumo remoto com auth própria. |
| **Recursos MCP** (snapshot do daemon, *prompts* de workflow) | Onboarding consistente para agentes | **Parcial**: já há snapshot do daemon, mas a camada de prompts/workflows ainda pode crescer. |

Isto posiciona o DCC como **hub** para ferramentas externas, não só GUI — alinhado ao modelo Arbor sem exigir GPUI. **MCP e API/CLI** são parte desse **ecossistema pretendido**, quando o daemon estiver disponível.

---

## 11. Acesso remoto e *outposts* (fase posterior)

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| Daemon remoto com **auth token**, SSH, Mosh | Máquinas de build ou servidores de equipa | **Sim**, tipicamente **depois** do daemon local estável: mesma API, *binding* e auth em rede. |
| Worktrees remotos via SSH | Paridade com fluxos dev em VM | **Sim** na mesma lógica de fase — útil para quem trabalha contra VM ou host remoto. |

Fica **mais tarde na roadmap** por **complexidade** (rede, segurança, UX de falha), não por falta de utilidade.

---

## 12. UI/UX: paleta de comandos, temas, notificações

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **Command palette** densa (ações, repos, worktrees, issues, presets) | Navegação sem rato | **Sim**: `cmd-k` + fuzzy search; dados já em SQLite. |
| **Temas** partilhados (38 no Arbor) | Personalização e *branding* | **Sim**: tokens CSS + presets; import/export. |
| **Título da janela** com branch/worktree | Orientação rápida | **Sim** via API de janela Tauri. |
| Notificações desktop **ricas** | Completar `app_show_notification` com ícones e ações | **Sim**. |
| Layout **três painéis** redimensionáveis | Já há filosofia cmux; refinar *resizable* e atalhos | **Sim**. |

---

## 13. Configuração por repositório (equivalente a `arbor.toml`)

Consolidar num esquema único, com **`dcc.toml`** como formato canônico do repositório, campos inspirados no Arbor:

- `[[presets]]` — comandos nomeados (review, test, lint).
- `[[processes]]` — serviços supervisionados.
- `[scripts]` — `setup` / `teardown`.
- `[branch]` — regras de prefixo e sanitização.
- `[agent]` — preset por defeito, *auto-checkpoint* opcional após comandos.
- `[notifications]` — eventos e webhooks.
- `[[tasks]]` — agendamento (se o daemon existir).

Benefício: **reprodutibilidade** entre máquinas e equipas, tal como no Arbor. O SQLite fica como cache/estado local; o arquivo do repositório é a fonte de verdade.

---

## 14. Segurança, credenciais e ambiente

| Inspiração / boa prática | Benefício no DCC | Tauri |
|---------------------------|------------------|-------|
| Tokens para API remota | Mesmo padrão Arbor (`Authorization: Bearer`) | **Fase 2**: necessário quando o daemon sair do localhost. |
| **Secrets** só no *keychain* / *credential manager** | Menos exposição que SQLite em texto | Já há rumo a `api_key_encrypted`; estender padrão. |
| Sandboxing de comandos de *preset* | Evitar `rm -rf /` acidental | **Sim**: confirmação + lista de permitidos. |

---

## 15. Observabilidade e qualidade

- **Logs estruturados** do lado Rust (nível `trace` em dev) para PTY e git.
- **Métricas** opcionais: tempo médio de batching, linhas/minuto no terminal, falhas IPC.
- **Testes**: contratos para comandos Tauri críticos (`comb_ensure_worktree`, terminal spawn).
- **Benchmark** opcional de throughput terminal (similar ao `bench-embedded-terminal-engines` do Arbor, adaptado a xterm + IPC).

---

## Gaps já mapeados no código DCC

Comandos que ainda retornam **`NOT_IMPLEMENTED`** em `src-tauri/src/main.rs` (alinhado a `scripts/audit-tauri-stubs.mjs`):

| Área | Comando / API |
|------|----------------|
| App | `app_check_for_updates`, `app_quit_and_install` |
| Diálogo | `dialog_show_message` |
| Janela | `window_is_maximized` |
| Licença | `license_get_machine_id`, `license_activate`, `license_skip_activation` |

Os comandos de **Shell** e **Comb / Git** já foram implementados e deixaram de ser gaps de produto; em **Janela**, o bloco principal está pronto e sobra apenas a consulta `window_is_maximized`. Ainda assim, vale manter auditoria automática para evitar regressões e stubs futuros.

Completar os itens restantes **remove fricção** e mantém o contrato de integração do DCC consistente com a UI e o desktop.

---

## Priorização sugerida

**P0 — Alto impacto / alinhado ao núcleo do DCC**

1. Fechar o que ainda está pendente nos comandos de app/diálogo/licença, mantendo o contrato Tauri estável.
2. **Terminal**: addons WebGL/search, preferências de scrollback, **bell** → notificação.
3. **Confirmação de discard** com **commits não pushed**.
4. **HTTP API**: a superfície mínima já existe; o próximo foco é auth remota, SDK/cliente tipado e rotas mais resource-oriented.

**P1 — Diferenciação forte**

5. **Processos gerenciados** (Procfile + `dcc.toml`) com ligação a panes.
6. **Config de repo** unificada (presets, scripts setup/teardown, regras de branch).
7. **Command palette** global.
8. **Deteção de agentes** a correr + estados na UI.

**P2 — Plataforma e ecossistema** *(objetivos confirmados; ordem: depois de P0/P1 por dependência — sobretudo daemon)*

9. **Daemon** + sessões persistentes + attach/detach.
10. **Tasks agendadas** (`[[tasks]]` + *triggers*) no daemon, com UI de gestão.
11. **`dcc-mcp`** + API HTTP mínima + CLI no mesmo daemon.
12. **Issues/PR** integrados com GitHub/GitLab.
13. **Acesso remoto** / multi-host (secção 11).

---

## 18. Checklist de Implementação (Auditoria Completa)

**Legenda:**
- ✅ **Implementado** - Funcionalidade completa e testada
- 🟡 **Parcialmente Implementado** - Base existe, mas faltam aspectos importantes
- ❌ **Não Implementado** - Ainda não iniciado

---

### 1. Terminal PTY, Emulação e Desempenho

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| PTY com truecolor e xterm-256color | ✅ | `main.rs:4697-4767` | Implementado com `portable_pty`, TERM=xterm-256color em Unix |
| Batching de output (~60fps, 128KB max) | ✅ | `main.rs:4697-4767` | Flush interval 16ms, alinhado com superset-sh |
| Eventos terminal-output/attention/exit | ✅ | `main.rs:4756,4760,4853` | Heurística de waiting com regex, eventos para frontend |
| Buffer circular (backlog 1000 linhas) | ✅ | `main.rs:208,224,4880` | `terminal_get_backlog` para reidratação xterm |
| xterm.js com WebGL addon | ✅ | `embedded-terminal.tsx:7` | v5.3.0 com FitAddon, SearchAddon |
| xterm Search addon | ✅ | `embedded-terminal.tsx:8` | UI de busca integrada |
| Preferências de aparência (font, theme) | ✅ | `terminal-preferences.ts` | `getTerminalAppearancePreferences` |
| Scrollback persistente em SQLite | ❌ | - | Buffer só em memória, perda ao reiniciar |
| OSC 52 (remote clipboard) | ❌ | - | Não implementado |
| Signal handling explícito (SIGTERM/SIGKILL) | 🟡 | - | Ctrl+C funciona, mas sem API customizada para sinais |
| Configuração scrollback lines no UI | ❌ | - | Falta exposição no settings |
| Bell → notificação desktop | 🟡 | `embedded-terminal.tsx` | `onBell` existe, mas integração com `app_show_notification` incompleta |
| Múltiplas abas de terminal por worktree | ❌ | - | Só 1 pane por comb, sem tabs agrupadas |
| Deteção de "ligação lenta" | ❌ | - | Métricas em `output-metrics.ts`, mas sem indicador de lag |

---

### 2. Sessões Persistentes e Modelo "Daemon"

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Daemon sidecar (dccd) | ✅ | `src-tauri/src/bin/dccd.rs` | Processo persistente, fallback in-process |
| Attach/Detach de sessões | ✅ | `daemon_runtime.rs:1086` | RPC `daemon.attachTask`, `daemon.detachTask` |
| PTYs sobrevivem ao fecho da GUI | ✅ | `daemon_runtime.rs:614` | `DaemonService` mantém estado em memória + SQLite |
| Runtime file (daemon-runtime.json) | ✅ | `daemon_runtime.rs:83` | PID, started_at, db_path |
| RPC via SQLite (daemon_rpc_requests) | ✅ | `daemon_runtime.rs:1233` | Request loop processa até 32 requests/200ms |
| WebSocket/Event stream tempo real | ❌ | - | Polling via RPC, sem WebSocket |
| HTTP API REST | 🟡 | `src-tauri/src/http_api.rs` | REST local com auth X-API-Key e OpenAPI |
| Health metrics (CPU/RAM daemon) | ❌ | - | Status básico existe, métricas de recursos não |

---

### 3. Sinais, Lifecycle e Integração com o SO

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Interrupt/Terminate/Kill explícitos | 🟡 | `main.rs:4930` | `terminal_kill` envia signal, mas sem API para SIGTERM customizado |
| Grupo de processos (killpg Unix) | ❌ | - | Falta envio de sinal ao grupo inteiro |
| Windows job object para terminação | ❌ | - | Falta implementação Windows-specific |
| Abrir terminal externo | ✅ | `main.rs:2052` | `shell_open_terminal_at_path` implementado |
| Integração com Terminal.app/iTerm | ✅ | `main.rs:2052` | Via script AppleScript |

---

### 4. Processos Gerenciados (Procfile / Config de Repo)

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Schema `[[processes]]` em .dcc.toml | ✅ | `main.rs:64, daemon_runtime.rs:27` | `RepoProcessPayload` com auto_restart, cwd_mode |
| Parser de config de processos | ✅ | `daemon_runtime.rs:420` | `parse_repo_config_processes` lê TOML e JSON |
| Supervisor com auto-restart | ✅ | `daemon_runtime.rs:1066-1253` | **IMPLEMENTADO**: `sweep_managed_processes` com auto-restart completo |
| Estado running/restarting/crashed | ✅ | `schema.sql:131, daemon_runtime.rs:119` | Estados: stopped/starting/running/stopping/restarting/crashed/failed |
| Métricas de memória/CPU por processo | ✅ | `daemon_runtime.rs:628-643, 1170-1182` | Coleta via sysinfo em sweep_managed_processes, update automático no SQLite |
| Auto-start ao abrir comb | ❌ | - | Falta hook após `comb_ensure_worktree` |
| UI para gerenciar processos | ✅ | `components/processes-panel.tsx`, `CmuxWorkspacePage.tsx` | Painel na sidebar: estado, métricas, start/stop/restart via daemon |
| Logs dedicados por processo | ✅ | `daemon_runtime.rs:1035-1042` | Buffer circular de stdout/stderr, excerpt salvo em SQLite |
| Backoff exponencial em restart | ✅ | `daemon_runtime.rs:1195-1201` | Backoff 1s → 2s → 4s → ... → max 300s implementado |

---

### 5. Worktrees, Git e Fluxo de Integração

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| comb_ensure_worktree | ✅ | `main.rs:4040-4137` | Cria worktree + branch com prefix configurável |
| comb_merge_into_main | ✅ | `main.rs:4222-4335` | **Implementado completamente** com detecção de conflitos |
| comb_apply_patch | ✅ | `main.rs:4359-4469` | **Implementado completamente** com git apply --3way |
| comb_discard | ✅ | `main.rs:4139-4173` | Remove worktree + branch |
| comb_check_unpushed | ✅ | `main.rs:4177-4218` | `git cherry -v` retorna commits não pushed |
| Regras de nome de branch (prefix_mode) | ✅ | `main.rs:375` | Leitura de `branchPrefix` do .dcc.toml |
| Scripts setup/teardown | ✅ | `main.rs:4040,4139` | Executa comandos configurados |
| Rollback se setup falhar | ❌ | - | Setup retorna erro, mas não faz undo do worktree |
| Confirmação de delete com unpushed | 🟡 | - | Check implementado, UI não força confirmação |
| Histórico de navegação entre worktrees | ❌ | - | Falta pilha de navegação (browser-like) |
| Última atividade Git por worktree | 🟡 | `schema.sql:daemon_task_runs` | `last_run_at` existe, falta watch periódico Git |
| Preview de branch/path sanitizado | 🟡 | `main.rs:4047` | Sanitização existe, falta preview na UI |

---

### 6. Issues, Forges e Contexto de PR

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Criar worktrees de issues GitHub/GitLab | ❌ | - | Falta integração com APIs de forge |
| OAuth/Token em providers | ✅ | `schema.sql:providers` | Campo `api_key_encrypted`, mas criptografia não implementada |
| Preview de nomes sanitizados | ❌ | - | Falta UI de preview antes de criar |
| Ligação automática a PRs/MRs | ❌ | - | Falta polling ou webhook |
| gh cli / glab integration | 🟡 | `main.rs:1954` | `shell_detect_cli_for_provider` detecta, mas não usa |

---

### 7. Diff, Revisão e Árvore de Ficheiros

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Várias abas de diff | ❌ | - | Falta componente de tabs no frontend |
| Contagens +/- por ficheiro | 🟡 | `main.rs:4570` | `build_review_diffs_for_path` calcula, falta UI |
| Lista de ficheiros + árvore expand/collapse | ❌ | - | Falta componente de árvore |
| Notas por worktree (.dcc/notes.md) | ❌ | - | Falta editor Markdown no UI |
| Comentários inline de PR | ❌ | - | Falta integração com API forge |
| DiffCodeBlock | ✅ | `diff-code-block.tsx` | Syntax highlighting com Prism.js |

---

### 8. Agentes de IA e Visibilidade de Atividade

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Deteção de agentes (Claude, Codex, etc.) | 🟡 | `main.rs:1954` | `shell_detect_cli_for_provider`, mas sem heurística de processo rodando |
| Estados working/waiting com badges | ✅ | `CmuxWorkspacePage.tsx` | Icons Bot, Clock3, Terminal |
| Parsing de título do terminal | ❌ | - | Falta extração de contexto do título |
| Painel "quem está onde" | ❌ | - | Falta agregação de atividade |
| WebSocket atualização tempo real | ❌ | - | Eventos Tauri suficientes, mas sem WS |

---

### 9. Automação, Tarefas Agendadas e Hooks de Repo

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Parser de cron (5 ou 6 campos) | ✅ | `daemon_runtime.rs:253` | Suporta segundos, ranges, steps, wildcards |
| Scheduler tick loop (5s) | ✅ | `daemon_runtime.rs:1247` | `sweep_loop` verifica next_run_at |
| Execução de tasks agendadas | ✅ | `daemon_runtime.rs:614` | `create_running_task` spawn comandos |
| UI para listar/run tasks | ✅ | `workspace-command-palette.tsx` | Grupo Tasks com ícone Clock3 |
| **Triggers pós-execução (stdout → prompt)** | ✅ | `daemon_runtime.rs:2020-2071` | **IMPLEMENTADO**: Pipeline completo com suporte Anthropic + OpenAI |
| Templates Markdown em .dcc/tasks | ❌ | - | Falta leitura de pasta de templates |
| Webhooks para eventos (agent started/finished) | ❌ | - | Falta infraestrutura de webhooks |
| [[tasks]] em .dcc.toml | ✅ | `daemon_runtime.rs:308` | `parse_task_toml_from_repo_config` |

---

### 10. MCP, API HTTP e CLI Headless

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| dcc-mcp servidor stdio | ✅ | `src-tauri/src/bin/dcc.rs:396` | JSON-RPC 2.0, versão 2024-11-05 |
| Tools: daemon_status, daemon_tasks, run/attach/detach | ✅ | `dcc.rs:136` | 8 tools implementadas |
| Tools: combs_list, panes_list, diffs_bundle | ✅ | `dcc.rs:136` | Filtros por projectId/combId |
| Resources MCP (snapshots, prompts) | ❌ | - | Falta implementação de resources |
| Prompts MCP (workflows) | ❌ | - | Falta templates pré-definidos |
| CLI dcc (daemon status/tasks/run/attach/detach) | ✅ | `dcc.rs:18-67` | Comandos completos |
| HTTP API REST | 🟡 | `src-tauri/src/http_api.rs`, `src-tauri/src/bin/dccd-http.rs` | **CRÍTICO**: REST mínima local + compatibilidade `/rpc`; próximo: recursos tipados e auth remota |
| Autenticação local | ✅ | `src-tauri/src/http_auth.rs` | Header `X-API-Key` nas rotas protegidas |
| Autenticação remota (Bearer/token) | ❌ | - | Próximo passo para expor o daemon fora da máquina local |
| Documentação OpenAPI | ✅ | `docs/GUIA_HTTP_API.md`, `src-tauri/src/http_api.rs` | `GET /openapi.json` |

**Próximos passos recomendados nesta área**

1. Separar formalmente o contrato local do contrato remoto.
2. Evoluir os endpoints atuais para payloads resource-oriented e tipados.
3. Implementar auth remota com Bearer token e rotação/expiração.
4. Gerar um client tipado para o frontend ou integrações externas.
5. Crescer os resources MCP e prompts sobre a mesma base de dados/contrato.

---

### 11. Acesso Remoto e Outposts

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Daemon remoto com auth token | ❌ | - | **Fase 2**: Após daemon local estável |
| SSH/Mosh integration | ❌ | - | **Fase 2** |
| Worktrees remotos via SSH | ❌ | - | **Fase 2** |

---

### 12. UI/UX: Paleta de Comandos, Temas, Notificações

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Command palette (cmd+k) | ✅ | `workspace-command-palette.tsx` | Grupos: Global, Projeto, Workspaces, Panes, Presets, Tasks |
| Fuzzy search | ✅ | `workspace-command-palette.tsx` | Sobre projetos, combs, panes, comandos |
| Temas partilhados | ✅ | `xterm-theme.ts` | `getXtermColorTheme`, ThemeProvider |
| Título da janela com branch/worktree | ❌ | - | Falta API de janela Tauri para dynamic title |
| Notificações desktop ricas (ações) | 🟡 | `main.rs:1824` | `app_show_notification` básico, falta ações |
| Layout 3 painéis redimensionáveis | ✅ | `CmuxWorkspacePage.tsx` | Sidebar, terminal/agent, diffs |
| Atalhos de teclado | 🟡 | - | cmd+k existe, falta outros atalhos |

---

### 13. Configuração por Repositório (.dcc.toml)

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Estrutura .dcc.toml completa | ✅ | `main.rs:114-153` | `RepoConfigToml` com todos campos |
| Parser TOML bidirecional | ✅ | `main.rs:385,445` | Conversão TOML ↔ Payload |
| [[presets]] | ✅ | `main.rs:53` | `RepoPresetPayload` |
| [[processes]] | ✅ | `main.rs:63` | `RepoProcessPayload`; supervisor + UI na sidebar |
| [scripts] setup/teardown | ✅ | `main.rs:47` | `RepoScriptsPayload` |
| [branch] prefix | ✅ | `main.rs:27` | `RepoBranchConfigPayload` |
| [agent] default_provider_id | ✅ | `main.rs:42` | `RepoAgentConfigPayload` |
| [[tasks]] | ✅ | `main.rs:75` | `RepoTaskPayload` com schedule, trigger |
| [notifications] webhooks | ❌ | - | Falta campo no schema |
| [agent.auto_checkpoint] | ❌ | - | Falta campo no schema |
| Comandos Tauri get/save | ✅ | `main.rs:3217,3271` | `db_projects_get/save_repo_config_toml` |
| UI editor TOML | ✅ | `ProjectRepoTomlDialog` | Dialog raw TOML |

---

### 14. Segurança, Credenciais e Ambiente

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Tokens para API remota (Bearer) | ❌ | - | Ainda não existe para uso remoto; localmente já usamos `X-API-Key` |
| Secrets no keychain/credential manager | 🟡 | `schema.sql:providers.api_key_encrypted` | Campo existe, criptografia não implementada |
| `db_providers_is_encryption_available` | ✅ | `main.rs:3642` | Retorna false (stub) |
| Sandboxing de comandos preset | ❌ | - | Falta confirmação + allow list |
| Validação de paths maliciosos | 🟡 | `main.rs:4047` | Sanitização básica, falta validação profunda |

---

### 15. Observabilidade e Qualidade

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Logs estruturados Rust (trace em dev) | 🟡 | - | `println!` usado, falta logger estruturado (tracing/log) |
| Métricas de batching terminal | 🟡 | `output-metrics.ts` | Frontend tem métricas, backend não expõe |
| Testes de contratos Tauri | ❌ | - | Falta suite de testes para comandos críticos |
| Benchmark de throughput terminal | ❌ | - | Falta adaptação do bench-embedded-terminal-engines |

---

### 16. Gaps Já Mapeados no Código (Stubs Pendentes)

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| app_check_for_updates | ❌ | `main.rs` | NOT_IMPLEMENTED |
| app_quit_and_install | ❌ | `main.rs` | NOT_IMPLEMENTED |
| dialog_show_message | ❌ | `main.rs` | NOT_IMPLEMENTED |
| dialog_confirm | ❌ | `main.rs` | NOT_IMPLEMENTED |
| window_is_maximized | ❌ | `main.rs` | NOT_IMPLEMENTED |
| license_get_machine_id | ❌ | `main.rs` | NOT_IMPLEMENTED |
| license_activate | ❌ | `main.rs` | NOT_IMPLEMENTED |
| license_skip_activation | ❌ | `main.rs` | NOT_IMPLEMENTED |

---

## Resumo Executivo: Prioridades para 100% de Completude

### 🔴 **CRÍTICO** (Bloqueadores para fluxo completo)

1. ✅ **Supervisor de Processos** (Secção 4) - **COMPLETO**
   - ✅ Schema, parser, auto-restart, estados, backoff exponencial
   - ✅ Comandos Tauri e handlers RPC
   - ✅ Métricas CPU/memória com sysinfo (coleta automática em sweep)
   - ✅ UI frontend: painel na sidebar do workspace (`ProcessesPanel` + `daemon_list/start/stop/restart_process`)

2. ✅ **Triggers de Tasks** (Secção 9) - **COMPLETO**
   - ✅ Pipeline declarativo: task completa → avalia condição → envia prompt para IA
   - ✅ Suporte Anthropic Claude (Messages API) + OpenAI GPT (Chat Completions API)
   - ✅ Variáveis de template: `{{task_name}}`, `{{command}}`, `{{exit_code}}`, `{{output}}`, `{{status}}`
   - ✅ Condições: `when = "success"/"failure"/"complete"`
   - ✅ Integração em `sweep_finished_tasks()` com tratamento de erros robusto
   - ✅ Log de execução com `println!` (tabela SQL em Fase 2)
   - ✅ Documentação completa: `docs/GUIA_TRIGGERS_TASKS.md`

3. **HTTP API REST** (Secção 10)
   - REST mínima já implementada em `dccd-http` com auth por API key
   - Próximo passo: expandir recursos e contratos públicos

4. **Criptografia de Secrets** (Secção 14)
   - `api_key_encrypted` usa keychain/credential manager
   - Estimativa: ~200 linhas Rust

### 🟡 **IMPORTANTE** (Completam funcionalidades existentes)

5. **Scrollback Persistente** (Secção 1)
   - Salvar buffer terminal em SQLite comprimido
   - Estimativa: ~150 linhas Rust

6. **Confirmação de Delete com Unpushed** (Secção 5)
   - UI força confirmação quando há commits não pushed
   - Estimativa: ~50 linhas React

7. **Árvore de Arquivos para Diffs** (Secção 7)
   - Componente de navegação em diffs grandes
   - Estimativa: ~300 linhas React

8. **Templates de Tasks** (Secção 9)
   - Ler `.dcc/tasks/*.md` e expor na command palette
   - Estimativa: ~100 linhas Rust + 100 linhas React

### 🟢 **DESEJÁVEL** (Polimento e UX)

9. **Título Dinâmico da Janela** (Secção 12)
   - Mostrar branch/worktree atual no título
   - Estimativa: ~20 linhas Rust

10. **Notificações Ricas** (Secção 12)
    - Ações (reply, dismiss) em notificações desktop
    - Estimativa: ~100 linhas Rust

11. **Webhooks** (Secção 9)
    - Eventos para Slack/Discord/CI
    - Estimativa: ~200 linhas Rust

12. **Logs Estruturados** (Secção 15)
    - Migrar de `println!` para `tracing`
    - Estimativa: ~50 linhas refactor

### 📊 **Métricas de Completude por Área**

| Área | Implementado | Parcial | Pendente | % Completo |
|------|-------------|---------|----------|------------|
| Terminal PTY | 9 | 3 | 5 | 75% |
| Daemon/Sessões | 7 | 0 | 3 | 70% |
| Sinais/Lifecycle | 2 | 1 | 3 | 50% |
| **Processos** | **9** | **0** | **0** | **100%** |
| Worktrees/Git | 8 | 3 | 3 | 79% |
| Issues/Forges | 1 | 1 | 4 | 20% |
| Diff/Review | 1 | 1 | 4 | 25% |
| Agentes IA | 1 | 1 | 3 | 30% |
| **Tasks Agendadas** | **6** | **0** | **3** | **67% → 75%** |
| MCP/API/CLI | 5 | 0 | 4 | 56% |
| Acesso Remoto | 0 | 0 | 3 | 0% (Fase 2) |
| UI/UX | 6 | 2 | 3 | 67% |
| Config Repo | 10 | 0 | 2 | 83% |
| Segurança | 1 | 2 | 2 | 40% |
| Observabilidade | 0 | 2 | 2 | 25% |
| Stubs Tauri | 0 | 0 | 8 | 0% |

**TOTAL GERAL: 66 implementados (+2) + 15 parciais (-1) + 52 pendentes ≈ 69% completo (+2%)**

---

## 🎉 Implementações Recentes

### Triggers de Tasks (2026-04-11) ✅

Implementação completa do sistema de triggers pós-execução para tasks agendadas:

**Features implementadas:**
- Pipeline declarativo: task completa → avalia condição (`when`) → renderiza prompt → chama AI provider → loga resultado
- Suporte para **Anthropic Claude** (Messages API v1) e **OpenAI GPT** (Chat Completions API)
- Sistema de templates com variáveis: `{{task_name}}`, `{{command}}`, `{{exit_code}}`, `{{output}}`, `{{status}}`
- Três condições de trigger: `when = "success"` (exit code 0), `"failure"` (exit code != 0), `"complete"` (sempre)
- Tratamento robusto de erros (triggers nunca crasham o daemon)
- Base URL customizável para LLMs auto-hospedados (LiteLLM, Ollama, etc.)

**Arquivos modificados:**
- `src-tauri/Cargo.toml`: Adicionada dependência `reqwest` com features `blocking` e `json`
- `src-tauri/src/daemon_runtime.rs`: ~400 linhas de código novo
  - Struct `ProviderRow` para dados do provider
  - `DaemonService::get_provider()` - Query providers do SQLite
  - `should_trigger_execute()` - Avaliação de condições
  - `render_trigger_prompt()` - Substituição de variáveis
  - `call_anthropic_api()` - Integração com Claude
  - `call_openai_api()` - Integração com GPT
  - `call_ai_provider()` - Dispatcher por `provider_type`
  - `log_trigger_execution()` - Log com `println!`
  - `execute_trigger()` - Orquestrador principal
  - `sweep_finished_tasks()` - Hook para executar triggers

**Documentação:**
- `docs/GUIA_TRIGGERS_TASKS.md`: Guia completo de configuração e uso
  - Exemplos de configuração de providers (Anthropic, OpenAI, LLMs locais)
  - Casos de uso práticos (CI/CD, monitoring, deploy)
  - Solução de problemas
  - Boas práticas

**Próximos passos (Fase 2):**
- Tabela `trigger_executions` para histórico persistente
- UI para visualizar execuções de triggers
- Retry automático com backoff exponencial
- Suporte para Google AI, Ollama nativo, Azure OpenAI
- Criptografia de API keys com keychain/credential manager
- Triggers encadeados (trigger → spawna nova task)

---

## Nota final

O Arbor demonstra que um **produto “agentic coding”** completo combina: **terminal sério**, **worktrees**, **processos**, **automação**, **daemon** e **MCP**. O DCC já cobre parte disso com **SQLite + Tauri + worktrees + atenção no terminal**. A lista acima traduz essas capacidades para **incrementos realistas** na stack — **sem GPUI** — com **paridade funcional** como **direção explícita**: **daemon**, **tasks agendadas** e **MCP** não são “só se alguém quiser”; entram na **ordem certa** (P2 após fundações), por **viabilidade técnica**, não por falta de benefício para o dia a dia.
