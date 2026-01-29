# Dev Command Center

Multi-engine command center for coding agents. A local-first desktop app that orchestrates multiple providers (CLI and API) with BYOK, structured missions, and diff-driven reviews.

For the full product document, see `SKILL.md`.

## Why it exists

Dev Command Center focuses on a clear agent workflow: select a repo, describe a mission, generate a plan, review changes, and apply safely. It aims to stay local-first and provider-agnostic while keeping execution transparent.

## Features (developer-focused)

- Multi-provider adapters (CLI and API) with validation and fallbacks
- Mission workflow: plan generation, code generation, diff review, apply changes
- Git context collection (branch, status, recent commits)
- Local SQLite persistence for projects, providers, and missions
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

## Usage (core workflow)

1. Add a provider (API key or CLI path)
2. Add a project (local repo)
3. Create a mission (prompt)
4. Generate a plan and review steps
5. Generate code and review diffs
6. Apply changes (with backup) and commit

## Roadmap (proposal)

- Worktrees as first-class workflow (create/list/open per mission)
- Advanced diff review UX (selective apply, per-file previews)
- Automation presets (apply + tests + commit with confirmation)
- Provider profiles per project and mission templates

## Contributing

Issues and PRs are welcome. Please keep changes focused and aligned with `SKILL.md`.

## License

MIT
