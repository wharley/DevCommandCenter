#!/usr/bin/env bash
# Script de setup inicial do Dev Command Center
# Garante que a versão correta do Node está ativa antes de instalar dependências

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Cores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m' # No Color

echo -e "${BLUE}${BOLD}"
echo "╔════════════════════════════════════════════════════════════╗"
echo "║         Dev Command Center - Setup Inicial                 ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# Verifica se .nvmrc existe
if [[ ! -f ".nvmrc" ]]; then
  echo -e "${RED}✗ .nvmrc não encontrado${NC}"
  exit 1
fi

REQUIRED_VERSION=$(cat .nvmrc | tr -d '[:space:]')
echo -e "${BLUE}ℹ${NC} Versão do Node requerida: ${BOLD}v$REQUIRED_VERSION${NC}"

# Detecta nvm
NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
NVM_SCRIPT=""

if [[ -s "$NVM_DIR/nvm.sh" ]]; then
  NVM_SCRIPT="$NVM_DIR/nvm.sh"
elif [[ -s "/usr/local/opt/nvm/nvm.sh" ]]; then
  NVM_SCRIPT="/usr/local/opt/nvm/nvm.sh"
  NVM_DIR="/usr/local/opt/nvm"
elif [[ -s "/opt/homebrew/opt/nvm/nvm.sh" ]]; then
  NVM_SCRIPT="/opt/homebrew/opt/nvm/nvm.sh"
  NVM_DIR="/opt/homebrew/opt/nvm"
fi

if [[ -z "$NVM_SCRIPT" ]]; then
  echo -e "${RED}✗ nvm não encontrado${NC}"
  echo ""
  echo -e "${YELLOW}Opções:${NC}"
  echo "  1. Instalar nvm: ${BLUE}https://github.com/nvm-sh/nvm${NC}"
  echo "  2. Instalar Node >= 18.18.0 globalmente e executar: ${BOLD}yarn install${NC}"
  exit 1
fi

# Carrega nvm
echo -e "${BLUE}ℹ${NC} Carregando nvm..."
# shellcheck source=/dev/null
source "$NVM_SCRIPT"

# Verifica se a versão está instalada
if ! nvm ls "$REQUIRED_VERSION" &>/dev/null; then
  echo -e "${YELLOW}⚠${NC} Node v$REQUIRED_VERSION não encontrado"
  echo -e "${BLUE}ℹ${NC} Instalando Node v$REQUIRED_VERSION..."
  nvm install "$REQUIRED_VERSION"
fi

# Ativa a versão
echo -e "${BLUE}ℹ${NC} Ativando Node v$REQUIRED_VERSION..."
nvm use "$REQUIRED_VERSION"

# Confirma versão
ACTIVE_VERSION=$(node -v)
echo -e "${GREEN}✓${NC} Node ativo: ${BOLD}$ACTIVE_VERSION${NC}"

# Verifica se yarn está disponível
if ! command -v yarn &>/dev/null; then
  echo -e "${YELLOW}⚠${NC} Yarn não encontrado"
  echo -e "${BLUE}ℹ${NC} Habilitando corepack..."
  corepack enable
fi

# Executa yarn install
echo ""
echo -e "${BLUE}${BOLD}Instalando dependências...${NC}"
echo ""
yarn install

# Executa setup do worktree
echo ""
echo -e "${BLUE}${BOLD}Configurando worktree...${NC}"
echo ""
yarn setup-worktree

echo ""
echo -e "${GREEN}${BOLD}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}${BOLD}║  ✓ Setup concluído com sucesso!                           ║${NC}"
echo -e "${GREEN}${BOLD}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}Próximos passos:${NC}"
echo -e "  1. Configure .env conforme necessário"
echo -e "  2. Execute: ${BOLD}yarn dev${NC} para iniciar o desenvolvimento"
echo ""
echo -e "${YELLOW}Importante:${NC}"
echo -e "  • Sempre use Node v$REQUIRED_VERSION neste projeto"
echo -e "  • Execute ${BOLD}nvm use${NC} ao abrir um novo terminal"
echo -e "  • Ou adicione ao seu shell profile: ${BOLD}nvm use${NC} automático"
echo ""
