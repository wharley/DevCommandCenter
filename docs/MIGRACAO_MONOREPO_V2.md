# DCC v2 — Refatoração para Monorepo (Tauri + React + Rust em camadas)

## Context

O DevCommandCenter (DCC) tem stack moderna (Tauri 2.10 + React 19 + Vite 6 + Radix + Tailwind 4) mas a organização do código mostra dívida arquitetural significativa: `src/pages/CmuxWorkspacePage.tsx` com **~3.500 linhas / 145 KB**, `src-tauri/src/main.rs` com **~8.600 linhas**, pasta `components/` (78 arquivos) misturando UI primitiva, dialogs e features sem separação clara, ausência de testes, e bridge frontend↔Rust sem tipo-segurança gerada (`src/lib/desktop-bridge.ts` com 19 KB).

A meta é **migrar o DCC para uma arquitetura em monorepo em camadas**, com:

- **Frontend (`apps/desktop`)** seguindo a UX/shell do **Helmor** (Tauri + React + TanStack Query + feature folders).
- **Domain model + contracts** seguindo o modelo mental do **t3code** (boundaries claros, schemas tipados, lógica fora da UI), **sem importar a parte Electron**.
- **Core operacional em Rust em camadas** (`dcc-core` em hexagonal, `dcc-infra` como adapters, `dcc-providers` como capabilities), com Tauri como bridge fina (`dcc-tauri`).
- **Providers de agente como capabilities trocáveis**: Claude Code, Codex, Gemini (estáveis) e Cursor (experimental).

> **Tese arquitetural**: T3 Code é referência de **organização de sistema** (monorepo, contracts, event model, separação UI/lógica). Helmor é referência de **forma de produto desktop** (shell Tauri, UX de workbench, feature folders). Rust é o **motor operacional** (processos, git, sessions, providers). Tauri é apenas a **borda** (commands + events). Nada do Electron de t3code (preload IPC, electron-updater, lifecycle main-process) é portado.

## Ordem de referência para UX/UI

**Meta declarada**: o resultado no DCC deve ser **igual ou superior** ao melhor dos referenciais (**Helmor** + **t3code**) nas dimensões que cada um cobre melhor — **sem “aproximação por conveniência”** e sem inventar padrões que não existem em nenhum dos dois (a menos que sejam upgrades óbvios e documentados aqui).

Antes de alterar qualquer tela do DCC, seguir esta ordem:

1. Ler a tela real do Helmor em `../helmor-main/src/App.tsx`, `../helmor-main/src/features/conversation/`, `../helmor-main/src/features/panel/` e `../helmor-main/src/features/inspector/` (`InspectorTabsSection`: Setup / Run / **terminais** como sub‑abas dentro do inspector, não “Context vs Terminal” genérico).
2. Ler o t3code em `../t3code-main/apps/web/src/components/`: `AppSidebarLayout.tsx`, `Sidebar.tsx`, `ChatView.tsx`, **`ChatHeader.tsx`** (toggle do terminal no header), **`ThreadTerminalDrawer.tsx`** (gaveta inferior no fluxo da conversa), `ChatComposer.tsx`, `MessagesTimeline.tsx`.
3. Usar o DCC atual apenas como **ponto de integração técnica** (Tauri/Rust/contracts). Produto/visual: **combinar explicitamente**: shell workbench central + sidebar (Helmor) **e** lista/chat header + terminal em **drawer** na coluna principal (t3).
4. Não tratar a UI legada interna do DCC como referência de produto; só não regredir integrações que já funcionam.

### Onde vai o terminal (não negociável na intenção de produto)

| Referência | Onde o terminal mora na UI |
|------------|----------------------------|
| **Helmor** | Dentro da **calha inspector** direita, na faixa unificada de abas (Run / terminals como instâncias), com resize/hover comportamento próprio (`InspectorTabsSection`). |
| **t3code** | **Gaveta inferior** na área principal do chat, acionada pelo **toggle no header** (`ChatHeader` + `PersistentThreadTerminalDrawer` / `ThreadTerminalDrawer`). **Não** é um par “Context \| Terminal” no painel lateral. |

**No DCC (decisão alinhada ao usuário)**:

- Inspector direito = **contexto contínuo** (providers, estado de sessão, metadados) — modelo **inspector Helmor**.
- Terminal = **drawer inferior na coluna do workbench** + **toggle no header** do runtime — modelo **t3**.  
- É proibido reintroduzir abas artificiais tipo “Context / Terminal” só no inspector **como substituto** do drawer do t3; isso não bate com nenhum dos dois produtos citados pelo time.

### Sidebar esquerda: Helmor vs t3code — análise de produto (PM + mercado dev)

Não existe resposta “Helmor ou t3” no absoluto: são **dois modelos mentais** sobre **o que a lista da esquerda representa**.

| Dimensão | **Helmor** (`features/navigation/`, `WorkspacesSidebar`) | **t3code** (`Sidebar.tsx`, threads sob projetos) |
|----------|--------------------------------------------------------|--------------------------------------------------|
| **Unidade primária** | **Workspace** como “cartão de trabalho” com **estado de fluxo** (ex.: feito, em envio, precisa de interação, arquivado). | **Projeto (cwd/repo)** → **threads** (conversas/linhas de execução). |
| **Organização** | **Grupos por estágio do pipeline** (kanban leve na vertical): separa o que já fechou do que está ativo do que exige input humano. | **Agrupamento por repositório / path / “separate”**, ordenação por data, DnD, multi‑ambiente; muitos **tokens de estado na linha** (PR, terminal, erro, unread). |
| **Força principal** | **Clareza de workflow** quando “entrega” é um workspace: revisar algo “Done” enquanto outro está “Sending” fica **visível na própria estrutura**, sem depender só de filtros mentais do usuário. | **Throughput diário**: pular entre conversas dentro do mesmo repo, abrir nova thread, atalhos e densidade próximos de **IDE + backlog de chats**. |
| **Mercado típico** | Equipes/fluxos onde **pull request**, revisão e “estado oficial” do trabalho importam tanto quanto o chat. Equipes mais **product-led** sobre “fatias de trabalho” nomeadas. | **Builders** que vivem em **múltiplas sessões paralelas** (experimentos, hotfix, refactor) sob os mesmos diretórios; mentalidade **orientada a histórico de conversação**, menos a “pastas por fase”. |
| **Limite típico** | Se só existir “lista por fase”, perde‑se granularidade quando o utilizador trabalha sobretudo por **sessão/agent run** dentro do mesmo workspace; exige modelo de dados rico nos workspaces. | Se não houver estado de ciclo forte, trabalhos “para rever” podem **enterrar‑se cronologicamente**; depende mais de pinning, filtros ou pills na linha. |

**Interpretação pragmática (o que a literatura e tooling dev costumam reforçar):**

- Um dev **individual** (“meu próprio trabalho técnico, múltiplas tasks em paralelo”) tende a alinhar com **lista densa tipo t3**: velocidade para localizar uma thread/repo e continuar onde parou — alinhado com o uso intenso de **Copilot Chat, Cursor chats, Warp worktrees**.
- Fluxos onde **humanos diferentes** integram trabalho ao longo do tempo (**review, QA, stakeholder**) beneficiam‑se mais de **buckets tipo Helmor**, porque comunicam progresso por **bucket** só de olhar a sidebar (“isto está em espera de mim”).
- **Rever enquanto outro corre** não é propriedade exclusiva do Helmor: **também existe no t3** via **várias threads** (uma em review, outra a correr agente); a diferença é **semiologia**. No Helmor o paralelismo aparece mais como **estados diferentes no mesmo objeto (workspace)**; no t3, mais como **objetos (threads)** lado a lado.

**Decisão para o documento — paridade conscientemente escolhida (baseline sugerido para o DCC):**

1. **Default de navegação e “feel” lista:** seguir mais de perto o **t3** — **lista escaneável, agrupável por projeto/repo**, forte suporte a **múltiplas sessões/threads por contexto**. Isto casa com **rotina típica de dev** mencionada pelo utilizador‑product owner.
2. **Semântica pipeline Helmor onde o domínio DCC já tiver equivalência:** manter ou **portar badges / secções opcionais** para estados como *pronto para review*, *aguardando input*, *arquivado*, **sem obrigar** a sidebar inteira a ser só “pipeline por workspaces” como no Helmor, se isso regressar velocidade típica t3.

Isto equivale a dizer na doc: **“Paridade UX com os referenciais não é copiar apenas um lado; para a rail esquerda, o alvo declarado é o modelo **t3** como espinha, com **semiologia/chips de ciclo tipo Helmor** onde fizer sentido no modelo `workspace/session`.”** Alterações ao baseline exigem **decisão explícita aqui**, não apenas preferência verbal em PR.

Implementação de referência a releer antes de código fechado: `helmor-main/src/features/navigation/index.tsx`, `helmor-main/src/features/navigation/shared.tsx` (grupos e tons “Done” etc.), `t3code-main/apps/web/src/components/Sidebar.tsx` e `Sidebar.logic.tsx`.

### Rail esquerdo no DCC — portar o shell desktop de referência sem trade-offs herdados

**Status — concluído.** A superfície de navegação esquerda do shell em `apps/desktop` cumpre o escopo abaixo no que diz respeito a hierarquia escaneável, densidade da lista (virtualização), agrupamento por projeto, secção archived, resize com hit-area, persistência da largura do painel e do colapso total da rail, secções expansíveis com estado em `localStorage`, CTAs (incluindo criar workspace) e integração visual com tokens do tema. Pontos de implementação: `apps/desktop/src/features/workspaces/sidebar.tsx` (`WorkspacesSidebar`), `workspace-rail-projection.ts`, `workspace-rail-row.tsx`, `workspace-rail-shared.tsx`, `workspace-rail-open-state.ts`, `use-workspaces.ts`; geometria/persistência de painéis em `apps/desktop/src/shell/{layout,use-panels}.ts` (por exemplo `SIDEBAR_WIDTH_STORAGE_KEY`, `dcc.workbenchRail.sectionOpenState`). **Semiologia opcional de ciclo** (badges tipo review / aguardando input conforme baseline “espinha t3 + ciclo opcional”) permanece backlog de produto, não bloqueante desta entrega.

**Escopo original (mantido como especificação de regressão).** O objetivo era o **rail esquerdo** do workbench desktop de referência **funcionar igual de bem como superfície de navegação** no DCC: hierarquia, densidade da lista, colapso/expansão, resize, secções, CTAs e paleta quando integrada no mesmo bloco. Para isso pode-se **copiar de forma literal** classes **Tailwind CSS**, uso de **ícones**, **botões**/variantes, **layouts** e composição JSX do referencial — o que não se copia são **estruturas de produto ruins** já listadas mais abaixo como trade-offs vetados.

**Única restrição dura em código.** **Nenhum símbolo, ficheiro, pasta, classe CSS “semântica” própria, chave de storage/query ou string técnica** pode conter o fragmento **`Helmor`**/`helmor`. Ao colar código, renomeie componentes, hooks e variáveis para prefixos/variações **`dcc`** / vocabulário de domínio (`Workspace`, `Thread`, …) e `@dcc/contracts`. Isto não impede reusar markup e utilitários Tailwind iguais ao original.

**Labels e copy de UI (sim).** Textos **user-facing**: títulos de secção, placeholders, **`<label>`**, **`aria-label`**, tooltips e CTAs devem existir com a **mesma clareza** do referencial; o idioma/copy segue **produto DCC** (workspace, projeto, sessão conforme decidido aqui). Não há obrigação de “traduzir tudo só por ser portado”: o ponto é **boa semântica e acessibilidade**, não proibir inglês onde o app já usa inglês técnico comum (“Toggle sidebar”, etc.), desde que **não apareça marca ou nome do repositório externo** onde o utilizador leia marca errada.

**O que é permitido portar (comportamento e forma).**

- **Visual e interação**, incluindo **Tailwind, ícones, botões, layout** espelhados do referencial.  
- Geometria e persistência do rail (largura mínima/máxima, hit area de resize, `localStorage`, colapso).  
- Ritmo visual: cabeçalho fixo, lista rolável, blocos de secção, CTA “criar workspace”, vazios/skeletons.  
- Agrupamentos de lista **desde que** respeitem o baseline da tabela “Sidebar esquerda” acima (espinha t3 + ciclo opcional).

**Trade-offs do referencial que o DCC não deve herdar (explícito).**

- **Não** substituir o baseline “espinha t3 + chips de ciclo opcionais” por um rail **só** pipeline se isso **reduzir** “encontrar thread/repo rápido” — ciclo como **opcional**.  
- **Não** reabrir terminal no inspector: segue **Onde vai o terminal** (drawer + header).  
- **Não** importar acoplamentos de dados (stores globais, shape de cache) que violem boundaries deste doc (Query modular, invalidação por eventos, contracts gerados).

**Checklist de aceite (baseline)** — aplicável a qualquer refactor grande da rail; já verificado para a entrega atual.

1. Paridade visual/interação forte: **Tailwind + ícones + botões + layout** comparáveis ao trecho copiado do referencial.  
2. **Zero** substring `helmor`/`Helmor` em nomes de código, pastas ou chaves técnicas.  
3. **Labels**/ARIA/copy presentes e coerentes com o produto DCC.  
4. Terminal + inspector dentro do contrato de surfaces já documentado.  
5. Resize/colapso e teclado no mesmo nível ou melhor que o referencial.

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
- Concluído: `legacy/` criado a partir de `src/`, preservado como base histórica enquanto o shell novo vira o caminho principal (período de coexistência já encerrado).
- Concluído: pasta `legacy/` na raiz **removida em 2026-05-03** — não era mais referenciada pelo Vite (`@` → `apps/desktop/src`), nem pelo `index.html` (entrada `apps/desktop/src/main.tsx`); apenas snapshot histórico.
- Concluído: novo shell inicial em `apps/desktop` com query client, tema, layout base e entrada Vite separada.
- Concluído: boot principal agora aponta para o shell novo, sem alternância por flag.
- Concluído: `yarn install`, `yarn vite:build`, `cargo check -p dcc-core` e `cargo check -p dcc-tauri` passam no workspace atual.
- Concluído: `yarn build:contracts` passa no pacote `@dcc/contracts`.
- Concluído: Fase 1 shell + UI primitiva, com base visual, primitives e feature folders já portados.
- Em andamento: Fase 2 já tem o fluxo de criar workspace ligado do shell ao Rust e o contrato agora é gerado por `build.rs` via `tauri-specta`.
- Fora do escopo de 0b: providers e adapters Rust de verdade, que entram nas Fases 2 e 3.
- Concluído: Fase 3 fechada com `dcc-core::domain::session`, event log, provider runtime bridge, stream de eventos e cockpit de sessão no shell.
- Concluído: UX pass 1 do shell/runtime para aproximar ainda mais o comportamento visual do Helmor sem mexer no core.
- Concluído: UX pass 2 do shell/runtime com thread temporal, auto-scroll e affordance de latest event no estilo Helmor.
- Concluído: UX pass 3 do shell/runtime com provider strip, footer de composer e CTA primária de envio mais evidente.
- Concluído: UX pass 4 do shell/runtime com cabeçalho mais compacto e contexto de provider reduzido ao essencial.
- Concluído: visual polish final do shell/runtime com topbar contextual, rail de providers mais informativo e hierarquia visual mais clara.
- Concluído: shell principal sem `Overview`/`Runtime` tabbed shell, agora entrando direto no runtime workbench para ficar mais próximo do Helmor.
- Concluído: **rail esquerdo** no shell novo — navegação agrupada por projeto, lista virtualizada, archived, resize/largura e colapso persistidos (`useShellPanels` + estado de secções da rail).
- Em andamento: resto da **Fase 4** (workbench central, inspector à direita, demais features da lista da fase); o terminal já está integrado ao runtime com xterm, fit suspension, ações de foco/limpeza e bridge PTY Tauri conectada, mantendo 0a concluída em compile, 0b fechada no boot principal, Fase 1 fechada e Fase 2 validada no core.

**Objetivo das Fases 1-3**

Trazer **paridade ou melhoria real** relativamente aos referenciais: shell/UX/colocação de surfaces (Helmor + t3) e contratos/boundaries/eventos (t3), com Tauri + Rust como motor e fronteira do sistema. “Parecer parecido” sem cobrir os mesmos affordances conta como **não atendido**.

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
- Concluído: `start_thread`, `send_turn`, `abort_run` e `resume_session` expostos no Tauri bridge com estado de sessão ativo no shell.
- Concluído: o runtime shell ganhou um workbench de sessão/composer mais próximo do Helmor, com thread timeline, provider selection e resumo de projection.
- Concluído: protocolos runtime específicos podem evoluir depois, mas a Fase 3 já está fechada com o bridge CLI genérico e o fluxo funcional ponta a ponta.

**Fase 4 — Features restantes (iterativo)**
1. Implementar na ordem: `terminal` → `review` → `inspector` → `composer` → `navigation` → `settings` → `shortcuts` → `onboarding` → `updater`.
2. Manter cada feature pequena, com container + view + tests.
3. Só extrair mais crate/package se houver consumidor real ou dor concreta.

**Status da Fase 4**

- Concluído: **rail esquerdo** conforme secção “Rail esquerdo no DCC” deste doc (implementação sob `apps/desktop/src/features/workspaces/`, especialmente `sidebar.tsx` e `workspace-rail-*`).
- Em andamento: `terminal` usa **PTY por `workspaceId`**, xterm, fit suspension, bridge Tauri; **surface primária = drawer inferior no workbench** (padrão t3 `ThreadTerminalDrawer`) com toggle no header do runtime — **não** painel lateral “Context \| Terminal”.
- Inspirado na **calha de terminais do Helmor** para persistência/multi‑instância no futuro; hoje DCC garante reuso do PTY ao fechar/abrir o drawer.

**Fase 5 — Sunset legacy + fechamento**
1. Remover `legacy/` quando a paridade funcional estiver fechada — **pasta removida em 2026-05-03** (critério aplicado: snapshot na raiz já órfão do Vite/`index.html`; ver status abaixo).
2. Reavaliar extrações só se `dcc-infra` ou `sessions` ficarem grandes demais.
3. Tratar editor rich como etapa posterior, não como pré-requisito.

**Status da Fase 5**

- Concluído (2026-05-03): remoção da pasta `legacy/` na raiz do repositório, após validação de que nenhum script de build, Tauri ou código em `apps/` / `packages/` a importava (o shell ativo já estava só em `apps/desktop/`).
- Em aberto: itens 2 e 3 acima (extrações por tamanho e editor rich).

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

1. **Shell único**: o boot e o bundle usam apenas `apps/desktop/`. Durante a migração, `legacy/` (antigo `src/` renomeado) coexistiu na raiz; em **2026-05-03** essa pasta foi removida por não fazer parte do caminho de build.
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
