# Melhorias inspiradas no Arbor para o Dev Command Center (Rust + Tauri)

Este documento consolida **oportunidades de evolução do DCC** inspiradas no projeto [**Arbor**](https://github.com/penso/arbor) (README oficial, documentação em `docs/src` do repositório, e visão geral dos crates), **sem reescrever a UI em GPUI**: mantém-se **Tauri 2 + React + TypeScript** no front e **Rust** no host, como hoje.

**Visão de produto (âmbito desejado)**  
O DCC pretende ser **ferramenta de uso diário** o mais completa e útil possível dentro da stack Tauri. Por isso, as capacidades abaixo — em especial **daemon** (sessões persistentes, API estável), **tasks agendadas** (cron + *triggers* no daemon) e **MCP + API/CLI** — são tratadas como **objetivos de produto**, não como “extras só se alguém precisar”. A secção [Priorização sugerida](#priorização-sugerida) define **ordem de implementação** (o que vem primeiro por dependência técnica e valor imediato), não o que fica de fora por falta de interesse.

**Base no DCC atual (auditoria interna):**

- Terminal: `portable_pty` + eventos `terminal-output` / `terminal-attention` / `terminal-exit`, buffer de backlog (`terminal_get_backlog`), `TERM=xterm-256color` em Unix, integração Git em worktrees via `GIT_DIR` / `GIT_WORK_TREE`.
- UI: `xterm.js` + `FitAddon`, preferências em `localStorage`, heurística de atenção e integração com panes (`paneId`, reattach).
- Dados: SQLite (`projects`, `combs`, `panes`, `providers`).
- Worktree: `comb_ensure_worktree`, `comb_discard`, diffs de review; **`comb_merge_into_main` e `comb_apply_patch` estão como *stub* (`NOT_IMPLEMENTED`)**.
- Vários comandos Tauri mapeados mas **não implementados** (ver secção [Gaps já mapeados no código](#gaps-já-mapeados-no-código-dcc)).

Para cada área: **o que o Arbor destaca** → **benefício para o DCC** → **viável com Tauri** (sim/não/parcial) e notas de implementação de alto nível.

---

## Índice

1. [Terminal PTY, emulação e desempenho](#1-terminal-pty-emulação-e-desempenho)
2. [Sessões persistentes e modelo “daemon”](#2-sessões-persistentes-e-modelo-daemon)
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
| **Sessões de terminal que sobrevivem ao fecho da GUI** | Agentes longos (`npm test`, servidores, pipelines) continuam após fechar a janela | **Sim**, com arquitetura extra: processo **sidecar** (`dcc-d` ou serviço) que detém PTYs e expõe socket/pipe/HTTP local; o Tauri reconecta e **reattacha** streams. |
| **Attach / detach** explícitos | Fluxo claro para “deixar a correr em background” | **Sim**: comandos `session_attach`, `session_detach`, lista de sessões por projeto/comb. |
| Um **daemon** alimenta GUI, Web e CLI | Um único modelo de verdade para automações e integrações | **Sim** (grande): **planeado como núcleo da plataforma**; entrega **modular** (ex.: **API local** mínima primeiro, depois sessões persistentes e streams). |

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

As **`[[tasks]]` com cron** e os **triggers** pós-execução (ver Arbor) dependem de um **processo em background** que não morre com a janela — ou seja, encaixam no **objetivo de daemon** (secção 2). *Hooks* de repo e templates em Markdown podem começar **antes**, no próprio app; o agendamento tipo Arbor é **fase seguinte**, não “opcional por falta de valor”.

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **`[[tasks]]` com cron** (incl. segundos) no daemon | *Triage* periódico, relatórios, sync | **Sim** com crate `cron`/`tokio` num processo de fundo; UI para listar/pausar. |
| **Triggers** pós-execução (stdout → prompt para agente) | Automação “quando o script terminar, pedir revisão à IA” | **Sim**: pipeline declarativo no config. |
| **Templates** Markdown em pasta do repo (`.arbor/tasks` → `.dcc/tasks`) | Presets partilháveis no repositório | **Sim**: ler ficheiros + mostrar na paleta de comandos. |
| **Webhooks** para eventos (agent started/finished) | Integração com Slack/Discord/CI | **Sim**: HTTP client a partir do daemon ou *hook* opcional no app. |

---

## 10. MCP, API HTTP e CLI *headless*

| Inspiração Arbor | Benefício no DCC | Tauri |
|------------------|------------------|-------|
| **`dcc-mcp`** (stdio) a falar com API local | Cursor/Codex/Claude Desktop orquestram worktrees/terminais via MCP | **Sim**: espelhar *tools* mínimas: listar combs, criar worktree, escrever no PTY, obter diff. |
| **HTTP API** + **CLI** (`arbor-cli` → `dcc`) | Scripts CI, automação remota (com token) | **Sim**: **mesmo daemon** que serve MCP — API e CLI como superfícies adicionais, não alternativas descartáveis. |
| **Recursos MCP** (snapshot do daemon, *prompts* de workflow) | Onboarding consistente para agentes | **Sim**. |

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
| Tokens para API remota | Mesmo padrão Arbor (`Authorization: Bearer`) | **Sim** quando houver HTTP daemon. |
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

Comandos que hoje retornam **`NOT_IMPLEMENTED`** em `src-tauri/src/main.rs` (alinhado a `scripts/audit-tauri-stubs.mjs`):

| Área | Comando / API |
|------|----------------|
| App | `app_check_for_updates`, `app_quit_and_install` |
| Diálogo | `dialog_show_message` |
| Shell | `shell_open_external`, `shell_open_path`, `shell_show_item_in_folder` |
| Janela | `window_minimize`, `window_maximize`, `window_close` |
| Licença | `license_activate`, `license_skip_activation` |
| Comb / Git | **`comb_merge_into_main`**, **`comb_apply_patch`** |

Completar estes itens **remove fricção** e, no caso de **merge/apply patch**, desbloqueia fluxos de trabalho centrais para um *command center*.

---

## Priorização sugerida

**P0 — Alto impacto / alinhado ao núcleo do DCC**

1. Implementar **`comb_merge_into_main`** e **`comb_apply_patch`** (ou delegar claramente a `git_apply_worktree_patch` com UX unificada).
2. Completar **stubs de shell** (`openExternal`, `openPath`, `showItemInFolder`) e **janela** — melhoram integração com o SO no dia a dia.
3. **Terminal**: addons WebGL/search, preferências de scrollback, **bell** → notificação.
4. **Confirmação de discard** com **commits não pushed**.

**P1 — Diferenciação forte**

5. **Processos gerenciados** (Procfile + `dcc.toml`) com ligação a panes.
6. **Config de repo** unificada (presets, scripts setup/teardown, regras de branch).
7. **Command palette** global.
8. **Deteção de agentes** a correr + estados na UI.

**P2 — Plataforma e ecossistema** *(objetivos confirmados; ordem: depois de P0/P1 por dependência — sobretudo daemon)*

9. **Daemon** + sessões persistentes + attach/detach (base para tasks agendadas e MCP).
10. **Tasks agendadas** (`[[tasks]]` + *triggers*) no daemon, com UI de gestão.
11. **`dcc-mcp`** + API HTTP mínima + CLI no mesmo daemon.
12. **Issues/PR** integrados com GitHub/GitLab.
13. **Acesso remoto** / multi-host (secção 11).

---

## Nota final

O Arbor demonstra que um **produto “agentic coding”** completo combina: **terminal sério**, **worktrees**, **processos**, **automação**, **daemon** e **MCP**. O DCC já cobre parte disso com **SQLite + Tauri + worktrees + atenção no terminal**. A lista acima traduz essas capacidades para **incrementos realistas** na stack — **sem GPUI** — com **paridade funcional** como **direção explícita**: **daemon**, **tasks agendadas** e **MCP** não são “só se alguém quiser”; entram na **ordem certa** (P2 após fundações), por **viabilidade técnica**, não por falta de benefício para o dia a dia.
