<p align="center">
  <a href="docs/BRAND.md">
    <img src="public/dcc-mark.svg" alt="Dev Command Center mark" width="112" height="112" />
  </a>
</p>

<h1 align="center">Dev Command Center</h1>

<p align="center">
  Workspace-first AI coding hub for managing agents, reviews, terminals, and task flows across multiple providers.
</p>

Dev Command Center (DCC) is a local-first desktop workbench for software
engineering with AI agents. It connects isolated Git worktrees, multi-provider
sessions, review and delivery workflows, terminals, usage insights, and local
persistence in one Tauri application.

> [!IMPORTANT]
> Before starting agent sessions, set up at least one provider on the computer
> running DCC. Claude, Codex, Cursor, Grok, Gemini and Droid require their own
> installed and authenticated CLIs. Install only the providers you want to use.
> See [Provider setup](#provider-setup), including Antigravity's separate setup.

![Dev Command Center running an agent task across multiple projects](docs/assets/dcc-workbench-running-task.png)

## Core capabilities

- **Workspace-first agent sessions**: run AI coding work inside isolated Git worktrees while keeping session history, runtime context, terminals, and reviews connected to the active workspace.
- **Last Turn Review and Guarded Undo**: inspect the exact result of a completed agent turn and, for eligible macOS workspaces, preview and safely restore the previous file contents. See [Last Turn Review and Guarded Undo](docs/GUARDED_UNDO.md).
- **Pull Request Center**: review GitHub pull requests and GitLab merge requests, inspect checks and discussions, publish review actions, create isolated implementation tasks, and directly merge eligible GitHub PRs with an explicitly confirmed strategy.
- **DCC Feedback**: report bugs and suggest improvements from the sidebar, review a public issue before publishing with your connected GitHub account, and follow your reports in “My feedback”. Drafts stay on your device; version and system details are optional. See [DCC Feedback](docs/DCC_FEEDBACK.md).
- **Delegation agents**: hand off review, explanation, or implementation tasks to child sessions, inspect their work in the Inspector, send feedback back to the child agent, and apply or discard the isolated worktree output. See [Delegation agents](docs/DELEGATION_AGENTS.md).
- **Git and delivery workflows**: inspect live changes, commit and push, create or update change requests, recover delivery failures, and resolve merge conflicts without leaving the workbench.
- **Managed MCP integrations**: connect trusted local or remote tools with scoped bindings, OS-backed credential storage, runtime status, and per-tool Ask/Allow/Deny policies.
- **Usage and skills**: compare real provider activity, inspect token and model usage, and manage project skills from a provider-neutral source.
- **Mobile companion pairing**: pair a phone with the desktop app through QR code + PIN and use companion workflows on the same trusted network or through Tailscale. See [Mobile web companion](docs/MOBILE_WEB.md).
- **Provider-neutral workflows**: use Claude, Gemini, Codex, Cursor, and other provider integrations from the same workbench surface.
- **Rich text composer**: format text, edit links, and create bullet or numbered lists while typing. Drafts preserve the structure and prompts are sent as Markdown. See [Composer](docs/COMPOSER.md).
- **Model favorites**: save provider, model, and effort combinations from the composer picker. Use **Edit favorites** to add, remove, change effort, or drag combinations into your preferred order (arrow controls are also available). Favorites are saved on this device across projects; changing the conversation's effort leaves the saved favorite unchanged. **All models** keeps the full catalog accessible.
- **Provider handoff**: select another provider inside an existing session and send the next turn normally; DCC automatically attaches one bounded, provider-neutral re-anchor. DCC persists the timeline, but the new runtime does not receive native 1:1 memory and a new thread starts fresh. See [Provider handoff](docs/PROVIDER_HANDOFF.md).
- **Appshots**: attach screenshots of open macOS app windows from the composer or a configurable global shortcut. Preview multiple windows and keep captures with the conversation draft. Requires macOS 14+. See [Appshots](docs/APPSHOTS.md).
- **macOS menu bar**: click the DCC icon to follow running tasks and pending replies/approvals, inspect CPU/memory usage, or open the quick composer. Closing the main window keeps DCC available; use Quit to exit. See [Menu bar](docs/MENU_BAR.md).
- **Quick composer**: press Command + Shift + C on macOS to start a task over another app, choosing a project, Local or Worktree, and a provider/model. Reuses composer attachments and preserves interrupted requests. See [Quick composer](docs/QUICK_COMPOSER.md).
- **Project notes**: capture ideas and conversation excerpts in floating, draggable balloons; prepare a task or composer draft when ready. Notes survive completed-task and worktree cleanup. See [Project notes](docs/PROJECT_NOTES.md).
- **Visual direction**: shared surfaces, motion and navigation patterns inspired by project notes. See [DCC visual direction](docs/DCC_VISUAL_DIRECTION.md).
- **Built-in review surface**: inspect changed files, inline diffs, annotations, branch status, CodeRabbit feedback, validations, and PR-ready state without leaving DCC.

## What you can do in DCC

- Create isolated workspaces and Git worktrees for parallel tasks without juggling `git stash`.
- See active tasks from every project in one Running section and track completed worktree storage before permanent deletion.
- Run agent workflows across providers such as Claude, Gemini, Codex, and Cursor from the same desktop surface.
- Keep local session history, replay prior activity, and preserve workspace-specific runtime context.
- Open embedded project terminals with tabs for repo-level work that should stay inside the app.
- Open the active workspace in a preferred editor such as Cursor, Zed, or VS Code.
- Review the current workspace or isolate the latest agent turn, with a guarded restore when its safety capture is eligible.
- Inspect pull requests, discussions, checks, approvals, conflicts, and merge readiness from a dedicated hub.
- Create readable semantic `dcc/`, `dcc/fix/`, and `dcc/feat/` branches from task titles while preserving existing user branches.
- Resolve Git conflicts with index-backed ours/theirs/result controls and optional agent assistance.
- Drive plan mode, mission specs, and follow-up implementation flows from the same session.
- Manage project skills from a provider-neutral source and compile them into agent-native targets such as `.claude/skills/`, `AGENTS.md`, `GEMINI.md`, and `.cursor/rules/`.
- Connect DCC-managed MCP servers to compatible providers without editing provider-owned configuration.
- Compare provider, model, token, cache, reasoning, and cost activity recorded by DCC.
- Use optional mobile pairing and local HTTP access for companion workflows on the same trusted network.

## Product shape

- Workspace-first: the main unit is an isolated task workspace tied to a repository and branch context.
- Local-first: state, sessions, and runtime surfaces stay on your machine.
- Human-controlled: agents can implement and prepare delivery, while destructive actions, permissions, reviews, merges, and restores stay explicit.
- Agent-aware: DCC is not just a terminal wrapper; it keeps plans, specs, diffs, session events, permissions, usage, and provider context connected inside one workbench.

## Stack

- Tauri 2 + Rust
- React 19 + TypeScript + OJ
- SQLite for local persistence
- xterm.js for terminal surfaces

## Project memory

DCC integrates [ai-memory](https://github.com/akitaonrails/ai-memory), an open-source
long-term memory and handoff server for AI coding agents. DCC uses it as an optional,
replaceable sidecar; the DCC session database remains the source of truth.

The integration is maintained in DCC's own adapter and user interface. It adds
provider-neutral session export, scoped retrieval, a durable retry outbox, export
history, and recovered-source curation. DCC is not a fork of ai-memory and does not
replace its upstream protocol or wiki storage.

For automatic memory, the configured Workspace is shared intentionally, while the configured
Project is used as a prefix and combined with the DCC's stable project id. Sessions and agents
working in the same repository therefore share memory; different repositories remain isolated.

In distributed builds, DCC currently pins the upstream `v2.2.2` release and bundles
the matching sidecar binary. Development builds never download it automatically. Users
can run the managed local sidecar, configure an existing remote server, or disable
memory in **Settings → Memory**. See the [ai-memory feasibility and integration notes](docs/AI_MEMORY_FEASIBILITY.md)
for scope, performance measurements, limitations, and the exact upstream references.

ai-memory is distributed under its own upstream terms; consult its [license](https://github.com/akitaonrails/ai-memory/blob/main/LICENSE)
when redistributing or replacing the sidecar. DCC's own source remains licensed under
Apache-2.0.

## Requirements

### Provider setup

Install and authenticate the CLI for each provider you intend to use, on the
computer running DCC and under the same OS user account. DCC uses those existing
installations; it does not bundle, install or update these provider CLIs.

| Provider | Required installation | Authentication/setup |
| --- | --- | --- |
| Claude | [Claude Code CLI](https://code.claude.com/docs/en/setup), command `claude` | Run `claude auth login`. |
| Codex | [Codex CLI](https://developers.openai.com/codex/cli/), command `codex` | Run `codex login`. |
| Cursor | [Cursor CLI](https://cursor.com/docs/cli/installation), command `cursor-agent` in DCC | Run `cursor-agent login`. DCC currently requires this command to be available, even where upstream docs use the name `agent`. |
| Grok Build | Grok Build CLI, command `grok` | Run `grok login` or configure its supported API-key authentication. |
| Gemini | [Gemini CLI](https://geminicli.com/docs/get-started/authentication/), command `gemini` | Configure API-key, Vertex AI or an eligible enterprise account. For personal Google sign-in in DCC, use Antigravity below. |
| Droid | [Factory Droid CLI](https://docs.factory.ai/droid-cli/quickstart), command `droid` | Open `droid` in a terminal and complete its sign-in flow. |

**Antigravity has a separate setup:** open **Settings > Models > Antigravity**,
select **Install official runtime**, then **Sign in with Google**. Follow the
[Antigravity provider guide](docs/ANTIGRAVITY_PROVIDER.md) for supported platforms
and manual runtime paths.

Installing DCC does not create a provider account or grant model access. Use an
account or API configuration with access to the models you select. For the mobile
companion, provider setup belongs on the computer running DCC, not on the phone.

For Claude, DCC retains the Anthropic Agent SDK integration and points it at your
installed CLI. No second Claude executable is included in the DCC package.

If Claude is missing or cannot run, DCC shows the provider as unavailable with
setup guidance. Install or repair the CLI, then check the provider again. Native
Claude installations do not require a separate Node.js or Bun installation for
DCC's Claude integration; older npm JavaScript installations still require Node.js.

If a provider cannot start, first verify its CLI works in a terminal under the
same account, then follow [Provider troubleshooting](SUPPORT.md#provider-troubleshooting).
Missing providers do not require installing every other provider in the table.

### Development requirements

The following tools are needed to build DCC from source, not to install the
desktop release. Individual provider CLIs may have their own runtime requirements.

- Node.js 22 recommended
- Yarn v1
- Rust stable
- OJ 0.2.2 (`cargo install oj --version 0.2.2 --locked`)
- Git

OJ is the desktop and mobile build/dev tool. Vite remains only as the peer
runtime used by Vitest 4 for the test suites.

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

Before starting your first agent session, complete [Provider setup](#provider-setup).

- Releases page: <https://github.com/wharley/DevCommandCenter/releases>
- Signed builds are published for macOS and Linux through GitHub Releases.
- macOS public release artifacts are signed/notarized app bundle archives (`.app.tar.gz`) for the updater.
- Linux public release artifacts are currently distributed as Debian packages (`.deb`) in the public release pipeline.
- Public releases are signed for this repository. Forks that want their own downloadable builds should publish from their own repository with their own signing keys and release endpoint.

## CI and releases

- GitHub Actions provide manual validation workflows for Linux and macOS.
- Signed public releases are prepared through GitHub Releases via `.github/workflows/publish-release.yml`.
- Public release publication is intentionally limited to manual dispatch or version tags.
- Manual release dispatch can target `all`, `linux-x64`, `macos-arm64`, or `macos-intel`.
- OJ installation is cached by exact version, runner OS/architecture, and OS
  version. Each restored executable is checked before use; a missing or unusable
  cache falls back to `cargo install --locked`.
- `Prepare OJ cache` runs on `main` when its setup changes, and supports manual
  dispatch on `main`. It prepares Linux and both macOS binaries for future release
  tags, which cannot directly reuse caches saved under other tags. The first run
  still compiles OJ; an evicted cache also requires recompilation. After upgrading
  OJ or changing runner OS versions, run this workflow on `main` before releasing.
- The in-app updater is configured to read `latest.json` from GitHub Releases after the first signed release is published.
- Validation workflows and release publishing are intentionally separated so signing secrets stay isolated to the protected `release` environment.

## Project docs

- [Brand identity](docs/BRAND.md)
- [Security policy](SECURITY.md)
- [Contributing guide](CONTRIBUTING.md)
- [Support](SUPPORT.md)
- [Third-party notices](docs/THIRD_PARTY_NOTICES.md)
- [Release guide](docs/RELEASING.md)
- [Codex orchestration](docs/CODEX_ORCHESTRATION.md)
- [Last Turn Review and Guarded Undo](docs/GUARDED_UNDO.md)
- [Guarded Undo engineering contract](docs/GUARDED_UNDO_DESIGN.md)
- [Delegation agents](docs/DELEGATION_AGENTS.md)
- [Mobile web companion](docs/MOBILE_WEB.md)
- [Mobile pairing security model](docs/SECURITY_MOBILE_PAIRING.md)
- [MCP trust model](docs/MCP_TRUST_MODEL.md)
- [CodeRabbit integration](docs/CODERABBIT.md)
- [Git conflict resolution](docs/GIT_CONFLICT_RESOLUTION.md)
- [Delivery workflows roadmap](docs/DELIVERY_WORKFLOWS_ROADMAP.md)
- [Antigravity provider](docs/ANTIGRAVITY_PROVIDER.md)
- [Browser workbench](docs/BROWSER_WORKBENCH.md)
- [Monaco Editor in Tauri](docs/MONACO_TAURI.md)
