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
- **Node.js 22+** e **Yarn v1**.
- **Rust (stable)** para compilar o backend Tauri.
- **Git** instalado (essencial para o gerenciamento de Worktrees).

### Instalação e Execução
```bash
yarn install
yarn dev # Inicia o App Desktop (Tauri + Vite)
```

---

## 📚 Documentação Detalhada

- 📖 **[Guia de Produtividade & Fluxos](docs/GUIA_DE_PRODUTIVIDADE.md)** - Como extrair o máximo do DCC.
- 🏗️ **[Arquitetura do Sistema](docs/ARCHITECTURE.md)** - Como o DCC funciona por baixo do capô.
- 🛠️ **[Guia de Migração Tauri](docs/MIGRACAO_TAURI.md)** - Detalhes sobre a stack Rust/Tauri.

---

## 📄 Licença
MIT. Desenvolvido para devs, por devs.
