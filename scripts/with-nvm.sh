#!/usr/bin/env bash
# Wrapper que garante uso da versão correta do Node via nvm
# Uso: ./scripts/with-nvm.sh <comando>
# Exemplo: ./scripts/with-nvm.sh yarn install

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Cores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Função para log
log_info() {
  echo -e "${GREEN}[DevCommandCenter]${NC} $1"
}

log_warn() {
  echo -e "${YELLOW}[DevCommandCenter]${NC} $1"
}

log_error() {
  echo -e "${RED}[DevCommandCenter]${NC} $1" >&2
}

# Verifica se .nvmrc existe
if [[ ! -f "$ROOT/.nvmrc" ]]; then
  log_error ".nvmrc não encontrado na raiz do projeto"
  exit 1
fi

REQUIRED_VERSION=$(cat "$ROOT/.nvmrc" | tr -d '[:space:]')
log_info "Versão requerida do Node: $REQUIRED_VERSION"

# Detecta a localização do nvm
NVM_DIR="${NVM_DIR:-$HOME/.nvm}"

if [[ ! -d "$NVM_DIR" ]]; then
  # Tenta outras localizações comuns
  if [[ -d "$HOME/.nvm" ]]; then
    NVM_DIR="$HOME/.nvm"
  elif [[ -f "/usr/local/opt/nvm/nvm.sh" ]]; then
    NVM_DIR="/usr/local/opt/nvm"
  elif [[ -f "/opt/homebrew/opt/nvm/nvm.sh" ]]; then
    NVM_DIR="/opt/homebrew/opt/nvm"
  else
    log_error "nvm não encontrado. Instale via: https://github.com/nvm-sh/nvm"
    log_error "Ou use Node >= 18.18.0 instalado globalmente"
    exit 1
  fi
fi

# Carrega nvm
if [[ -s "$NVM_DIR/nvm.sh" ]]; then
  log_info "Carregando nvm de: $NVM_DIR"
  # shellcheck source=/dev/null
  source "$NVM_DIR/nvm.sh"
else
  log_error "nvm.sh não encontrado em $NVM_DIR"
  exit 1
fi

# Verifica versão atual do Node
CURRENT_NODE_VERSION=$(node -v 2>/dev/null || echo "none")
log_info "Versão atual do Node: $CURRENT_NODE_VERSION"

# Usa a versão do .nvmrc
log_info "Ativando Node v$REQUIRED_VERSION..."
if ! nvm use "$REQUIRED_VERSION" 2>/dev/null; then
  log_warn "Node v$REQUIRED_VERSION não instalado. Instalando..."
  nvm install "$REQUIRED_VERSION"
  nvm use "$REQUIRED_VERSION"
fi

# Confirma a versão ativa
ACTIVE_VERSION=$(node -v)
log_info "Node ativo: $ACTIVE_VERSION"

# Executa o comando passado como argumento
if [[ $# -eq 0 ]]; then
  log_error "Uso: $0 <comando>"
  log_error "Exemplo: $0 yarn install"
  exit 1
fi

log_info "Executando: $*"
echo ""

# Executa o comando
exec "$@"
