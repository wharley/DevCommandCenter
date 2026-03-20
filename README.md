# Dev Command Center

Multi-engine command center for coding agents. A local-first desktop app that orchestrates multiple providers (CLI and API) with BYOK, structured missions, and diff-driven reviews.

For the full product document, see `SKILL.md`. For product positioning and landing-page copy (non-technical), see [docs/POSICIONAMENTO_E_LANDING.md](docs/POSICIONAMENTO_E_LANDING.md).

## Why it exists

Dev Command Center focuses on a clear agent workflow: select a repo, describe a mission, generate a plan, review changes, and apply safely. It aims to stay local-first and provider-agnostic while keeping execution transparent.

## Features (developer-focused)

- **Hive workspace (default):** project picker, missions as Git worktrees, multiple terminal/agent panes per mission, mission-level review
- Multi-provider adapters (CLI and API) with validation and fallbacks
- Legacy mission workflow: plan generation, code generation, diff review, apply changes (Projects route)
- Git context collection (branch, status, recent commits)
- Local SQLite persistence for projects, providers, missions, combs, and panes
- Electron IPC bridge with a mock fallback for browser/dev

## Tech stack (condensed)

- Electron + Vite + React + TypeScript
- SQLite (better-sqlite3)
- Zustand, Radix UI, Tailwind CSS

## Architecture at a glance

- UI (React) → Zustand → IPC (preload) → Electron main process
- Electron main process → services (AI orchestrator, Git service) → SQLite
- Adapters encapsulate provider-specific behavior

## Getting started

### Prerequisites

- Node.js 22+
- Yarn
- Git

### Install

```bash
yarn install
```

### Development (Electron)

```bash
yarn electron:dev
```

### Development (web mock)

```bash
yarn dev
```

### Build

```bash
yarn electron:build
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

### Legacy: Projects and mission pipeline

The **Projects** route (`/projects`) and project sub-routes still expose the earlier **mission** workflow (pipeline, agents, review per project). The numbered steps in **Usage (mission pipeline)** (section below) describe that path.

## Usage (mission pipeline)

Classic flow when you work from **Projects** and the structured mission UI (plan → code → diff → apply):

1. Add a provider (API key or CLI path)
2. Add a project (local repo)
3. Create a mission (prompt)
4. Generate a plan and review steps
5. Generate code and review diffs
6. Apply changes (with backup) and commit

For the rationale behind each step and best practices (small missions, review before code, one mission per project), see [docs/CONCEITOS_E_USO.md](docs/CONCEITOS_E_USO.md).

## Roadmap (proposal)

- Hive workspace polish (onboarding, empty states, optional migration from legacy missions)
- Advanced diff review UX (selective apply, per-file previews)
- Automation presets (apply + tests + commit with confirmation)
- Provider profiles per project and mission templates

## Contributing

Issues and PRs are welcome. Please keep changes focused and aligned with `SKILL.md`.

## License

MIT
