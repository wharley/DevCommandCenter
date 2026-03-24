# Dev Command Center Architecture

## Product Direction

Dev Command Center follows a workspace-first model inspired by cmux:

- Primary unit is `Workspace` (backed by `comb` + worktree context).
- Main UI is a single shell with:
  - Workspace list in sidebar
  - Pane grid in main area (`terminal` + `agent`)
  - Attention notifications (toast + badge + panel)
- Providers are a differentiated capability, managed in a focused Providers screen.

The app intentionally avoids dashboard/review/mission-heavy flow in the primary UX.

## Runtime Stack

- Frontend: React + TypeScript + Vite
- Desktop host: Tauri 2 (Rust backend)
- Persistence: SQLite via Rust commands exposed on `window.db`
- Desktop APIs and events: exposed on `window.desktopAPI`

## Core Data Model

- `Project`: local repository path and metadata.
- `Comb` (workspace): logical workspace, base branch, optional worktree path.
- `Pane`: terminal or agent session metadata linked to a workspace.
- `Provider`: CLI/API provider configuration used to launch agents.

## Main UI Flow

1. User adds/selects a `Project`.
2. User creates/selects a `Workspace`.
3. User opens panes:
   - `terminal` pane (interactive shell, workspace cwd)
   - `agent` pane (provider CLI command launched in shell context)
4. App listens for attention events and surfaces them in:
   - Toast notifications
   - Sidebar indicators
   - Notifications panel

## Terminal Session Lifecycle

- Terminal rendering uses xterm.js.
- PTY lifecycle is managed by Tauri commands/events.
- Pane sessions are keyed by `paneId` for reattach behavior.
- Spawn strategy uses interactive/login shell for native prompt and git branch context.

## Attention Notifications

- Events arrive from desktop runtime (`terminal-attention`).
- Renderer deduplicates noisy events.
- UI stores a short local history for unread/read navigation.
- Clicking a notification navigates to related workspace/pane.

## Performance Principles

- Minimize unnecessary rerenders in workspace list and pane grid.
- Use memoized maps/sets for attention lookups.
- Keep heavy UI sections split and memoized by item.
- Persist lightweight state locally and cap history size.

## Current Source Anchors

- App entry: `src/App.tsx`
- Main workspace shell: `src/pages/CmuxWorkspacePage.tsx`
- Providers screen: `src/pages/SettingsPage.tsx`
- Terminal component: `components/embedded-terminal.tsx`
- Attention hook: `hooks/use-terminal-attention-toasts.ts`
- Types: `lib/database/types.ts`, `lib/terminal/attention-types.ts`
