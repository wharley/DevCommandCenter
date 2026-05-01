# DCC v2 — Refatoração para Monorepo (Tauri + React + Rust em camadas)

## Context

O DevCommandCenter (DCC) tem stack moderna (Tauri 2.10 + React 19 + Vite 6 + Radix + Tailwind 4) mas a organização do código mostra dívida arquitetural significativa: `src/pages/CmuxWorkspacePage.tsx` com **~3.500 linhas / 145 KB**, `src-tauri/src/main.rs` com **~8.600 linhas**, pasta `components/` (78 arquivos) misturando UI primitiva, dialogs e features sem separação clara, ausência de testes, e bridge frontend↔Rust sem tipo-segurança gerada (`src/lib/desktop-bridge.ts` com 19 KB).

A meta é **migrar o DCC para uma arquitetura em monorepo em camadas**, com:

- **Frontend (`apps/desktop`)** seguindo a UX/shell do **Helmor** (Tauri + React + TanStack Query + feature folders).
- **Domain model + contracts** seguindo o modelo mental do **t3code** (boundaries claros, schemas tipados, lógica fora da UI), **sem importar a parte Electron**.
- **Core operacional em Rust em camadas** (`dcc-core` em hexagonal, `dcc-infra` como adapters, `dcc-providers` como capabilities), com Tauri como bridge fina (`dcc-tauri`).
- **Providers de agente como capabilities trocáveis**: Claude Code, Codex, Gemini (estáveis) e Cursor (experimental).

> **Tese arquitetural**: T3 Code é referência de **organização de sistema** (monorepo, contracts, event model, separação UI/lógica). Helmor é referência de **forma de produto desktop** (shell Tauri, UX de workbench, feature folders). Rust é o **motor operacional** (processos, git, sessions, providers). Tauri é apenas a **borda** (commands + events). Nada do Electron de t3code (preload IPC, electron-updater, lifecycle main-process) é portado.

---

## Princípios não-negociáveis

1. **Hexagonal em Rust**: `dcc-core` define `ports` (traits). `dcc-infra` e `dcc-providers` implementam. UI fala com `dcc-tauri`, que fala com `dcc-core::application` (use cases). `dcc-core` **não conhece** Tauri, SQLite, git2, ou nenhum provider concreto.
2. **Capabilities, não implementações**: a UI dispara `Capability::SendInput` em uma `Box<dyn Provider>`. Trocar Claude por Gemini é config, não código.
3. **Bridge fina**: `dcc-tauri` só traduz `#[tauri::command]` → use case + serializa eventos. Zero lógica de domínio.
4. **Contracts gerados, não escritos à mão**: tipos em `dcc-core` derivam `specta::Type`; `packages/contracts` recebe TypeScript gerado. `desktop-bridge.ts` (atual) é descartado.
5. **Event sourcing seletivo**: apenas `sessions` (turns/checkpoints/replay). Demais entidades em SQLite normal.
6. **YAGNI agressivo em packages e crates**: começar com o mínimo viável; extrair só quando houver dor real (build lento, segundo consumidor, equipe paralela).
7. **Migração feature-a-feature, sem big bang**. Legado do DCC roda lado-a-lado até cada feature ter equivalente funcional.

---

## Stack-alvo

| Camada | Atual no DCC | Alvo | Razão |
|---|---|---|---|
| Runtime desktop | Tauri 2.10 + tokio | **Mantido** | Não-bloqueante, baixa memória. Único motivo de escolha. |
| UI | React 19 + Radix + shadcn + Tailwind 4 | **Mantido + reorganizado** | Mesmo Helmor. |
| Server state | Zustand + queries custom | **TanStack Query 5** + queryOptions factories | Padrão Helmor. Invalidação por evento Tauri → `invalidateQueries`. |
| Client state | Zustand (store gigante) | **Zustand modular** (1 store por domínio) | Padrão t3code: `projectsStore`, `threadsStore`, `composerStore`. Sem god-store. |
| Routing | react-router-dom (1 rota só) | **Sem router** na navegação principal, só composição de shell | Padrão Helmor. Isso não elimina navegação; só evita uma árvore de rotas desnecessária. Re-avaliar só se deep-linking virar requisito real. |
| Forms | react-hook-form + zod | **Mantido** | OK. |
| Terminal | xterm 5.3 | **xterm 6** | Alinha com Helmor; renderer mais performático. |
| Animação | Tailwind animate | **+ motion 12** | Padrão Helmor. |
| Variants | shadcn manual | **+ class-variance-authority** explícito | Padrão Helmor. |
| Editor rich | Nenhum | **Adiar para depois da Fase 3** | Lexical/Monaco entram quando houver requisito real. |
| Bridge tipada | `desktop-bridge.ts` manual | **`specta` + `tauri-specta`** gerando `packages/contracts/` | Single source of truth. Mata classe inteira de bugs. |
| Testes | Nenhum | **Vitest colocado** + `cargo test` por crate | Padrão Helmor. |
| Lint | (verificar) | **Biome 2** | Mais rápido que ESLint; padrão Helmor. |
| Package mgr | yarn | **Mantido (yarn workspaces)** | Sem troca de Bun no escopo. |
| Monorepo orchestrator | Nenhum | **Turbo 2** | Build cache cross-crate/package. Padrão t3code. |

---

## Estrutura-alvo do monorepo

```
dcc/                                # raiz do monorepo
├── apps/
│   └── desktop/                    # ÚNICO app: Tauri + React
│       ├── src/                    # Frontend React
│       │   ├── App.tsx             # Shell mínimo (providers + outlet)
│       │   ├── main.tsx
│       │   ├── styles/color-theme.css        # tokens OkLch (helmor)
│       │   ├── components/
│       │   │   ├── ui/             # primitivos shadcn PUROS
│       │   │   ├── chrome/         # TitleBar, StatusBar
│       │   │   └── icons/
│       │   ├── features/           # feature folders (helmor pattern)
│       │   │   ├── workspaces/
│       │   │   │   ├── index.tsx           # apresentação (memo)
│       │   │   │   ├── container.tsx       # useQuery + orquestração
│       │   │   │   ├── store.ts             # Zustand local (UI ephemera)
│       │   │   │   ├── hooks/
│       │   │   │   ├── *.test.tsx
│       │   │   │   └── types.ts             # re-export de @dcc/contracts
│       │   │   ├── terminal/
│       │   │   ├── review/
│       │   │   ├── composer/                 # textarea simples até Fase 3+
│       │   │   ├── conversation/
│       │   │   ├── inspector/
│       │   │   ├── navigation/
│       │   │   ├── settings/
│       │   │   ├── shortcuts/
│       │   │   ├── onboarding/
│       │   │   └── updater/
│       │   ├── shell/              # orquestração de layout (helmor)
│       │   │   ├── layout.ts
│       │   │   ├── use-zoom.ts
│       │   │   ├── use-panels.ts
│       │   │   └── types.ts
│       │   ├── lib/
│       │   │   ├── api.ts          # invoke wrapper fino sobre tauri-specta
│       │   │   ├── query-client.ts # createDccQueryClient + queryOptions factories
│       │   │   └── errors.ts
│       │   └── hooks/              # hooks reusáveis cross-feature
│       └── src-tauri/              # binário Tauri (consome crates do workspace)
│           ├── src/main.rs         # ~10 linhas: chama dcc_tauri::run()
│           ├── Cargo.toml          # depende de dcc-tauri
│           └── tauri.conf.json
│
├── packages/                       # MÍNIMO viável
│   ├── contracts/                  # TypeScript GERADO de dcc-core via specta
│   │   ├── src/generated/          # tauri-specta output (não editar à mão)
│   │   ├── src/index.ts            # re-exports + manuais (se houver)
│   │   └── package.json            # @dcc/contracts
│   └── config/                     # tsconfig.base, biome.json, vitest.config.base
│
├── crates/                         # MÍNIMO viável (4 crates)
│   ├── dcc-core/                   # domain + application + ports (hexagonal)
│   │   └── src/
│   │       ├── domain/             # entidades, value objects, regras
│   │       │   ├── workspace.rs    # WorkspaceState, WorkspaceStatus enums
│   │       │   ├── session.rs      # Session, Turn, Checkpoint
│   │       │   ├── project.rs
│   │       │   ├── thread.rs
│   │       │   └── provider.rs     # ProviderId, Capabilities, ProviderEvent
│   │       ├── application/        # use cases (orquestram domain via ports)
│   │       │   ├── start_thread.rs
│   │       │   ├── send_turn.rs
│   │       │   ├── attach_file_reference.rs
│   │       │   ├── create_worktree_for_task.rs
│   │       │   ├── resume_session.rs
│   │       │   └── abort_run.rs
│   │       ├── ports/              # traits que infra/providers implementam
│   │       │   ├── repository.rs   # WorkspaceRepo, SessionRepo, ProjectRepo
│   │       │   ├── git.rs          # GitOps trait
│   │       │   ├── fs.rs           # FsResolver trait (@file/@folder)
│   │       │   ├── process.rs      # ProcessSupervisor trait
│   │       │   ├── events.rs       # EventBus trait
│   │       │   └── provider.rs     # Provider trait
│   │       └── lib.rs
│   │
│   ├── dcc-infra/                  # implementações de ports (extrair quando doer)
│   │   └── src/
│   │       ├── git/                # impl de GitOps via git2 ou subprocess
│   │       ├── fs/                 # impl de FsResolver
│   │       ├── process/            # impl de ProcessSupervisor (portable-pty + tokio)
│   │       ├── db/                 # impl de *Repo via rusqlite + r2d2
│   │       ├── events/             # impl de EventBus (in-process broadcast + sqlite log)
│   │       └── lib.rs
│   │
│   ├── dcc-providers/              # adapters de CLI de agentes
│   │   └── src/
│   │       ├── common/
│   │       │   ├── stream.rs       # parser de JSONL, backpressure
│   │       │   └── handle.rs       # SessionHandle wrapper
│   │       ├── claude_code/        # estável
│   │       ├── codex/              # estável (@openai/codex)
│   │       ├── gemini/             # estável (Google Gemini CLI)
│   │       ├── cursor/             # EXPERIMENTAL — cursor-agent CLI
│   │       └── lib.rs              # registro: ProviderRegistry::default()
│   │
│   └── dcc-tauri/                  # bridge fina: commands + events
│       └── src/
│           ├── commands/           # #[tauri::command] handlers
│           │   ├── workspace_commands.rs
│           │   ├── session_commands.rs
│           │   ├── provider_commands.rs
│           │   ├── git_commands.rs
│           │   ├── settings_commands.rs
│           │   ├── system_commands.rs
│           │   └── common.rs       # CmdResult<T>, run_blocking
│           ├── events/             # forwarding de EventBus → tauri::emit
│           ├── run.rs              # tauri::Builder, plugins, generate_handler!
│           └── lib.rs
│
├── docs/                           # MDX/Astro — FORA do workspace TS
├── turbo.json
├── package.json                    # workspaces: ["apps/*", "packages/*"]
├── Cargo.toml                      # [workspace] members = ["crates/*", "apps/desktop/src-tauri"]
├── biome.json
└── tsconfig.base.json
```

### O que NÃO entra na Fase 0 (extrair quando doer)

- `apps/web` — descartado. DCC é local-first com PTY/git. Web companion duplica auth/sync. Se um dia precisar de "dashboard remoto", `apps/server` em Rust expondo HTTP read-only.
- `apps/docs` — `docs/` na raiz, fora do workspace TS.
- `packages/composer`, `packages/sdk`, `packages/state`, `packages/ui` — vivem dentro de `apps/desktop/src/` até aparecer um segundo consumidor real.
- `crates/dcc-events`, `dcc-sessions`, `dcc-git`, `dcc-fs`, `dcc-process`, `dcc-search` — vivem como módulos dentro de `dcc-infra` até a complexidade justificar split. Regra: **nunca crie crate por especulação**.
- `crates/dcc-cli` — extrair quando CLI for produto público.

### Fluxo de dependências (rígido)

```
apps/desktop/src ─→ @dcc/contracts (gerado)
apps/desktop/src ─→ invoke()/listen() ─→ apps/desktop/src-tauri ─→ crates/dcc-tauri
                                                                       │
                                                                       ▼
                                              crates/dcc-core::application (use cases)
                                                                       │
                                              ┌────────────────────────┼────────────────────────┐
                                              ▼                        ▼                        ▼
                                   ports (traits) ←─ dcc-infra    ports ←─ dcc-providers    ports ←─ dcc-tauri::events
```

`dcc-core` **só** depende de `serde`, `thiserror`, `async-trait`, `tokio` (traits). Zero deps de I/O concreto.

---

## Providers — design e roadmap

```rust
// crates/dcc-core/src/ports/provider.rs
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> Capabilities;
    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle>;
    async fn send_input(&self, h: &SessionHandle, input: Input) -> Result<()>;
    fn stream_events(&self, h: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>>;
    async fn cancel(&self, h: &SessionHandle) -> Result<()>;
    async fn resume(&self, prev: SessionId) -> Result<SessionHandle>;
    async fn healthcheck(&self) -> Result<HealthStatus>;
}

pub struct Capabilities {
    pub streaming: bool,
    pub mcp: bool,
    pub tools: bool,
    pub vision: bool,
    pub resumable: bool,
    pub experimental: bool,
}
```

| Provider | Stack | Estabilidade | Notas |
|---|---|---|---|
| `claude_code` | `claude` CLI (Anthropic) | Estável | JSONL streaming, MCP, tools. Já usado no DCC atual. |
| `codex` | `@openai/codex` CLI | Estável | Padrão Helmor. |
| `gemini` | `@google/gemini-cli` | Estável | Open-source, MCP, ReAct. Adapter análogo a Claude Code. |
| `cursor` | `cursor-agent` CLI | **Experimental** | Beta, protocolo instável. Gated por feature flag `experimental-cursor`. UI esconde se `capabilities().experimental` e flag desligada. |

**Isolamento do Cursor**: módulo próprio, tipos próprios para parser JSONL (não compartilhar struct com outros providers). Se Cursor mudar API, o estrago fica contido.

---

## Event sourcing — escopo cirúrgico

| Entidade | Persistência | Por quê |
|---|---|---|
| `Session`, `Turn`, `Checkpoint` | **Event-sourced** (event log em SQLite + projeções) | Replay de runs de agente, undo, auditoria, debugging. Histórico É o produto. |
| `Workspace`, `Project`, `Settings`, `Provider`, `RepoConfig` | **CRUD normal** (rusqlite, schema versionado) | Zero benefício de event sourcing; custo de migrations não justifica. |

`dcc-infra::events` implementa o `EventBus` trait com:
- in-process broadcast (`tokio::sync::broadcast`) para tempo real (UI subscribe via Tauri event)
- append-only log SQLite para sessions (replay/resume)

---

## Geração de contracts (`packages/contracts`)

Setup:
```toml
# crates/dcc-core/Cargo.toml
[dependencies]
specta = { version = "2", features = ["serde", "chrono"] }
tauri-specta = { version = "2", features = ["typescript"] }
```

```rust
// dcc-core domain types
#[derive(Serialize, Deserialize, specta::Type)]
pub enum WorkspaceState { Initializing, SetupPending, Ready, Archived }
```

```rust
// dcc-tauri/src/run.rs — durante build dev/CI
tauri_specta::Builder::<tauri::Wry>::new()
    .commands(collect_commands![...])
    .events(collect_events![...])
    .typ::<WorkspaceState>()
    .typ::<SessionHandle>()
    .export(Typescript::default(), "../packages/contracts/src/generated/bindings.ts")
    .expect("Failed to export bindings");
```

Resultado: `packages/contracts/src/generated/bindings.ts` com **types + funções tipadas** que substituem `desktop-bridge.ts`. PRs que mudam tipos no Rust regeneram o arquivo; CI falha se `bindings.ts` não bater com source. Single source of truth real.

---

## Faseamento de migração

**Fase 0 — Andaimes do monorepo**

Objetivo: criar a nova espinha dorsal sem quebrar o app atual, mantendo `src/` vivo até o novo shell passar no primeiro corte.

**0a — Workspace + contracts + base Rust/TS (concluída em compile)**

1. Criar `package.json` com `yarn workspaces` e `turbo.json`, sem trocar o gerenciador de pacotes.
2. Criar `Cargo.toml` de workspace e os diretórios `apps/desktop`, `packages/contracts`, `packages/config`, `crates/{dcc-core,dcc-infra,dcc-providers,dcc-tauri}`.
3. Configurar `specta`/`tauri-specta` para gerar `packages/contracts/src/generated/`.
4. Definir os primeiros tipos e traits em `dcc-core/src/{domain,ports,application}/` com implementação mínima.
5. Introduzir `vitest` + helper de render, e `cargo test` no workspace.

**0b — Coexistência do legado + novo shell (fechada)**

1. Mover `src/` atual para `legacy/`, preservando build e dev como antes.
2. Criar `apps/desktop/src/{App,main}.tsx` mínimo com `QueryClientProvider`, `Toaster` e `ThemeProvider`.
3. Adicionar `@tanstack/react-query@5` e `@tanstack/react-query-persist-client` como base do novo shell.
4. Apontar o boot principal para o novo shell, sem flag de alternância.
5. Validar que o app principal sobe pelo novo shell enquanto o legado fica preservado em `legacy/`.

**Status de execução**

- Concluído: workspace Yarn + Cargo + Turbo, `packages/config`, `packages/contracts`, `crates/dcc-core`, `crates/dcc-infra`, `crates/dcc-providers`, `crates/dcc-tauri`.
- Concluído: `legacy/` criado a partir de `src/`, preservado como base histórica enquanto o shell novo vira o caminho principal.
- Concluído: novo shell inicial em `apps/desktop` com query client, tema, layout base e entrada Vite separada.
- Concluído: boot principal agora aponta para o shell novo, sem alternância por flag.
- Concluído: `yarn install`, `yarn vite:build`, `cargo check -p dcc-core` e `cargo check -p dcc-tauri` passam no workspace atual.
- Concluído: `yarn build:contracts` passa no pacote `@dcc/contracts`.
- Concluído: Fase 1 shell + UI primitiva, com base visual, primitives e feature folders já portados.
- Em andamento: Fase 2 já tem o fluxo de criar workspace ligado do shell ao Rust e o contrato agora é gerado por `build.rs` via `tauri-specta`.
- Em aberto: smoke test manual runtime do fluxo completo de criação de workspace no app.
- Fora do escopo de 0b: providers e adapters Rust de verdade, que entram nas Fases 2 e 3.
- Concluído: Fase 3 fechada com `dcc-core::domain::session`, event log, provider runtime bridge, stream de eventos e cockpit de sessão no shell.
- Pausa atual: Fase 3 concluída; o próximo foco é UX para aproximar ainda mais o shell do Helmor, com 0a concluída em compile, 0b fechada no boot principal, Fase 1 fechada e Fase 2 validada no core.

**Objetivo das Fases 1-3**

Trazer o melhor do Helmor na forma de shell/UX e o melhor do t3code na forma de contratos, boundaries e fluxo de eventos, mas com Tauri + Rust como motor e fronteira do sistema.

**Fase 1 — Shell + UI primitiva (concluída)**
1. Copiar `components/ui/*` (shadcn) para `apps/desktop/src/components/ui/`.
2. Implementar `apps/desktop/src/styles/color-theme.css` com tokens do shell.
3. Portar `shell/{layout,use-panels,use-zoom}.ts` e a estrutura visual base.
4. Subir a feature `workspaces/` como casca vazia, pronta para receber dados.

**Status da Fase 1**

- Concluído: `apps/desktop/src/components/ui/{button,badge,input,label,separator,textarea,tooltip,dropdown-menu,scroll-area,dialog,popover,command,command-popover,card,tabs,switch}.tsx`.
- Concluído: `apps/desktop/src/styles/color-theme.css` com tokens e composição visual do shell.
- Concluído: `apps/desktop/src/lib/query-client.ts` com persister e keys no padrão do shell.
- Concluído: `apps/desktop/src/shell/{layout,use-panels,use-zoom}.ts`.
- Concluído: `apps/desktop/src/features/workspaces/` extraído com hook, sidebar, tipos e command palette.
- Concluído: `apps/desktop/src/App.tsx` refeito como shell principal com sidebar, divider, topbar, tabs, cards, composer e command palette.
- Concluído: `apps/desktop/src/features/workspaces/` ganhou controle de archived-workspaces via `Switch`.

**Fase 2 — Primeiro use case end-to-end: criar workspace (em andamento)**
1. Implementar `dcc-core::application::create_workspace_for_repo` com dois passos: prepare e finalize.
2. Definir `dcc-core::ports::{WorkspaceRepo, GitOps, EventBus}` e suas primeiras entidades de domínio.
3. Conectar `dcc-infra::db::SqliteWorkspaceRepo` e `dcc-infra::git::CommandGitOps`.
4. Expor o fluxo em `dcc-tauri::commands::workspace_commands` e consumir via `@dcc/contracts` gerado por `build.rs` + `tauri-specta`.
5. Validar o ciclo completo: UI → Tauri → Rust core → infra → evento → React Query → UI.
6. Usar essa fase como gate: se o fluxo não ficar limpo, corrigir antes de crescer o resto.

**Status da Fase 2**

- Concluído: `dcc-core::application::create_workspace_for_repo` com `prepare` + `finalize` e eventos `WorkspacePrepared`/`WorkspaceReady`.
- Concluído: `dcc-infra::db::SqliteWorkspaceRepo` com persistência SQLite do workspace.
- Concluído: `dcc-infra::git::CommandGitOps` preparando worktree no disco.
- Concluído: `dcc-tauri` como bridge do comando e state mínimo para o host Tauri.
- Concluído: `src-tauri` registrando `create_workspace_for_repo` no `invoke_handler`.
- Concluído: shell novo com dialog de criação de workspace e atualização local da lista.
- Concluído: smoke test Rust do fluxo `create_workspace_for_repo` com fakes de repo/git/event bus.
- Em aberto: smoke test manual runtime do fluxo completo de criação de workspace no app.

**Fase 3 — Providers + sessions (concluída)**
1. Introduzir `dcc-core::domain::session` com event log e projeções.
2. Implementar `dcc-core::application::{start_thread, send_turn, abort_run, resume_session}`.
3. Conectar `dcc-providers::{claude_code, codex, gemini}` como capabilities estáveis.
4. Deixar `dcc-providers::cursor` isolado atrás de feature flag.
5. Fazer `conversation/` consumir stream de eventos via Tauri `listen`.

**Status da Fase 3**

- Concluído: `dcc-core::domain::session` com `SessionEventKind`, `SessionEventRecord` e `SessionProjection`.
- Concluído: `dcc-core::application::{start_thread, send_turn, abort_run, resume_session}`.
- Concluído: `packages/contracts/src/generated/bindings.ts` já exporta o modelo de sessão e a surface de comandos gerada por `tauri-specta`.
- Concluído: `apps/desktop/src/lib/session-api.ts` consumindo os contracts de sessão de forma tipada.
- Concluído: `dcc-providers` com catálogo tipado, healthcheck por CLI e surface estável de capabilities para `claude_code`, `codex`, `gemini` e `cursor`.
- Concluído: provider runtime bridge com processo interativo, stdin/stdout e emissão de `SessionTurnDelta`/`SessionTurnCompleted` via Tauri.
- Concluído: stream de eventos de sessão chegando ao shell via `listen` em `apps/desktop/src/features/sessions/session-event-feed.tsx`.
- Concluído: `apps/desktop/src/features/providers/provider-catalog-card.tsx` exibindo o estado do catálogo no runtime.
- Concluído: `session.startThread`, `session.sendTurn`, `session.abortRun` e `session.resumeSession` expostos no Tauri bridge com estado de sessão ativo no shell.
- Concluído: o runtime shell ganhou um workbench de sessão/composer mais próximo do Helmor, com thread timeline, provider selection e resumo de projection.
- Concluído: protocolos runtime específicos podem evoluir depois, mas a Fase 3 já está fechada com o bridge CLI genérico e o fluxo funcional ponta a ponta.
- Em andamento: UX pass 1 do shell/runtime para aproximar ainda mais o comportamento visual do Helmor sem mexer no core.
- Em andamento: UX pass 2 do shell/runtime com thread temporal, auto-scroll e affordance de latest event no estilo Helmor.

**Fase 4 — Features restantes (iterativo)**
1. Implementar na ordem: `terminal` → `review` → `inspector` → `composer` → `navigation` → `settings` → `shortcuts` → `onboarding` → `updater`.
2. Manter cada feature pequena, com container + view + tests.
3. Só extrair mais crate/package se houver consumidor real ou dor concreta.

**Fase 5 — Sunset legacy + fechamento**
1. Remover `legacy/` quando a paridade funcional estiver fechada.
2. Reavaliar extrações só se `dcc-infra` ou `sessions` ficarem grandes demais.
3. Tratar editor rich como etapa posterior, não como pré-requisito.

---

## Arquivos críticos a criar (com referências de código)

| Novo arquivo | Inspirado em | O que faz |
|---|---|---|
| `apps/desktop/src/lib/api.ts` | `helmor/src/lib/api.ts` + bindings de tauri-specta | Wrapper fino que importa funções tipadas geradas; adiciona toasts em erro. |
| `apps/desktop/src/lib/query-client.ts` | `helmor/src/lib/query-client.ts:1-580` | `createDccQueryClient()` com focusManager em `tauri://focus`/`blur`; `dccQueryKeys` namespace; factories `xQueryOptions(id)`. |
| `apps/desktop/src/styles/color-theme.css` | `helmor/src/styles/color-theme.css:1-200` | Tokens OkLch (`:root` light + `.dark`), `color-mix()` para variações, semantic colors. |
| `apps/desktop/src/shell/layout.ts` | `helmor/src/shell/layout.ts` | Constantes (MIN_SIDEBAR_WIDTH, RESIZE_HIT_AREA), helpers de navegação. |
| `apps/desktop/src/features/shortcuts/registry.ts` | `helmor/src/features/shortcuts/registry.ts` | Lista declarativa de atalhos + scopes; `getShortcut(id)` lookup. |
| `crates/dcc-core/src/ports/provider.rs` | (próprio) | Trait `Provider` + `Capabilities` + `SessionHandle` + `ProviderEvent`. |
| `crates/dcc-tauri/src/commands/common.rs` | `helmor/src-tauri/src/commands/common.rs` | `pub type CmdResult<T> = Result<T, CmdError>`; `run_blocking` helper. |
| `crates/dcc-tauri/src/commands/workspace_commands.rs` | `helmor/src-tauri/src/commands/workspace_commands.rs:24-72` | Two-phase pattern: `prepare_workspace_from_repo` + `finalize_workspace_from_repo`. Emite `workspace-changed`. |

## Arquivos a preservar (e mover do DCC atual)

| Atual | Destino | Notas |
|---|---|---|
| `lib/terminal/{attention-heuristic,attention-types,output-metrics}.ts` | `apps/desktop/src/features/terminal/attention/` | Lógica preservada, organizada por feature. |
| `lib/database/types.ts` | **Descartado** | Substituído por `@dcc/contracts/generated/bindings.ts`. |
| `components/ui/*` (57 arquivos shadcn) | `apps/desktop/src/components/ui/` | Cópia direta. |
| `hooks/use-terminal-attention-toasts.ts` | `apps/desktop/src/features/terminal/hooks/` | |
| `hooks/use-worktree-navigation-history.ts` | `apps/desktop/src/features/navigation/hooks/` | |
| `src-tauri/src/daemon_runtime.rs`, `daemon_client.rs` | `crates/dcc-infra/src/daemon/` (ou `dcc-tauri/src/daemon/` se for sidecar-only) | Avaliar destino correto na Fase 0. |
| `src-tauri/src/forge_issue.rs` | `crates/dcc-infra/src/forge/issue.rs` | |
| `src-tauri/src/http_*.rs` | `crates/dcc-tauri/src/http/` (ou crate `dcc-http` se ficar denso) | |

## A descartar / decompor

- `src/pages/CmuxWorkspacePage.tsx` (3.500 linhas) → decomposto em 8+ features sob `apps/desktop/src/features/`. Some no fim.
- `src/pages/DashboardPage.tsx` → avaliar; se sem uso real, deletar.
- `src/lib/desktop-bridge.ts` (19 KB) → **descartado** completamente; substituído por contracts gerados.
- `src-tauri/src/main.rs` (8.6k linhas) → migrado para `crates/dcc-{core,infra,tauri}`. `apps/desktop/src-tauri/src/main.rs` fica com ~10 linhas.

---

## Verificação end-to-end (após Fase 2)

1. `yarn install` na raiz instala todos workspaces sem erro.
2. `yarn tauri dev` (no apps/desktop) sobe app sem warnings.
3. `yarn build:contracts` (script Turbo) gera `packages/contracts/src/generated/bindings.ts`.
4. O app abre direto no shell novo e mostra a lista de workspaces vinda de TanStack Query consumindo `@dcc/contracts`.
5. Criar workspace via UI → `dcc-tauri::workspace_commands::prepare_workspace` → `dcc-core::application::create_workspace_for_repo` → `dcc-infra::git` cria worktree → `EventBus` emite → Tauri `emit("workspace-changed")` → React Query invalida → UI atualiza sem reload.
6. `yarn test:frontend` executa `features/workspaces/*.test.tsx` verde.
7. `cargo test --workspace` executa testes de cada crate verde, incluindo testes de `dcc-core` que mockam ports (validando isolamento hexagonal).
8. Toggle dark/light → tokens OkLch aplicam corretamente.
9. Cmd+K abre command palette com ações da feature.
10. Resize do sidebar persiste em localStorage entre reloads.
11. CI executa `cargo clippy --workspace -- -D warnings` + `biome check` + testes — todos verdes.

---

## Riscos e mitigações

| Risco | Mitigação |
|---|---|
| Migração trava por escopo | Cada fase tem aceite claro e pode parar sem quebrar o legado. |
| `specta`/`tauri-specta` não cobre algum tipo | Validar cedo na Fase 0 e manter fallback de contracts manual se preciso. |
| `dcc-core` cresce demais | Extrair só quando doer, sem criar crates por hipótese. |
| Drift entre frontend e Rust | Contracts gerados continuam sendo a fonte única de verdade. |

---

## Decisões já confirmadas

1. **Coexistência**: `apps/desktop/` (novo) vive lado-a-lado com `legacy/` (renomeado de `src/` atual), mas o boot principal já aponta para o shell novo.
2. **State server**: TanStack Query 5 como padrão. Zustand modular **por domínio** apenas para UI ephemera.
3. **Editor rich**: adiar para depois da Fase 3.
4. **Package manager**: yarn (com workspaces).
5. **Providers iniciais**: Claude Code, Codex, Gemini estáveis; Cursor experimental atrás de flag.
6. **Event sourcing**: apenas `sessions` (turns/checkpoints/replay).
7. **Contracts**: gerados via `tauri-specta`, descartando `desktop-bridge.ts`.

## Decisões a verificar durante a Fase 0 (não bloqueiam)

- Lint atual do DCC (Biome ou ESLint?). Se ESLint, manter; se ausente, adicionar Biome.
- `git2` crate vs subprocess `git` — definir na Fase 0 com base em performance medida.
- Persistência de cache TanStack (`PersistQueryClientProvider`): habilitar a partir da Fase 2.
- Daemon (`dccd`/`dccd-http`) atual: integrar como sidecar Tauri em `dcc-tauri::daemon` ou refatorar para módulo dentro de `dcc-infra::process`. Decisão depende de quanto do daemon é "processo separado obrigatório" vs "task tokio".
