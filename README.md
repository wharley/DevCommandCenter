# 🕹️ Dev Command Center (DCC)

**A Central de Comando definitiva para Engenharia de Software assistida por IA.**

O DCC é uma interface local-first que orquestra agentes de IA (Claude, Gemini, Codex) diretamente no seu fluxo de trabalho Git. Ele resolve o maior gargalo da produtividade moderna: **o custo da troca de contexto.**

---

## 🚀 O que torna o DCC único?

Diferente de IDEs com chat, o DCC é um **gerenciador de estado de engenharia**. Ele não apenas conversa com a IA; ele prepara o terreno para que ela trabalhe com segurança e isolamento.

### 🏗️ Worktree-First (Adeus, `git stash`)
O DCC utiliza **Git Worktrees** para isolar cada tarefa. 
- **Fluxo:** Uma tarefa (Mission) = Um diretório isolado no disco.
- **Produtividade:** Corrija um bug crítico no Worktree A sem interromper o servidor de desenvolvimento ou "sujar" a branch que você está trabalhando no Worktree B.

### 🤖 Execução Multimodal (Panes & Agents)
Execute múltiplos agentes e terminais simultaneamente no mesmo contexto.
- **Claude Code** refatorando a lógica em um pane.
- **Gemini** gerando testes unitários em outro pane.
- **Terminal Nativo** monitorando logs em tempo real.
Tudo compartilhando o mesmo diretório de trabalho (CWD).

### 🔔 Sistema de Atenção Inteligente
Não fique vigiando o terminal. O DCC possui uma heurística que detecta quando um agente terminou ou precisa de interação, disparando notificações (Toasts/Badges) para que você mantenha o foco no que importa.

---

## 🛠️ Como usar para Máxima Produtividade

O fluxo ideal para um desenvolvedor sênior no DCC:

1.  **Contexto:** Selecione um Projeto -> Crie um **Comb** (Workspace) para sua tarefa.
2.  **Isolamento:** O DCC cria o Worktree automaticamente. 
3.  **Delegação:** Abra um **Agent Pane** e descreva a missão.
4.  **Paralelismo:** Abra um **Base Terminal** para consultar o código estável da `main` e um **Workspace Terminal** para rodar seus testes.
5.  **Revisão:** Use o sistema de Diff integrado para validar cada mudança antes do commit.

> 💡 **Dica de Especialista:** Use o DCC como um "segundo cérebro" para tarefas repetitivas (migrações, testes, documentação) enquanto você foca na arquitetura e resolução de problemas complexos.

---

## 🧱 Stack Técnica

- **Core:** Tauri 2 (Rust) + React + TypeScript.
- **Database:** SQLite local para persistência total de sessões e contextos.
- **Providers:** Abstração completa para CLI Agents e APIs (BYOK - Bring Your Own Key).
- **Terminal:** xterm.js com integração nativa PTY.

---

## 🏁 Começando

### Pré-requisitos
- **Node.js >= 18.18.0** (recomendado: **Node.js 22 LTS**)
- **Yarn v1**
- **Rust (stable)** para compilar o backend Tauri
- **Git** instalado (essencial para o gerenciamento de Worktrees)

### Instalação e Execução

#### Método Recomendado: Setup Automático

Execute o script de setup que configura tudo automaticamente:

```bash
./setup.sh
```

Este script irá:
- ✅ Detectar e usar automaticamente o Node.js 22 via nvm
- ✅ Instalar todas as dependências
- ✅ Configurar o ambiente (.env)
- ✅ Preparar o projeto para desenvolvimento

#### Método Manual

Se você já tem Node.js 22+ instalado globalmente:

```bash
yarn install
yarn dev # Inicia o App Desktop nativo (Tauri + Vite)
# ou
yarn dev:desktop # Abre apenas o shell novo em Vite
```

#### ⚠️ Problemas com Versão do Node?

Se você encontrar erros como:
```
error @aws-sdk/client-s3: The engine "node" is incompatible with this module
```

**Solução Rápida:**
```bash
# Se você usa nvm
nvm use 22  # ou nvm use
yarn install

# Se não tem nvm instalado
./setup.sh  # O script guiará você
```

📖 **Mais detalhes:** Veja [Troubleshooting: Versão do Node](docs/TROUBLESHOOTING-NODE.md)

### Git worktrees e `.env`

Arquivos `.env` **não** vão para o Git (`.gitignore`). Cada clone ou **worktree** é uma pasta nova: se algo que você roda no terminal espera variáveis em `.env`, copie ou vincule o arquivo.

- **Modelo recomendado:** na raiz do repositório há `.devcommandcenter/config.json` com um script de setup. Rode na raiz do worktree:
  ```bash
  yarn setup-worktree
  ```
  O script, se possível, cria um **symlink** do `.env` do worktree onde está a branch `main`; se não achar, copia `.env.example` → `.env`.
- **Manual:** `cp .env.example .env` ou `ln -s /caminho/absoluto/do/repo-principal/.env .env`

O template versionado é **`.env.example`** (este arquivo sim é commitado).

---

## 📚 Documentação Detalhada

- 📖 **[Guia de Produtividade & Fluxos](docs/GUIA_DE_PRODUTIVIDADE.md)** — Worktrees, panes, atenção e boas práticas.
- 🧭 **[Guia de recursos e fluxos (DCC atual)](docs/GUIA_RECURSOS_E_FLUXOS_DCC.md)** — Como usar daemon, `.dcc.toml`, processos, tasks, palette, CLI/MCP e fluxos que combinam estes benefícios.
- 🌲 **[Melhorias inspiradas no Arbor](docs/MELHORIAS_INSPIRADAS_ARBOR.md)** — Visão de produto, checklist de implementação e roadmap.
- 🏗️ **[Arquitetura do Sistema](docs/ARCHITECTURE.md)** - Como o DCC funciona por baixo do capô.
- 🛠️ **[Guia de Migração Tauri](docs/MIGRACAO_TAURI.md)** - Detalhes sobre a stack Rust/Tauri.

---

## 📄 Licença
MIT. Desenvolvido para devs, por devs.
