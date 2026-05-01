# Migração Electron → Tauri 2

Este documento explica **por que** migramos, **o que mudou** no projeto e **o que instalar** para desenvolver e gerar builds com sucesso.

---

## Por que migramos

| Aspecto | Antes (Electron) | Agora (Tauri 2) |
|--------|-------------------|-----------------|
| **Processo principal** | Node.js + Chromium embarcado | Binário nativo **Rust** + **WebView** do SO |
| **Tamanho do app** | Runtime Chromium grande | Pacotes bem menores em geral |
| **Segurança / superfície** | Dois runtimes (Node + browser) | IPC controlado; backend em Rust |
| **Backend** | `electron/main`, IPC, `better-sqlite3`, `node-pty` | `src-tauri` (Rust): SQLite, Git, terminal, etc. |
| **Ponte com a UI** | `preload` → `window.electronAPI` / `window.db` | `installDesktopBridge()` → `window.desktopAPI` / `window.db` via `invoke` |

Objetivos da migração: **menor footprint**, **stack alinhada (Rust no core)**, **mesma UX React** mantendo Vite + React 19, e **um único caminho** de build desktop (`tauri build`).

---

## O que mudou no código (visão rápida)

- **Pasta `electron/`** removida (main, preload, IPC Node).
- **Bridge no frontend:** `src/lib/desktop-bridge.ts` — função `installDesktopBridge()` (antes `installTauriElectronCompat`), expõe APIs compatíveis com o antigo preload.
- **Globais tipados:** `types/app.d.ts` — `window.desktopAPI` e `window.db` (não usamos mais o nome “electron” nos tipos globais).
- **Scripts npm/yarn:**
  - `yarn dev` → **`yarn dev:tauri`** (sobe o Vite e abre o app nativo; ver `src-tauri/tauri.conf.json`: `beforeDevCommand`).
  - `yarn dev:desktop` → shell novo em Vite, útil para inspecionar a UI sem a janela nativa.
  - `yarn build` → **`tauri build`** (frontend `vite:build` + compilação Rust).
  - `yarn vite` / `yarn vite:build` → só o frontend (útil para UI sem shell nativo; **sem** DB/Git/terminal reais).

---

## O que precisa estar instalado

Sem estes itens, **`yarn dev`** ou **`yarn build`** falham em etapas diferentes (Node, Rust ou dependências de sistema).

### 1. Obrigatório em todos os sistemas

| Ferramenta | Versão sugerida | Para quê |
|------------|-----------------|----------|
| **Node.js** | 22+ (LTS) | Vite, TypeScript, Yarn |
| **Yarn** | Classic (v1) ou Berry | Este repo usa `yarn.lock` no estilo v1 |
| **Git** | recente | Worktrees, comandos usados pelo backend |
| **Rust** | **stable** (via [rustup](https://rustup.rs/)) | Compilar `src-tauri` |
| **Cargo** | vem com Rust | `cargo build` dentro do Tauri |

Instalar Rust (macOS / Linux / Windows):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Depois confirme:

```bash
rustc --version
cargo --version
```

O **CLI do Tauri** já vem como dependência de desenvolvimento (`@tauri-apps/cli` no `package.json`). Os comandos `yarn dev` e `yarn build` chamam `tauri` via `yarn`; não é obrigatório instalar o binário global `tauri`, mas pode instalar com:

```bash
cargo install tauri-cli --version "^2.0.0"
```

(apenas se quiseres usar `tauri` fora do `yarn`.)

### 2. macOS

- **Xcode Command Line Tools** (compilador C/C++ para crates nativas):

  ```bash
  xcode-select --install
  ```

- Para builds iOS (opcional, só se fores além do desktop): Xcode completo — não é necessário só para app macOS desktop.

### 3. Linux (Ubuntu / Debian e derivados)

Pacotes usuais para WebView GTK e toolchain (ajusta nomes se a tua distro usar outros pacotes):

```bash
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libssl-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf
```

Outras distribuições: segue a [documentação oficial do Tauri — pré-requisitos Linux](https://v2.tauri.app/start/prerequisites/) (pacotes equivalentes para Fedora, Arch, etc.).

### 4. Windows

- **Microsoft C++ Build Tools** (Visual Studio Installer → workload “Desktop development with C++”) ou **Visual Studio Build Tools**.
- **WebView2** — normalmente já presente no Windows 10/11; o instalador do Tauri/WebView2 pode pedir download se faltar.
- Confirma também **Git for Windows** se usares Git pela UI/terminal.

Guia oficial: [Tauri prerequisites — Windows](https://v2.tauri.app/start/prerequisites/).

---

## Fluxo típico após clonar o repositório

```bash
# 1. Dependências Node (na raiz do repo)
yarn install
# Se o teu ambiente exigir (alguns monorepos / engines):
# yarn install --ignore-engines

# 2. Desenvolvimento desktop (Vite + Rust + janela Tauri)
yarn dev

# 3. Só interface web (sem APIs nativas / sem DB real via Tauri)
yarn vite
```

O primeiro `yarn dev` pode demorar: o Cargo compila todas as dependências Rust.

---

## Build de produção

```bash
yarn build
```

Artefatos ficam em `src-tauri/target/release/bundle/` (formato depende do SO e da config em `src-tauri/tauri.conf.json`).

---

## Problemas comuns

| Sintoma | Causa provável |
|--------|----------------|
| `cargo: command not found` | Rust não instalado ou `PATH` sem `~/.cargo/bin` |
| Erro ao compilar crate X no Linux | Falta pacote `-dev` (webkit/gtk); vê secção Linux acima |
| `yarn dev` abre só Vite mas não a janela | Executar na **raiz** do projeto; verificar se `@tauri-apps/cli` está instalado |
| App web sem dados | Esperado com `yarn vite` — não há `window.desktopAPI` sem Tauri |

---

## Referências

- [Tauri 2 — Prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri 2 — Develop](https://v2.tauri.app/develop/)
- Código do bridge: `src/lib/desktop-bridge.ts`
- Backend: `src-tauri/`

---

*Última atualização: alinhado à remoção do Electron e scripts `tauri dev` / `tauri build` na raiz do projeto.*
