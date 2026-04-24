# Melhorias inspiradas no Arbor para o Dev Command Center (Rust + Tauri)

Este documento consolida **oportunidades de evolução do DCC** inspiradas no projeto [**Arbor**](https://github.com/penso/arbor) (README oficial, documentação em `docs/src` do repositório, e visão geral dos crates), **sem reescrever a UI em GPUI**: mantém-se **Tauri 2 + React + TypeScript** no front e **Rust** no host, como hoje.

**Visão de produto (âmbito desejado)**  
O DCC pretende ser **ferramenta de uso diário** o mais completa e útil possível dentro da stack Tauri. Por isso, as capacidades abaixo — em especial **daemon** (sessões persistentes, API estável), **tasks agendadas** (cron + *triggers* no daemon) e **MCP + API/CLI** — são tratadas como **objetivos de produto**, não como “extras só se alguém precisar”. A secção [Priorização sugerida](#priorização-sugerida) define **ordem de implementação** (o que vem primeiro por dependência técnica e valor imediato), não o que fica de fora por falta de interesse.

**Base no DCC atual (auditoria interna):**

- Terminal: `portable_pty` + eventos `terminal-output` / `terminal-attention` / `terminal-exit`, buffer de backlog (`terminal_get_backlog`) com **persistência por painel** (`pane_terminal_scrollback`, gzip/JSON), `TERM=xterm-256color` em Unix, integração Git em worktrees via `GIT_DIR` / `GIT_WORK_TREE`.
- UI: `xterm.js` + `FitAddon`, preferências em `localStorage`, heurística de atenção e integração com panes (`paneId`, reattach).
- Dados: SQLite (`projects`, `combs`, `panes`, `providers`).
- Worktree: `comb_ensure_worktree`, `comb_discard`, diffs de review; **`comb_merge_into_main` e `comb_apply_patch` já foram implementados** e fecham o ciclo de integração do worktree.
- Comandos Tauri que antes eram *stubs* (`NOT_IMPLEMENTED`) foram **fechados** (ver secção [Gaps já mapeados no código](#gaps-já-mapeados-no-código-dcc)); auditoria: `yarn audit:tauri-stubs` → **0** ocorrências em `main.rs`.

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
| Scrollback grande / histórico | Depuração de logs longos sem perder contexto | **Sim**: limites no buffer em memória (~1000 chunks); **persistência** por `pane_id` em SQLite (payload gzip + JSON), reidratação ao reabrir o workspace. |
| **Batching** de output para o renderer | Menos jank e menos pressão no IPC | **Já existe** padrão ~60fps + limite de bytes no reader. Documentar e afinar constantes por plataforma. |

**Implementações concretas recomendadas**

- **xterm addons**: `@xterm/addon-webgl` (ou canvas otimizado) para **renderização GPU no browser**; `addon-search`; opcional **unicode11** para larguras corretas; revisar `scripts/patch-xterm-viewport.mjs` como parte da estratégia de estabilidade.
- **Preferências**: expor no UI (além de fonte/tema) **scrollback lines**, **cursor style**, **copy on select**, **right-click paste**, **audible bell** on/off.
- **Seleção e clipboard**: integração nativa Tauri para cópia rica; **OSC 52** (clipboard remota) implementado no emulador (`registerOscHandler(52)` + `lib/terminal/osc52.ts`, escrita/leitura via `navigator.clipboard` e resposta injectada no PTY).
- **Deteção de “ligação lenta”**: indicador por pane quando o batching começa a acumular backlog/latência no renderer, com métricas em `output-metrics` e sensibilidade ajustável na UI.

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
| **Histórico de navegação** entre worktrees | UX tipo browser para saltar entre contextos | **Implementado** no front (`useWorktreeNavigationHistory`, pilhas em memória); persistência SQLite permanece opcional para evolução futura. |
| **Última atividade Git** por worktree | Ordenação e “o que está ativo” na sidebar | **Implementado**: `combs.last_git_activity_at`, `comb_refresh_git_activity` (reflog/log/index + index mtime se dirty), refresh na UI (~60s + foco na aba), ordenação na lista; toque em `ensure` / `merge` / `apply`. Ver §18 e *Implementações Recentes*. |
| **Merge para main / integração** | Fechar o ciclo da *mission* | **Implementado** no DCC: `comb_merge_into_main` com merge `--no-ff`, deteção de conflitos e feedback na UI. |
| **Apply patch** | Aplicar alterações do worktree na base | **Implementado**: `comb_apply_patch` com fluxo git apply alinhado ao bridge. |

---

## 6. Issues, forges e contexto de PR

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| Criar worktrees a partir de **issues** GitHub/GitLab | Ligação tarefa ↔ código isolado | **Implementado** (MVP): comando `forge_fetch_issue`; tenta primeiro **`gh issue view`** / **`glab issue view`** (com `cwd` no repositório) quando os binários estão no `PATH` e autenticados; caso contrário usa APIs REST (tokens `GITHUB_TOKEN`/`GH_TOKEN`, `GITLAB_TOKEN` ou PAT opcional na UI). No «Novo Workspace» carrega título/corpo e pré-preenche nome/descrição antes de `comb_ensure_worktree`. No GitHub, `gh pr view` rejeita números que são PR em vez de issue (paridade com a validação REST). |
| Pré-visualização de nomes sanitizados (branch/path) | Menos erros de path no Windows/macOS | **Implementado** no fluxo «Novo Workspace»: `comb_preview_worktree_naming` + bloco de pré-visualização; após carregar uma issue, o nome sugerido alimenta o mesmo fluxo de sanitização que `comb_ensure_worktree`. |
| Ligação automática a **PRs/MRs** abertos | Contexto de revisão sem sair do DCC | **Implementado** (MVP): tenta primeiro **`gh pr list`** / **`glab mr list`** (mesmo critério de auth); fallback **REST** — GitHub (`pulls` com `head=owner:branch`) e GitLab (`merge_requests` por `source_branch`). Resultado em `combs.forge_link` (JSON); sincronização após `comb_ensure_worktree`, ao reabrir worktree existente e com refresh leve ao focar workspace com worktree; sidebar com atalho PR/MR. **Webhooks** em tempo real continuam como melhoria futura opcional. |

---

## 7. Diff, revisão e árvore de ficheiros

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **Várias abas de diff** | Comparar ficheiros sem perder o anterior | **Implementado** no React: `RepoReviewSection` com barra de separadores por path (máx. 16), um diff visível por separador; clique na árvore abre/foca; ✕ fecha. |
| Contagens de linhas +/- por ficheiro | Resumo rápido do impacto | **Sim**: já há bases de review; unificar métricas. |
| **Lista de ficheiros alterados** + árvore com expand/collapse | Navegação em repos grandes | **Sim**: componentes de árvore + dados de `git diff --name-status`. |
| **Notas** por worktree (ex. `.arbor/notes.md`) | Memória de contexto humana | **Implementado**: ficheiro `.dcc/notes.md` na worktree; UI em `WorktreeNotesPanel` dentro de `RepoReviewSection`; comandos `worktree_read_notes` / `worktree_write_notes` (máx. 512 KiB UTF-8); cria `.dcc/` ao guardar. |
| Comentários inline de PR | Revisão colaborativa | **Implementado** (leitura): `forge_fetch_pr_review_comments` + `PrReviewCommentsPanel` na revisão; GitHub (`gh api` / REST review comments), GitLab (`glab api` / REST discussões com posição); requer `forge_link` no comb. Resposta no diff e realce do ficheiro activo. |

---

## 8. Agentes de IA e visibilidade de atividade

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| Deteção de agentes a correr (Claude Code, Codex, OpenCode, …) | Painel “quem está a trabalhar onde” | **Sim**: heurística por processo + `cwd` + metadados do pane/provider/title; eventos `terminal-activity` para a UI. |
| Estados **working / waiting** com indicadores coloridos | Mesma filosofia do sistema de atenção atual, porém mais rico | **Sim**: estender `attention-types` e badges na `CmuxWorkspacePage`. |
| **WebSocket** de atualização em tempo real | Menos polling; UI mais viva | **Opcional futuro**: hoje os eventos Tauri + polling leve já cobrem o produto; só vale reabrir se houver cliente remoto/multi-UI. |

---

## 9. Automação, tarefas agendadas e *hooks* de repo

As **`[[tasks]]` com cron** e os **triggers** pós-execução (ver Arbor) já estão ligados ao **processo em background** do daemon. *Hooks* de repo e templates em Markdown continuam como área de expansão, mas o esqueleto do agendamento já existe no DCC.

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **`[[tasks]]` com cron** (incl. segundos) no daemon | *Triage* periódico, relatórios, sync | **Já existe** um scheduler no daemon com UI para listar, executar, anexar e desanexar tarefas. |
| **Triggers** pós-execução (stdout → prompt para agente) | Automação “quando o script terminar, pedir revisão à IA” | **Parcial**: a infraestrutura está montada, mas o pipeline declarativo ainda pode ficar mais rico. |
| **Templates** Markdown em pasta do repo (`.arbor/tasks` → `.dcc/tasks`) | Presets partilháveis no repositório | **Implementado**: ficheiros `.md` em `.dcc/tasks/` (subpastas incluídas), frontmatter opcional (`title`, `command`, `description`, `cwd_mode`), corpo como prompt se `command` vazio; grupo na paleta (**⌘K**) e API `repo_list_task_templates`. |
| **Webhooks** para eventos (agent started/finished) | Integração com Slack/Discord/CI | **Ainda por fazer**: pode ser encaixado no daemon ou como *hook* opcional no app. |

---

## 10. MCP, API HTTP e CLI *headless*

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **`dcc-mcp`** (stdio) a falar com API local | Cursor/Codex/Claude Desktop orquestram worktrees/terminais via MCP | **Já existe** um servidor MCP via stdio no binário `dcc`, com tools mínimas e expansão incremental. |
| **HTTP API** + **CLI** (`arbor-cli` → `dcc`) | Scripts CI, automação remota (com token) | **CLI já existe**; a **HTTP API** já suporta modo local e remoto com auth própria e rotação de token. |
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
| **Título da janela** com branch/worktree | Orientação rápida | **Sim** — **implementado** no front: `getCurrentWindow().setTitle` (`@tauri-apps/api/window`) + `document.title`; ver `hooks/use-app-window-title.ts`. |
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
| Tokens para API remota | Mesmo padrão Arbor (`Authorization: Bearer`) | **Já existe**: modo remoto/híbrido com rotação e expiração configuráveis. |
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

**Estado atual:** os comandos listados abaixo **deixaram de retornar** `NOT_IMPLEMENTED`; o script `scripts/audit-tauri-stubs.mjs` não encontra invocações de `mapped_not_implemented` / `ApiError::not_implemented` em `src-tauri/src/main.rs`. Vale **continuar a correr** `yarn audit:tauri-stubs` em CI ou antes de releases para evitar regressões.

| Área | Comando / API | Implementação (resumo) |
|------|-----------------|-------------------------|
| App | `app_check_for_updates`, `app_quit_and_install` | `tauri-plugin-updater` (`UpdaterExt`), `plugins.updater` em `tauri.conf.json`, permissão `updater:default`; verificação devolve JSON (`available`, `currentVersion`, …); instalação via `download_and_install`. |
| Diálogo | `dialog_show_message`, `dialog_confirm` | `tauri-plugin-dialog`: mensagem com tipo/botões, confirmação Ok/Cancel; `spawn_blocking` + `blocking_show` / `blocking_show_with_result` (paridade com picker de pastas). |
| Janela | `window_is_maximized` | `get_webview_window("main")` + `is_maximized()`. |
| Licença | `license_get_status`, `license_get_machine_id`, `license_activate`, `license_skip_activation` | Estado e upsert na tabela SQLite `activation`; `machine_id` estável (SHA-256 de OS, arch, hostname, app data dir); ativação via POST `https://www.devcommandcenter.com/api/beta-activate`; *skip* só em `debug` (`license_skip_activation`). |

Os comandos de **Shell** e **Comb / Git** continuam implementados; o contrato Tauri + `desktop-bridge.ts` / `types/app.d.ts` foi alinhado aos retornos reais (ex.: `checkForUpdates`, Settings).

---

## Priorização sugerida

**P0 — Alto impacto / alinhado ao núcleo do DCC**

1. ~~Fechar o que ainda estava pendente nos comandos de app/diálogo/janela/licença~~ — **feito** (atualizações in-app, diálogos nativos, `window_is_maximized`, fluxo beta em SQLite + API); manter contrato estável e auditoria de stubs.
2. **Terminal**: addons WebGL/search, preferências de scrollback, **bell** → notificação.
3. **Confirmação de discard** com **commits não pushed**.
4. **Criptografia de Secrets**: fechar `api_key_encrypted` com keychain/credential manager.

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
| Scrollback persistente em SQLite | ✅ | `schema.sql:pane_terminal_scrollback`, `main.rs` (`persist_pane_scrollback_compressed`, `load_pane_scrollback_deque`, `terminal_clear_persisted_scrollback`) | Gzip(JSON dos chunks); throttle ~1,6s + flush ao fechar reader; `restart: true` limpa persistido; ação UI “limpar scrollback” sincroniza DB + buffer |
| OSC 52 (remote clipboard) | ✅ | `lib/terminal/osc52.ts`, `embedded-terminal.tsx` (`term.parser.registerOscHandler(52)`) | Copiar (base64 → clipboard), pedido `?` / `c;?` → `readText` + sequência OSC de volta ao PTY; fila `enqueuePtyUserInput` |
| Signal handling explícito (SIGTERM/SIGKILL) | ✅ | `terminal_send_signal` (`main.rs`), `sendSignal` na bridge, menu «Sinais» em `embedded-terminal.tsx` | Unix: `kill(-pgid, …)` ao grupo do PTY; Windows: `\x03` (SIGINT) ou `taskkill /T` (TERM) e `/F` (KILL) |
| Configuração scrollback lines no UI | ✅ | `src/pages/SettingsPage.tsx:529-551`, `lib/terminal/terminal-preferences.ts` | Campo numérico (100-50000 linhas, step 1000) na seção Terminal embutido; salvo em localStorage; aplicado ao criar novo XTerm |
| Bell → notificação desktop | ✅ | `embedded-terminal.tsx:524-567` | Handler completo: visual bell + notificações ricas com ações (Abrir painel/Dispensar), cooldown 3s anti-spam, metadata completa (paneId/combId/projectId) |
| Múltiplas abas de terminal por worktree | ✅ | `components/pane-tab.tsx`, `CmuxWorkspacePage.tsx:2096-2135` | Sistema completo de tabs: múltiplos panes por comb, context menu rename, drag-and-drop reordering com `@hello-pangea/dnd@18.0.1`, persistência de `layout_order` em SQLite |
| Deteção de "ligação lenta" | ✅ | `lib/terminal/output-metrics.ts`, `components/embedded-terminal.tsx`, `src/pages/SettingsPage.tsx` | Tracker por pane com heurística de backlog/latência; badge “Ligação lenta” no terminal e sensibilidade calibrável na UI |

---

### 2. Sessões Persistentes e Modelo "Daemon"

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Daemon sidecar (dccd) | ✅ | `src-tauri/src/bin/dccd.rs` | Processo persistente, fallback in-process |
| Attach/Detach de sessões | ✅ | `daemon_runtime.rs:1086` | RPC `daemon.attachTask`, `daemon.detachTask` |
| PTYs sobrevivem ao fecho da GUI | ✅ | `daemon_runtime.rs:614` | `DaemonService` mantém estado em memória + SQLite |
| Runtime file (daemon-runtime.json) | ✅ | `daemon_runtime.rs:83` | PID, started_at, db_path |
| RPC via SQLite (daemon_rpc_requests) | ✅ | `daemon_runtime.rs:1233` | Request loop processa até 32 requests/200ms |
| WebSocket/Event stream tempo real | 🟡 | - | Não é necessário no estado atual; eventos Tauri + polling leve cobrem a UX. Reabrir apenas para cliente remoto/multi-UI. |
| HTTP API REST | ✅ | `src-tauri/src/http_api.rs` | REST local + remota, auth `X-API-Key`/`Bearer`, OpenAPI e rotação |
| Health metrics (CPU/RAM daemon) | ✅ | `daemon_runtime.rs`, `main.rs`, `types/app.d.ts` | `daemon.health` e `daemon.getStatus` agora expõem CPU/RAM/lastMetricsAt do processo do daemon |

---

### 3. Sinais, Lifecycle e Integração com o SO

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Interrupt/Terminate/Kill explícitos | ✅ | `main.rs` `terminal_send_signal` | SIGINT/SIGTERM/SIGKILL via `terminal_send_signal`; `terminal_kill` continua a fechar o PTY |
| Grupo de processos (killpg Unix) | ✅ | `main.rs` `send_signal_to_managed_terminal` | `kill(-pgid, sig)` com fallback ao PID do filho |
| Windows job object para terminação | ✅ | `src-tauri/src/main.rs` | Sessões Windows agora nascem num Job Object com `KILL_ON_JOB_CLOSE`; `SIGTERM/SIGKILL` e `kill` usam `TerminateJobObject` com fallback para `taskkill`. |
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
| Rollback se setup falhar | ✅ | `main.rs` (`comb_ensure_worktree`, `run_repo_setup_script`, `git_remove_worktree_and_branch_best_effort`) | Após `git worktree add`, executa `setupCommand` do `.dcc.toml` no cwd do worktree; se falhar, remove worktree + branch (best-effort) e não grava `worktree_path` no SQLite; mensagem de erro inclui output do script |
| Confirmação de delete com unpushed | ✅ | `lib/comb-discard-confirmation.ts`, `CmuxWorkspacePage.tsx` | `comb_check_unpushed` + diálogo (variante destrutiva se há unpushed ou se a verificação falha) |
| Histórico de navegação entre worktrees | ✅ | `hooks/use-worktree-navigation-history.ts`, `CmuxWorkspacePage.tsx`, `workspace-command-palette.tsx` | Pilhas voltar/avançar em memória (máx. 50); `navigateToComb` nas entradas de UI; ⌘[/⌘] ou Ctrl+[/]; botões na barra do workspace; grupo na paleta (⌘K); limpeza quando combs deixam de existir |
| Última atividade Git por worktree | ✅ | `schema.sql` (`combs.last_git_activity_at`), `connection.ts` (`migrateCombsLastGitActivity`), `main.rs` (`comb_refresh_git_activity`, `git_worktree_last_activity_epoch`, `map_comb_to_renderer`), `daemon_runtime.rs` (ordenação listagens), `CmuxWorkspacePage.tsx`, `lib/format-relative-time.ts`, `desktop-bridge.ts` | Coluna + índice; heurística Git; poll leve + visibilidade; sidebar com tempo relativo (`Intl` pt); ordenação pin → git → aberto |
| Preview de branch/path sanitizado | ✅ | `main.rs` (`safe_branch_name_parts`, `comb_preview_worktree_naming`), `CmuxWorkspacePage.tsx` (`NewWorkspaceDialog`), `desktop-bridge.ts` | Pré-visualização em tempo real (debounce) da branch Git e da pasta `.dcc/worktrees/…`; sufixo hex de exemplo até existir `combId` real |

---

### 6. Issues, Forges e Contexto de PR

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Criar worktrees de issues GitHub/GitLab | ✅ | `forge_issue.rs`, `forge_fetch_issue` (`main.rs`), `CmuxWorkspacePage.tsx` (`NewWorkspaceDialog`), `desktop-bridge.ts` | URL de issue ou `owner/repo#123` (GitHub); GitLab com `/-/issues/` ou `grupo/sub/repo#123`; alinha API host com `git_remote_url`/`origin` quando aplicável; **CLI primeiro** (`gh` / `glab`) quando disponível, senão REST |
| OAuth/Token em providers | ✅ | `schema.sql:providers` | Campo `api_key_encrypted`, mas criptografia não implementada |
| Preview de nomes sanitizados | ✅ | `comb_preview_worktree_naming`, `NewWorkspaceDialog` | Mesmo diálogo: pré-visualização em tempo real; fluxo «Issue» preenche o nome e reutiliza a pré-visualização |
| Ligação automática a PRs/MRs | ✅ | `forge_issue.rs` (`resolve_open_pr_mr_for_branch`), `main.rs` (`forge_sync_pr_link`, `forge_sync_pr_link_run`, `comb_ensure_worktree`), `schema.sql` / `connection.ts` (`forge_link`), `CmuxWorkspacePage.tsx`, `desktop-bridge.ts`, `daemon_runtime.rs` (`list_combs`) | Deteção por branch + `origin`; **CLI primeiro** (`gh pr list` / `glab mr list`), senão tokens REST; UI na lista de workspaces; erros de rede não apagam ligação anterior |
| gh cli / glab integration | ✅ | `forge_issue.rs` (`run_forge_cli_stdout`, helpers `*_via_gh` / `*_via_glab`), `main.rs` (`provider_cli_command`: `gh`, `github-cli`, `glab`, `gitlab-cli`, …) | Comandos com `cwd` no path do projeto; `shell_detect_cli_for_provider` / `shell_resolve_cli_path` reconhecem forges; fallback REST se CLI ausente ou sem resultado |

---

### 7. Diff, Revisão e Árvore de Ficheiros

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Várias abas de diff | ✅ | `repo-review-section.tsx` | Estado `diffOpenTabs` / `activeDiffTabPath`; separadores com fecho; primeiro path ao recarregar lista; limite 16 separadores |
| Contagens +/- por ficheiro | ✅ | `main.rs` (`build_review_diffs_for_path`), `repo-review-section.tsx`, `diff-file-tree.tsx` | Campos `insertions`/`deletions` por ficheiro no JSON; UI na árvore e no cabeçalho do cartão |
| Lista de ficheiros + árvore expand/collapse | ✅ | `diff-file-tree.tsx`, `lib/review/diff-file-tree-model.ts`, `repo-review-section.tsx` | Pastas expand/recolher; clique abre ou foca separador de diff; destaque do ficheiro ativo |
| Notas por worktree (.dcc/notes.md) | ✅ | `main.rs` (`worktree_read_notes`, `worktree_write_notes`), `worktree-notes-panel.tsx`, `repo-review-section.tsx` | Textarea com guarda automática (debounce); ficheiro `.dcc/notes.md`; limite 512 KiB |
| Comentários inline de PR | ✅ | `forge_issue.rs`, `main.rs` (`forge_fetch_pr_review_comments`), `pr-review-comments-panel.tsx`, `repo-review-section.tsx` | Lista com path, linha, autor, corpo; destaque no ficheiro activo; **só leitura** (criar comentário no forge fica fora do MVP) |
| DiffCodeBlock | ✅ | `diff-code-block.tsx` | Syntax highlighting com Prism.js |

---

### 8. Agentes de IA e Visibilidade de Atividade

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Deteção de agentes (Claude, Codex, etc.) | ✅ | `src-tauri/src/main.rs`, `src/lib/desktop-bridge.ts`, `hooks/use-terminal-project-activity.ts`, `src/pages/CmuxWorkspacePage.tsx` | Heurística por processo + `cwd` + provider/title metadata; agregação “quem está onde” na sidebar; `terminal-activity` para refresh em tempo real |
| Estados working/waiting com badges | ✅ | `CmuxWorkspacePage.tsx` | Icons Bot, Clock3, Terminal |
| Parsing de título do terminal | ✅ | `src-tauri/src/main.rs`, `src/lib/desktop-bridge.ts`, `src/pages/CmuxWorkspacePage.tsx` | Parse de OSC 0/2 no PTY, título runtime por sessão e fallback ao título do pane para enriquecer a deteção e a visibilidade |
| Painel "quem está onde" | ✅ | `src/pages/CmuxWorkspacePage.tsx` | Sidebar e resumo do command center agregam atividade por comb com detalhes de agente |
| WebSocket atualização tempo real | 🟡 | - | Opção futura, não bloqueante; a arquitetura atual já entrega atualização reativa sem WS |

---

### 9. Automação, Tarefas Agendadas e Hooks de Repo

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| Parser de cron (5 ou 6 campos) | ✅ | `daemon_runtime.rs:253` | Suporta segundos, ranges, steps, wildcards |
| Scheduler tick loop (5s) | ✅ | `daemon_runtime.rs:1247` | `sweep_loop` verifica next_run_at |
| Execução de tasks agendadas | ✅ | `daemon_runtime.rs:614` | `create_running_task` spawn comandos |
| UI para listar/run tasks | ✅ | `workspace-command-palette.tsx` | Grupo Tasks com ícone Clock3 |
| **Triggers pós-execução (stdout → prompt)** | ✅ | `daemon_runtime.rs:2020-2071` | **IMPLEMENTADO**: Pipeline completo com suporte Anthropic + OpenAI |
| Templates Markdown em .dcc/tasks | ✅ | `main.rs` (`repo_list_task_templates`, `list_repo_task_templates_impl`), `workspace-command-palette.tsx`, `CmuxWorkspacePage.tsx` | Leitura recursiva de `*.md`; YAML frontmatter simples; limite 256 KB/ficheiro; grupo na paleta quando há ficheiros |
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
| HTTP API REST | ✅ | `src-tauri/src/http_api.rs`, `src-tauri/src/bin/dccd-http.rs` | REST local + remota com `X-API-Key`/`Bearer`, compatibilidade `/rpc` e rotação de token |
| Autenticação local | ✅ | `src-tauri/src/http_auth.rs` | Header `X-API-Key` nas rotas protegidas |
| Autenticação remota (Bearer/token) | ✅ | `src-tauri/src/http_auth.rs`, `src-tauri/src/http_config.rs` | `Authorization: Bearer`, rotação e expiração configuráveis |
| Documentação OpenAPI | ✅ | `docs/GUIA_HTTP_API.md`, `src-tauri/src/http_api.rs` | `GET /openapi.json` |

**Próximos passos recomendados nesta área**

1. Evoluir os endpoints atuais para payloads resource-oriented e tipados.
2. Consolidar clientes tipados e SDK para integrações externas.
3. Crescer os resources MCP e prompts sobre a mesma base de dados/contrato.

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
| Command palette (cmd+k) | ✅ | `workspace-command-palette.tsx` | Grupos: Global, Projeto, Workspaces, Panes, Processos, Presets, Templates (.dcc/tasks), Tasks |
| Fuzzy search | ✅ | `workspace-command-palette.tsx` | Sobre projetos, combs, panes, comandos |
| Temas partilhados | ✅ | `xterm-theme.ts` | `getXtermColorTheme`, ThemeProvider |
| Título da janela com branch/worktree | ✅ | `hooks/use-app-window-title.ts`, `CmuxWorkspacePage.tsx` | Branch · basename do worktree (ou nome do workspace) · projeto — `Dev Command Center`; fallback só projeto quando não há comb |
| Notificações desktop ricas (ações) | ✅ | `src-tauri/src/main.rs:2611-2718`, `src/lib/desktop-bridge.ts`, `hooks/use-terminal-attention-toasts.ts`, `components/embedded-terminal.tsx` | `app_show_notification` rico com `actions`, evento `notification-action` e foco da janela ao abrir o painel |
| Layout 3 painéis redimensionáveis | ✅ | `CmuxWorkspacePage.tsx` | Sidebar, terminal/agent, diffs |
| Atalhos de teclado | ✅ | `src/pages/CmuxWorkspacePage.tsx`, `components/workspace-command-palette.tsx` | `Cmd/Ctrl+K` abre a palette; `Cmd/Ctrl+Shift+N/T/A/B/R/I/P` cobre workspace, terminal, agent, base, repo, notificações e providers; `Cmd/Ctrl+Shift+D/L/S` muda o tema; `Cmd/Ctrl+Alt+T` alterna tema. |
| Hints visuais dos atalhos | ✅ | `src/pages/CmuxWorkspacePage.tsx` | Segurar `⌘` no Mac ou `Ctrl` no Linux/Windows revela os atalhos inline na sidebar e nos botões principais do workspace. |

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
| Tokens para API remota (Bearer) | ✅ | `src-tauri/src/http_auth.rs`, `src-tauri/src/http_config.rs` | `Authorization: Bearer` com rotação e expiração |
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

### 16. Stubs Tauri (gaps fechados)

| Item | Status | Referência | Notas |
|------|--------|------------|-------|
| app_check_for_updates | ✅ | `main.rs`, `tauri.conf.json` (`plugins.updater`), `capabilities/default.json` | `tauri-plugin-updater`; JSON com `available` / `checkError` / versões |
| app_quit_and_install | ✅ | `main.rs` | `check` + `download_and_install` no `Update` |
| dialog_show_message | ✅ | `main.rs` | `MessageDialogBuilder`, índice do botão |
| dialog_confirm | ✅ | `main.rs` | Ok/Cancel → `bool` |
| window_is_maximized | ✅ | `main.rs` | Janela `main` |
| license_get_machine_id | ✅ | `main.rs` (`compute_stable_machine_id`) | Hex SHA-256 |
| license_activate | ✅ | `main.rs` | POST beta-activate + SQLite `activation` |
| license_skip_activation | ✅ | `main.rs` | Só `debug_assertions`; grava `dev@local` |

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

3. ✅ **HTTP API REST** (Secção 10)
   - ✅ REST local + remota em `dccd-http`
   - ✅ Auth por `X-API-Key` no modo local e `Authorization: Bearer` no modo remoto/híbrido
   - ✅ Rotação de bearer token e OpenAPI atualizado
   - Próximo foco: recursos tipados e SDK/clientes para integrações externas

4. 🟡 **Criptografia de Secrets** (Secção 14)
   - Importante para endurecer credenciais, mas já não bloqueia o fluxo completo
   - `api_key_encrypted` usa keychain/credential manager
   - Estimativa: ~200 linhas Rust

### 🟡 **IMPORTANTE** (Completam funcionalidades existentes)

5. ✅ **Scrollback Persistente** (Secção 1) — **implementado**
   - Tabela `pane_terminal_scrollback`, gzip sobre JSON dos mesmos chunks que o buffer circular; `terminal_get_or_create_for_pane` injeta `paneId` no spawn para o reader persistir por painel.

6. ✅ **Confirmação de Delete com Unpushed** (Secção 5)
   - Helper `getCombDiscardDialogCopy` + botão de confirmação destrutivo quando há unpushed ou erro ao verificar

7. ✅ **Árvore de Arquivos para Diffs** (Secção 7) — **implementado**
   - `DiffFileTree` + modelo `buildDiffFileTree`; integração em `RepoReviewSection` (árvore + separadores de diff por ficheiro; um cartão de diff visível por separador)
   - **Notas** `.dcc/notes.md` na worktree: `WorktreeNotesPanel`, comandos `worktree_read_notes` / `worktree_write_notes`
   - **Comentários inline PR/MR**: `forge_fetch_pr_review_comments` + `PrReviewCommentsPanel` (leitura via API forge)
   - API `git_get_review_diffs`: métricas `insertions`/`deletions` por ficheiro; tipos em `types/app.d.ts`

8. ✅ **Templates de Tasks** (Secção 9) — **COMPLETO**
   - Comando Tauri `repo_list_task_templates`, ponte `desktopAPI.repo.listTaskTemplates`
   - Grupo «Templates de tarefas (.dcc/tasks)» na paleta (antes de «Tarefas agendadas»)

### 🟢 **DESEJÁVEL** (Polimento e UX)

9. ✅ **Título Dinâmico da Janela** (Secção 12) — **COMPLETO**
   - Hook `useAppWindowTitle` + `buildAppWindowTitle`; Tauri `setTitle` quando `desktopAPI` está disponível
   - Ver `hooks/use-app-window-title.ts`, integração em `CmuxWorkspacePage.tsx`

10. ✅ **Notificações Ricas** (Secção 12) — **COMPLETO**
   - `app_show_notification` aceita `icon`, `sound`, `notificationId` e `actions`
   - Eventos `notification-action` para `reply` / `dismiss` no renderer
   - `window_focus` traz a janela para frente quando o utilizador abre o painel via notificação
   - `terminal:attention` agora carrega `notificationId` estável para correlacionar cliques e histórico

11. **Webhooks** (Secção 9)
    - Eventos para Slack/Discord/CI
    - Estimativa: ~200 linhas Rust

12. **Logs Estruturados** (Secção 15)
    - Migrar de `println!` para `tracing`
    - Estimativa: ~50 linhas refactor

### 📊 **Métricas de Completude por Área**

| Área | Implementado | Parcial | Pendente | % Completo |
|------|-------------|---------|----------|------------|
| Terminal PTY | 14 | 0 | 0 | 100% |
| Daemon/Sessões | 7 | 1 | 0 | ~94% |
| Sinais/Lifecycle | 4 | 0 | 1 | 80% |
| **Processos** | **8** | **0** | **1** | **~89%** |
| Worktrees/Git | 11 | 0 | 0 | 100% |
| Issues/Forges | 5 | 0 | 0 | 100% |
| Diff/Review | 6 | 0 | 0 | 100% |
| Agentes IA | 4 | 1 | 0 | 90% |
| **Tasks Agendadas** | **7** | **0** | **1** | **~87%** (falta webhooks) |
| MCP/API/CLI | 8 | 0 | 2 | 80% |
| Acesso Remoto | 0 | 0 | 3 | 0% (Fase 2) |
| UI/UX | 8 | 0 | 0 | 100% |
| Config Repo | 10 | 0 | 2 | 83% |
| Segurança | 2 | 2 | 1 | 60% |
| Observabilidade | 0 | 2 | 2 | 25% |
| Stubs Tauri | 8 | 0 | 0 | 100% |

**TOTAL GERAL: 102 implementados + 5 parciais + 15 pendentes ≈ 83% completo (métricas aproximadas)**

---

## 🎉 Implementações Recentes

### Stubs Tauri (secção 16) — atualizações, diálogos, janela, licença — (2026-04-12) ✅

- **Rust**: `app_check_for_updates` / `app_quit_and_install` com `tauri-plugin-updater`; `dialog_show_message` / `dialog_confirm` com `tauri-plugin-dialog` e janela pai opcional; `window_is_maximized`; licença com `compute_stable_machine_id`, leitura/escrita `activation`, `license_activate` (HTTP) e `license_skip_activation` (só dev).
- **Config**: `src-tauri/tauri.conf.json` — `plugins.updater` (endpoint inicial configurável; substituir em produção pelo manifesto real e chave minisign de assinatura); `capabilities/default.json` — `updater:default`.
- **Frontend**: `types/app.d.ts`, `desktop-bridge.ts` (tipos de `checkForUpdates` / `quitAndInstall`); `SettingsPage.tsx` — verificação de atualização usa o JSON devolvido pelo invoke (em vez de depender de `onUpdateStatus`, ainda no-op).
- **Auditoria**: `yarn audit:tauri-stubs` → **0** stubs em `main.rs`.

### Comentários inline de PR/MR (Secção 7) — (2026-04-12) ✅

- **Rust**: `forge_fetch_pr_review_comments` em `main.rs` — lê `forge_link` do comb + path do projeto; `forge_issue::fetch_pr_review_comments` — GitHub: `gh api …/pulls/{n}/comments --paginate` ou REST paginado; GitLab: `glab api …/merge_requests/{iid}/discussions` ou REST paginado; normalização de campos (`path`, `line`, `body`, `author`, `url`, `createdAt`).
- **Frontend**: `components/review/pr-review-comments-panel.tsx` — painel colapsável na `RepoReviewSection` (target principal da Missão), realce dos comentários do ficheiro activo, botão atualizar.
- **Ponte**: `desktopAPI.forge.fetchPrReviewComments` em `desktop-bridge.ts`; tipos em `types/app.d.ts`.
- **Nota**: envio de novos comentários para o forge não está incluído (MVP só leitura).

### Notas por worktree `.dcc/notes.md` (Secção 7) — (2026-04-12) ✅

- **Rust**: `worktree_read_notes`, `worktree_write_notes` em `main.rs` — leitura/escrita em `{worktree}/.dcc/notes.md`, `create_dir_all` em `.dcc/`, limite 512 KiB por ficheiro.
- **Frontend**: `components/review/worktree-notes-panel.tsx` — integrado em `RepoReviewSection` (acima dos diálogos de merge); guarda com debounce; indicações «A guardar…» / «Guardado»; fallback informativo fora do Tauri.
- **Ponte**: `window.desktopAPI.worktree.readNotes` / `writeNotes` em `desktop-bridge.ts`; tipos em `types/app.d.ts`.

### Várias abas de diff (Secção 7) — (2026-04-12) ✅

- **Frontend**: `components/review/repo-review-section.tsx` — barra de separadores (`role="tablist"`) por ficheiro alterado; cada clique na `DiffFileTree` adiciona ou activa um separador; botão ✕ remove; ao mudar o conjunto de paths (reload do diff), separam-se entradas inválidas e, se a lista ficar vazia, reabre-se o primeiro path; vista principal mostra um único cartão de diff (o do separador activo), em vez da lista contínua de todos os ficheiros.
- **Constantes**: `MAX_DIFF_TABS = 16`, rótulo do separador com basename truncado e `title` com path completo.

### Worktrees a partir de issues GitHub/GitLab (Secção 6) — (2026-04-12) ✅

- **Rust**: módulo `src-tauri/src/forge_issue.rs` — parsing de URL (`/issues/`, `/-/issues/`), atalho `owner/repo#N`, pedidos `reqwest` às APIs GitHub/GitLab; comando `forge_fetch_issue` em `main.rs` (projeto via SQLite + token opcional).
- **CLI**: quando `gh` / `glab` estão no `PATH`, `forge_fetch_issue` tenta primeiro `gh issue view` / `glab issue view` com `cwd` no repositório (auth herdada do CLI); validação GitHub PR vs issue via `gh pr view`; fallback REST.
- **Auth**: `GITHUB_TOKEN`/`GH_TOKEN`, `GITLAB_TOKEN`/`GITLAB_ACCESS_TOKEN`, ou campo de PAT no diálogo (não persistido); desnecessário para o caminho CLI se já autenticado.
- **Frontend**: `desktopAPI.forge.fetchIssue`; bloco «Issue (GitHub / GitLab)» em `NewWorkspaceDialog` — preenche nome e descrição; fluxo de criação de comb e `comb_ensure_worktree` inalterados.
- **Notas**: GitLab self-hosted requer URL completo da issue; atalho `grupo/sub#N` assume `gitlab.com` por defeito.

### Ligação automática a PRs/MRs (Secção 6) — (2026-04-12) ✅

- **Dados**: coluna `combs.forge_link` (JSON), migração em `run_legacy_schema_migrations` / `migrateCombsForgeLink` / `schema.sql`; tipos `ForgePrLink` em `lib/database/types.ts` e `forgeLink` no `map_comb_to_renderer`.
- **Rust**: `resolve_open_pr_mr_for_branch` em `forge_issue.rs` — **CLI primeiro** (`gh pr list --head owner:branch`; `glab mr list --source-branch`); depois GitHub (pulls com `head`) e GitLab (merge requests por `source_branch`) via REST; comando `forge_sync_pr_link` e helper `forge_sync_pr_link_run` em `main.rs`; chamada automática no fim de `comb_ensure_worktree` (worktree novo ou já existente); em falha HTTP mantém-se o último `forge_link` gravado.
- **Frontend**: `desktopAPI.forge.syncPrLink`; lista de workspaces (`WorkspaceListItem`) com linha PR/MR + abertura externa; `useEffect` com debounce ao focar workspace com worktree para refrescar a ligação.
- **Notas**: anfitriões GitLab self-hosted sem “gitlab” no hostname podem não ser classificados na heurística REST; **webhooks** em tempo real continuam fora de âmbito.

### Integração `gh` / `glab` (Secção 6) — (2026-04-12) ✅

- **Rust**: `forge_issue.rs` — `tokio::process::Command`, `run_forge_cli_stdout`, helpers para issue/PR/MR; ordem **CLI → REST** em `fetch_issue_for_project` e `resolve_open_pr_mr_for_branch`.
- **Deteção de binários**: `main.rs` — `provider_cli_command` inclui `gh`, `github-cli`, `github_forge`, `glab`, `gitlab-cli`, `gitlab_forge` para `shell_detect_cli_for_provider` / `shell_resolve_cli_path`.

### Pré-visualização de branch/path sanitizado (Secção 5) — (2026-04-12) ✅

- **Rust**: `safe_branch_name_parts` + comando `comb_preview_worktree_naming` (prefixo via `resolve_repo_config_value`, mesmo default que `comb_ensure_worktree`; UUID fixo só para o sufixo hex de exemplo na UI).
- **Frontend**: `desktopAPI.comb.previewWorktreeNaming`; bloco «Branch e pasta (pré-visualização)» em `NewWorkspaceDialog` (`CmuxWorkspacePage.tsx`) com debounce e nota sobre o sufixo definitivo após criação do comb.

### Última atividade Git por worktree (Secção 5) — (2026-04-12) ✅

- **Dados**: Coluna `last_git_activity_at` em `combs` (schema + migração `migrateCombsLastGitActivity`); índice `idx_combs_last_git_activity`.
- **Rust**: `git_worktree_last_activity_epoch` (reflog `HEAD`, último commit, mtime do `index` em árvore dirty ou fallback); comando `comb_refresh_git_activity`; atualização em `comb_ensure_worktree`, `comb_merge_into_main`, `comb_apply_patch`; listagens ordenadas (pin → atividade Git → último aberto).
- **Frontend**: `desktopAPI.comb.refreshGitActivity`; `refreshCombs({ silent: true })` no poll para não interferir no loader; tempo relativo em `lib/format-relative-time.ts`; tooltip na sidebar.
- **Nota**: Não substitui `last_run_at` de tasks do daemon (`daemon_task_runs`); são métricas diferentes (Git por worktree vs execução agendada).

### Histórico de navegação entre worktrees (Secção 5) — (2026-04-12) ✅

- **Frontend**: Hook `useWorktreeNavigationHistory` — pilhas voltar/avançar em memória (máx. 50 cada), gravação ao mudar de comb via `navigateToComb`, limpeza de IDs quando combs são removidos.
- **UI**: Botões ‹ › na barra do workspace; grupo «Histórico de workspaces» na paleta (⌘K); atalhos **⌘[** / **⌘]** (Mac) ou **Ctrl+[** / **Ctrl+]** (Linux/Windows), sem conflito com **⌘⇧[** / **⌘⇧]** (mudança de pane).
- **Integração**: Sidebar, paleta, notificações, explorer do daemon, novo workspace e `useTerminalAttentionToasts` passam por `navigateToComb`; hidratação/localStorage e correções internas continuam com `setActiveCombId` direto (sem poluir o histórico).
- **Referências**: `hooks/use-worktree-navigation-history.ts`, `src/pages/CmuxWorkspacePage.tsx`, `components/workspace-command-palette.tsx`.

### Múltiplas Abas com Rename e Drag-and-Drop (Secção 1) — (2026-04-12) ✅

- **Frontend**: Sistema completo de tabs implementado em 2 fases:
  - **Fase 1 - Rename Tabs**: Novo componente `PaneTab` (`components/pane-tab.tsx`) com context menu (right-click), modo de edição inline com Input + botões Check/Cancel, keyboard shortcuts (Enter/Escape), validação 1-50 caracteres; handler `handleRenamePane` em `CmuxWorkspacePage` atualiza campo `title` em SQLite.
  - **Fase 2 - Drag-and-Drop**: Integração com `@hello-pangea/dnd@18.0.1` via `DragDropContext`, `Droppable`, `Draggable`; handler `handleDragEnd` reordena array e persiste `layout_order` em batch; suporte visual com `isDragging` state (opacity + scale).
- **Database**: Campos `title` e `layout_order` já existiam na tabela `panes`, nenhuma migração necessária.
- **UX**: Context menu acessível por right-click; drag handle em todo o tab; feedback visual durante drag (opacity 50%, scale 95%); toasts de sucesso/erro.
- **Referências**: `components/pane-tab.tsx`, `CmuxWorkspacePage.tsx:2096-2135` (DragDropContext), `CmuxWorkspacePage.tsx:1467-1492` (handlers).

### Bell → Notificação Desktop Completa (Secção 1) — (2026-04-12) ✅

- **Frontend**: `components/embedded-terminal.tsx:524-567` — Handler completo de bell com visual flash + notificações desktop ricas; cooldown de 3s anti-spam (ref timestamp); ações condicionais ("Abrir painel" + "Dispensar" quando há metadata completa).
- **Metadata**: Notificações carregam `paneId`, `combId`, `projectId` para navegação; `source: "terminal-bell"` para filtragem.
- **Hook**: `hooks/use-terminal-attention-toasts.ts:197-198` — Listener de `onNotificationAction` estendido para processar `terminal-bell` além de `terminal-attention`.
- **Navegação**: Ação "Abrir painel" navega automaticamente para `projectId` → `combId` → `paneId` quando usuário clica e traz a janela para frente.
- **Props**: `EmbeddedTerminal` recebe `combId` e `projectId` opcionais; `PaneCard` (`CmuxWorkspacePage.tsx`) passa metadata do workspace ativo.
- **Cooldown inteligente**: Visual bell sempre executa; notificações desktop respeitam 3s de cooldown para evitar spam.
- **Configuração**: Usuário controla via Settings → Terminal → Bell Style (`none` / `visual` / `sound` / `both`).

### Hints visuais de atalhos (Secção 12) — (2026-04-12) ✅

- **Frontend**: `CmuxWorkspacePage.tsx` detecta quando o modificador está pressionado e revela `Kbd` inline nos botões principais da sidebar e da barra superior.
- **UX**: No Mac a dica usa `⌘`; em Linux/Windows usa `Ctrl`, sem mudar o fluxo do atalho real.
- **Objetivo**: reduzir dependência de memória de curto prazo e deixar os atalhos descobertos sem abrir ajuda separada.

### Deteção de agentes e visibilidade de atividade (Secção 8) — (2026-04-12) ✅

- **Backend**: `terminal_get_project_activity` agora agrega sessões ativas com heurística por processo, `cwd`, provider metadata e fallback por árvore de processos quando possível.
- **Backend**: o reader do PTY passa a parsear OSC 0/2 e mantém o título runtime por sessão; esse contexto entra na heurística e no payload de atividade.
- **Eventos**: o host emite `terminal-activity` para reduzir o atraso da UI quando um agente entra em estado de espera.
- **Frontend**: a sidebar do workspace mostra agentes detectados por comb, com badges `working/waiting`, e agora exibe o título runtime quando existe.
- **Tipos**: `DetectedTerminalAgent` e `TerminalProjectActivity` foram adicionados ao contrato TypeScript do bridge.

### OSC 52 — clipboard remota (Secção 1) — (2026-04-11) ✅

- **Frontend**: `lib/terminal/osc52.ts` — parse do payload `Pt;Pd` (ex.: `c;…`); cópia remota com base64 UTF-8 → `navigator.clipboard.writeText`; pedido de leitura (`?` ou `c;?`) → `readText` e injecção de `\e]52;Pt;<base64>\a` no PTY via `enqueuePtyUserInput`.
- **Integração**: `components/embedded-terminal.tsx` — `term.parser.registerOscHandler(52, …)` após `open()`, `dispose` no teardown do efeito.
- **Notas**: leitura do clipboard pode falhar por permissões do WebView; nesse caso envia-se sequência OSC 52 vazia.

### Título dinâmico da janela (Secção 12) — (2026-04-11) ✅

- **Frontend**: `hooks/use-app-window-title.ts` — `buildAppWindowTitle(project, comb)`; formato `branch · pasta-do-worktree · nomeDoProjeto — Dev Command Center` (basename do path do worktree; sem worktree usa o nome do workspace); só projeto quando ainda não há comb ativo.
- **Tauri**: import dinâmico de `getCurrentWindow` de `@tauri-apps/api/window` e `setTitle`; sempre atualiza `document.title` para alinhar browser/dev e barra de título nativa.
- **Integração**: `CmuxWorkspacePage.tsx` — `windowTitleProject` (projeto do comb ou projeto selecionado na sidebar) + `useAppWindowTitle(windowTitleProject, activeComb)`.

### Templates de tarefas `.dcc/tasks` (Secção 9) — (2026-04-11) ✅

- **Rust**: `repo_list_task_templates` em `src-tauri/src/main.rs` — pasta `.dcc/tasks`, ficheiros `*.md` (árvore recursiva), frontmatter `---` … `---` com chaves `title` / `name`, `command`, `description`, `cwd_mode` (`project` \| `worktree`); se `command` estiver vazio, usa o corpo Markdown como texto inicial do painel (mesmo fluxo que presets).
- **Frontend**: estado em `CmuxWorkspacePage.tsx` (carrega ao mudar o path do projeto), `WorkspaceCommandPalette` com grupo condicional e ícone `FileText`.
- **Ponte**: `window.desktopAPI.repo.listTaskTemplates(projectPath)` em `src/lib/desktop-bridge.ts`; tipos `RepoTaskTemplate` em `lib/database/types.ts` e `types/app.d.ts`.

### Árvore de ficheiros para revisão de diffs (Secção 7) — (2026-04-11) ✅

- **Rust**: `build_review_diffs_for_path` em `src-tauri/src/main.rs` inclui `insertions` e `deletions` por ficheiro no payload JSON (reutiliza `count_diff_stats` já usado no resumo).
- **Frontend**: `components/review/diff-file-tree.tsx`, `lib/review/diff-file-tree-model.ts`; lista hierárquica com pastas expansíveis, contagens +/− por ficheiro quando aplicável, seleção e scroll até ao bloco de diff em `components/review/repo-review-section.tsx`.
- **Tipos**: `types/app.d.ts` (`getReviewDiffs` / bundle de review).

### Scrollback persistente — terminal por painel (2026-04-11) ✅

- **SQLite**: tabela `pane_terminal_scrollback` (`pane_id`, `payload_z` gzip, `updated_at`), ver `lib/database/schema.sql`.
- **Host Rust**: serialização JSON dos chunks do buffer circular → gzip (`flate2`); carregamento ao `terminal_spawn` quando há `paneId`; persistência com throttle (~1,6s), flush ao terminar o reader e ao matar o PTY; comando `terminal_clear_persisted_scrollback` alinhado à ação “limpar scrollback” na UI (`embedded-terminal.tsx` + `desktop-bridge.ts`).
- **Workspace**: `terminal_get_or_create_for_pane` passa `paneId` nas options do spawn (o reader e o UPDATE de `panes` passam a receber o id na criação do PTY).

### Notificações ricas (Secção 12) — (2026-04-11) ✅

- **Rust/Tauri**: `app_show_notification` passou a aceitar payload rico com `icon`, `sound`, `notificationId` e lista de `actions`; em desktop UNIX/BSD a ação escolhida é emitida como evento `notification-action`.
- **Frontend**: `lib/notifications.ts` normaliza payload rico; `useTerminalAttentionToasts` injeta ações `Abrir painel` / `Dispensar` e reage a `notification-action` para navegar ou marcar o alerta como lido.
- **Bridge/tipos**: `window.desktopAPI.app.onNotificationAction`, payload tipado em `types/app.d.ts`; `terminal:attention` agora carrega `notificationId` para ligar banner nativo e registo persistido.

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
