# Troubleshooting: Problemas com Versão do Node.js

## Erro: "The engine 'node' is incompatible with this module"

### Sintomas

```
error @aws-sdk/client-s3@3.812.0: The engine "node" is incompatible with this module. Expected version ">=18.0.0". Got "16.14.2"
error Found incompatible module.
```

### Causa

O projeto requer **Node.js >= 18.18.0**, mas o sistema está usando uma versão mais antiga (tipicamente 16.x).

## Soluções

### ✅ Solução 1: Script de Setup Automático (Recomendado)

Execute o script de setup que configura tudo automaticamente:

```bash
./setup.sh
```

Este script irá:
- Detectar e carregar o nvm
- Instalar Node v22 se necessário
- Ativar a versão correta
- Instalar todas as dependências
- Configurar o ambiente

### ✅ Solução 2: Configuração Manual com nvm

Se você já tem o nvm instalado:

```bash
# 1. Ativar a versão do Node especificada no .nvmrc
nvm use

# 2. Se a versão não estiver instalada
nvm install 22
nvm use 22

# 3. Instalar dependências
yarn install
```

### ✅ Solução 3: Instalar nvm pela Primeira Vez

Se você não tem o nvm instalado:

**macOS/Linux:**
```bash
# Instalar nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash

# Reiniciar o terminal ou carregar nvm
source ~/.bashrc  # ou ~/.zshrc se usar zsh

# Instalar e usar Node 22
nvm install 22
nvm use 22

# Voltar ao projeto e executar setup
./setup.sh
```

**Windows:**
Use [nvm-windows](https://github.com/coreybutler/nvm-windows)

### ✅ Solução 4: Instalar Node.js Globalmente

Se você não quer usar nvm, instale o Node.js 22 globalmente:

1. Baixe de: https://nodejs.org/ (versão LTS 22.x)
2. Instale
3. Execute: `yarn install`

## Configuração Automática do nvm

Para evitar ter que executar `nvm use` toda vez que abrir o terminal:

### Opção A: Auto-switch com direnv

```bash
# Instalar direnv
brew install direnv  # macOS
# ou
sudo apt install direnv  # Ubuntu/Debian

# Adicionar ao shell (~/.zshrc ou ~/.bashrc)
eval "$(direnv hook zsh)"  # para zsh
eval "$(direnv hook bash)" # para bash

# Criar .envrc no projeto
echo "use nvm" > .envrc
direnv allow
```

### Opção B: Auto-switch no shell profile

Adicione ao final do `~/.zshrc` ou `~/.bashrc`:

```bash
# Auto-load .nvmrc
autoload -U add-zsh-hook
load-nvmrc() {
  local node_version="$(nvm version)"
  local nvmrc_path="$(nvm_find_nvmrc)"

  if [ -n "$nvmrc_path" ]; then
    local nvmrc_node_version=$(nvm version "$(cat "${nvmrc_path}")")

    if [ "$nvmrc_node_version" = "N/A" ]; then
      nvm install
    elif [ "$nvmrc_node_version" != "$node_version" ]; then
      nvm use
    fi
  elif [ "$node_version" != "$(nvm version default)" ]; then
    echo "Reverting to nvm default version"
    nvm use default
  fi
}
add-zsh-hook chpwd load-nvmrc
load-nvmrc
```

### Opção C: Shell Wrapper Script

Use o wrapper fornecido para executar comandos com a versão correta:

```bash
./scripts/with-nvm.sh yarn install
./scripts/with-nvm.sh yarn dev
```

## Verificação

Para confirmar que está usando a versão correta:

```bash
node -v
# Deve mostrar: v22.x.x
```

## Scripts Úteis

- `./setup.sh` - Setup inicial completo
- `./scripts/with-nvm.sh <comando>` - Executa comando com versão correta do Node
- `yarn setup` - Alias para ./setup.sh

## Problemas Comuns

### "nvm: command not found"

**Causa:** nvm não está instalado ou não foi carregado no shell.

**Solução:**
1. Instale o nvm (ver Solução 3 acima)
2. OU adicione ao `~/.zshrc` / `~/.bashrc`:
   ```bash
   export NVM_DIR="$HOME/.nvm"
   [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
   ```

### "Node v22 not installed"

**Solução:**
```bash
nvm install 22
nvm use 22
```

### Erro persiste mesmo depois de mudar versão

**Causa:** node_modules foi instalado com versão antiga.

**Solução:**
```bash
rm -rf node_modules yarn.lock
nvm use 22
yarn install
```

### Problema em CI/CD

Se estiver tendo problemas em pipelines de CI/CD:

**GitHub Actions:**
```yaml
- uses: actions/setup-node@v4
  with:
    node-version-file: '.nvmrc'
```

**GitLab CI:**
```yaml
image: node:22
```

## Mais Ajuda

Se o problema persistir:
1. Verifique o conteúdo do `.nvmrc`: `cat .nvmrc`
2. Verifique a versão ativa: `node -v`
3. Verifique se nvm está carregado: `nvm --version`
4. Limpe o cache: `yarn cache clean`
5. Abra uma issue no repositório com os detalhes do erro
