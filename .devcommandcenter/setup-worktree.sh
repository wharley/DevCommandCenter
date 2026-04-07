#!/usr/bin/env bash
# Prepara .env em um clone ou git worktree (mesma ideia que presets de workspace).
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  echo "devcommandcenter: execute na raiz de um repositório Git." >&2
  exit 1
}
cd "$ROOT"

if [[ -f .env ]]; then
  echo "devcommandcenter: .env já existe em $ROOT — nada a fazer."
  exit 0
fi

MAIN_ROOT="$(git worktree list --porcelain | awk '
  /^worktree / { cur = substr($0, 10); if (first == "") first = cur }
  /^branch refs\/heads\/main$/ { main_path = cur }
  END {
    if (main_path != "") print main_path
    else if (first != "") print first
  }
')"

if [[ -n "$MAIN_ROOT" && -f "$MAIN_ROOT/.env" && "$MAIN_ROOT" != "$ROOT" ]]; then
  ln -sf "$MAIN_ROOT/.env" .env
  echo "devcommandcenter: symlink .env → $MAIN_ROOT/.env"
  exit 0
fi

if [[ -f .env.example ]]; then
  cp .env.example .env
  echo "devcommandcenter: criado .env a partir de .env.example (edite se precisar)."
  exit 0
fi

echo "devcommandcenter: sem .env.example e sem .env no worktree principal — crie .env manualmente." >&2
exit 1
