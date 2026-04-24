#!/usr/bin/env bash
# Prepara .env e dependências em um clone ou git worktree.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  echo "devcommandcenter: execute na raiz de um repositório Git." >&2
  exit 1
}
cd "$ROOT"

# ==============================================================================
# Carrega nvm e usa a versão correta do Node antes de executar yarn install
# ==============================================================================
if [[ -f "$ROOT/.nvmrc" ]]; then
  REQUIRED_VERSION=$(cat "$ROOT/.nvmrc" | tr -d '[:space:]')

  # Detecta e carrega nvm
  NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
  NVM_SCRIPT=""

  if [[ -s "$NVM_DIR/nvm.sh" ]]; then
    NVM_SCRIPT="$NVM_DIR/nvm.sh"
  elif [[ -s "/usr/local/opt/nvm/nvm.sh" ]]; then
    NVM_SCRIPT="/usr/local/opt/nvm/nvm.sh"
  elif [[ -s "/opt/homebrew/opt/nvm/nvm.sh" ]]; then
    NVM_SCRIPT="/opt/homebrew/opt/nvm/nvm.sh"
  fi

  if [[ -n "$NVM_SCRIPT" ]]; then
    # shellcheck source=/dev/null
    source "$NVM_SCRIPT"

    # Instala a versão se necessário
    if ! nvm ls "$REQUIRED_VERSION" &>/dev/null; then
      echo "devcommandcenter: instalando Node v$REQUIRED_VERSION..."
      nvm install "$REQUIRED_VERSION"
    fi

    # Usa a versão correta
    nvm use "$REQUIRED_VERSION" >/dev/null 2>&1
    echo "devcommandcenter: usando Node $(node -v)"
  else
    echo "devcommandcenter: aviso - nvm não encontrado, usando Node global: $(node -v)" >&2
  fi
fi

# ==============================================================================
# Configura .env (symlink ou cópia de .env.example)
# ==============================================================================
if [[ -f .env ]]; then
  echo "devcommandcenter: .env já existe em $ROOT"
else
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
  elif [[ -f .env.example ]]; then
    cp .env.example .env
    echo "devcommandcenter: criado .env a partir de .env.example"
  else
    echo "devcommandcenter: aviso - sem .env.example e sem .env no worktree principal" >&2
  fi
fi

# ==============================================================================
# Instala dependências com yarn
# ==============================================================================
if [[ -f yarn.lock ]] || [[ -f package.json ]]; then
  echo "devcommandcenter: instalando dependências..."
  yarn install
  echo "devcommandcenter: dependências instaladas com sucesso"
fi

echo "devcommandcenter: setup do worktree concluído"
