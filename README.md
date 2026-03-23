# Dev Command Center

Multi-engine command center for coding agents. A local-first desktop app that orchestrates multiple providers (CLI and API) with BYOK, structured missions, and diff-driven reviews.

For the full product document, see `SKILL.md`. For product positioning and landing-page copy (non-technical), see [docs/POSICIONAMENTO_E_LANDING.md](docs/POSICIONAMENTO_E_LANDING.md).

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

## Primary user flow (Hive workspace)

The app opens on `/` into the **Hive workspace**: sidebar for the active repository and **missions** (each mission is one Git **worktree** / feature branch), and a main area for terminals, agents, and review.

1. **Select a project (Hive)** — Use the project selector at the top of the sidebar; **Add project** registers a local repo path.
2. **Create or select a mission** — Missions list the worktrees for that repo. **+** creates a new mission (name, optional description, base branch). Only one mission is **active** in the UI at a time.
3. **Work in Panes** — On the **Panes** tab, add **Terminal** and **Agent** panes. Every pane uses the **same working directory**: the active mission’s worktree. Multiple CLI agents and shells per mission are supported.
4. **Review** — On the **Review** tab, inspect diffs and run Git actions (commit, push, merge, etc.) at the **mission / worktree** level—not tied to a single pane session.
5. **Settings** — **Settings** in the sidebar opens provider and app preferences **in the main panel** (same full-screen workspace), without leaving this layout.

**In one sentence:** Pick a repo → open a mission (worktree) → run as many terminals/agents as you need in that folder → integrate changes from **Review**.

The app route is **`/`** only (unknown paths redirect to Hive). Older per-project and mission-detail UIs were removed; historical **mission** rows may still exist in the local DB from past versions but are not exposed in the UI.

## Roadmap (proposal)

- Hive workspace polish (onboarding, empty states)
- Advanced diff review UX (selective apply, per-file previews)
- Automation presets (apply + tests + commit with confirmation)
- Provider profiles per project and mission templates

## Contributing

Issues and PRs are welcome. Please keep changes focused and aligned with `SKILL.md`.

## License

MIT
