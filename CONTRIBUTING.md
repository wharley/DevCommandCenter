# Contributing

## Before you start

- Open an issue or start a discussion for large changes.
- Keep pull requests focused and reviewable.
- Do not commit secrets, tokens, certificates, or local environment files.

## Development setup

Requirements:

- Node.js 22 recommended
- Yarn v1
- Rust stable
- Git

Setup:

```bash
./setup.sh
```

Or manually:

```bash
yarn install
yarn dev
```

For a new clone or worktree:

```bash
yarn setup-worktree
```

## Validation

Run the smallest checks that prove your change:

```bash
yarn workspace @dcc/desktop test
yarn workspace @dcc/mobile-web build
cargo check
```

Add or update tests when behavior changes.

## Pull request guidelines

- Explain the user-visible change clearly.
- Mention risks, edge cases, and follow-up work.
- Keep documentation in sync with the current repository state.
- Avoid unrelated cleanup in the same pull request unless it is required.

## Security and privacy

- Never commit `.env` files.
- Never commit signing certificates or private keys.
- Be careful when editing `.github/workflows/` because CI may have access to release secrets.
