# Dev Command Center

Dev Command Center (DCC) is a local-first desktop workbench for software engineering with AI agents. It combines isolated Git worktrees, terminal execution, session orchestration, provider integrations, and local persistence in a single Tauri application.

![Dev Command Center screenshot](docs/assets/app-screenshot-new.png)

## What you can do in DCC

- Create isolated workspaces and git worktrees for parallel tasks without juggling `git stash`.
- Run agent workflows across providers such as Claude, Gemini, Codex, and Cursor from the same desktop surface.
- Keep local session history, replay prior activity, and preserve workspace-specific runtime context.
- Open embedded project terminals with tabs for repo-level work that should stay inside the app.
- Open the active workspace in a preferred editor such as Cursor, Zed, or VS Code.
- Inspect workspace changes, diffs, and review surfaces without leaving the DCC workbench.
- Drive plan mode, mission specs, and follow-up implementation flows from the same session.
- Manage project skills from a provider-neutral source and compile them into agent-native targets such as `.claude/skills/`, `AGENTS.md`, `GEMINI.md`, and `.cursor/rules/`.
- Use optional mobile pairing and local HTTP access for companion workflows on the same trusted network.

## Product shape

- Workspace-first: the main unit is an isolated task workspace tied to a repository and branch context.
- Local-first: state, sessions, and runtime surfaces stay on your machine.
- Agent-aware: DCC is not just a terminal wrapper; it keeps plans, specs, diffs, session events, and provider context connected inside one workbench.

## Stack

- Tauri 2 + Rust
- React 19 + TypeScript + Vite
- SQLite for local persistence
- xterm.js for terminal surfaces

## Requirements

- Node.js 22 recommended
- Yarn v1
- Rust stable
- Git

## Development

Recommended setup:

```bash
./setup.sh
```

Manual setup:

```bash
yarn install
yarn dev
```

Desktop-only frontend shell:

```bash
yarn dev:desktop
```

## Worktrees and `.env`

Environment files are ignored by Git. For a new clone or worktree:

```bash
yarn setup-worktree
```

If no shared `.env` is found, the setup script falls back to `.env.example`.

## Repository notes

- The project is open source under Apache-2.0.
- Signed release distribution is currently focused on macOS and Linux.
- Licensed under Apache-2.0. See [LICENSE](LICENSE).

## Acknowledgments

DCC was shaped by the broader ecosystem of AI coding tools, terminal-native developer workflows, local-first apps, and worktree-based development practices.

## Downloads

- Releases page: <https://github.com/wharley/DevCommandCenter/releases>
- Signed builds are published for macOS and Linux through GitHub Releases.
- macOS public release artifacts are currently published as signed app bundle archives (`.app.tar.gz`) with updater metadata.
- Linux public release artifacts are currently distributed as Debian packages (`.deb`) in the public release pipeline.
- Public releases are signed for this repository. Forks that want their own downloadable builds should publish from their own repository with their own signing keys and release endpoint.

## CI and releases

- GitHub Actions provide manual validation workflows for Linux and macOS.
- Signed public releases are prepared through GitHub Releases via `.github/workflows/publish-release.yml`.
- Public release publication is intentionally limited to manual dispatch or version tags.
- Manual release dispatch can target `all`, `linux-x64`, `macos-arm64`, or `macos-intel`.
- The in-app updater is configured to read `latest.json` from GitHub Releases after the first signed release is published.
- Validation workflows and release publishing are intentionally separated so signing secrets stay isolated to the protected `release` environment.

## Available docs

- [Mobile pairing security model](docs/SECURITY_MOBILE_PAIRING.md)
- [Security policy](SECURITY.md)
- [Contributing guide](CONTRIBUTING.md)
- [Support](SUPPORT.md)
- [Release guide](docs/RELEASING.md)
- [GitHub release bootstrap](docs/GITHUB_RELEASE_BOOTSTRAP.md)
- [GitHub repository settings](docs/GITHUB_REPOSITORY_SETTINGS.md)
- [Open source release checklist](docs/OPEN_SOURCE_RELEASE_CHECKLIST.md)
