# Dev Command Center

Multi-engine command center for coding agents. A local-first desktop app that orchestrates multiple providers (CLI and API) with BYOK, workspace-first sessions, and terminal-native flow.

For product and architecture notes, see the `docs/` folder. For positioning and landing-page copy (non-technical), see [docs/POSICIONAMENTO_E_LANDING.md](docs/POSICIONAMENTO_E_LANDING.md).

**Desktop stack:** o app migrou de Electron para **Tauri 2** — motivos, diferenças e **lista de instalação** (Node, Rust, dependências por SO): **[docs/MIGRACAO_TAURI.md](docs/MIGRACAO_TAURI.md)**.

## Why it exists

Dev Command Center focuses on a clear agent workflow: select a repo, describe a mission, generate a plan, review changes, and apply safely. It aims to stay local-first and provider-agnostic while keeping execution transparent.

## Features (developer-focused)

- **Hive workspace (single app shell):** project picker, missions as Git worktrees, multiple terminal/agent panes per mission, mission-level review
- Multi-provider adapters (CLI and API) with validation and fallbacks
- Git context collection (branch, status, recent commits)
- Local SQLite persistence for projects, providers, missions, combs, and panes
- Tauri bridge (`window.desktopAPI` / `window.db`) with mock fallback no browser

## Tech stack (condensed)

- Tauri 2 + Vite + React + TypeScript
- SQLite via Rust (rusqlite no backend)
- Zustand, Radix UI, Tailwind CSS

## Architecture at a glance

- UI (React) → Zustand → Tauri `invoke` / eventos → Rust (`src-tauri`)
- Processo Rust → SQLite, Git, terminal, serviços de IA
- Adapters encapsulate provider-specific behavior

## Getting started

### Prerequisites (resumo)

| Ferramenta | Notas |
|------------|--------|
| **Node.js** | 22+ |
| **Yarn** | v1 classic (este repo) |
| **Git** | Para worktrees / fluxo do app |
| **Rust (stable)** | Obrigatório para `yarn dev` e `yarn build` — compila `src-tauri` |
| **Deps de sistema** | macOS: Xcode CLT; Linux: WebKit/GTK dev libs; Windows: MSVC + WebView2 — **detalhe completo em [docs/MIGRACAO_TAURI.md](docs/MIGRACAO_TAURI.md)** |

### Install

```bash
yarn install
# se necessário no teu ambiente:
# yarn install --ignore-engines
```

### Development (app desktop Tauri + Vite)

```bash
yarn dev
```

### Development (só Vite no browser — APIs nativas indisponíveis)

```bash
yarn vite
```

### Build (binários desktop)

```bash
yarn build
```

### Lint

```bash
yarn lint
```

## Primary user flow (workspace-first)

The app opens on `/` into the main workspace shell: sidebar for **workspaces** and a main area for terminal/agent panes.

1. **Add project** — Register at least one local repository path. This is required because workspaces attach to a project/repo context.
2. **Create or select a workspace** — A workspace maps to a dedicated worktree/branch context.
3. **Work in panes** — Add **Terminal** and **Agent** panes. Panes share the active workspace directory.
4. **Watch notifications** — Attention events show toast + sidebar indicators and can be opened in the notifications panel.
5. **Manage providers** — Configure CLI/API providers in the Providers screen, then use them when opening new agent panes.

**In one sentence:** Add a repo → open a workspace → run terminals and agents in that directory → react to attention notifications quickly.

The app route is **`/`** only (unknown paths redirect to the main workspace shell). Older mission/review/dashboard flows are no longer part of the primary UI.

## Roadmap (proposal)

- Hive workspace polish (onboarding, empty states)
- Advanced diff review UX (selective apply, per-file previews)
- Automation presets (apply + tests + commit with confirmation)
- Provider profiles per project and mission templates

## Contributing

Issues and PRs are welcome. Please keep changes focused and aligned with the project direction documented in `docs/`.

## License

MIT
