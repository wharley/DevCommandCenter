# Ativação Beta (Licença) — Dev Command Center

Este documento descreve o que foi implementado na **camada de ativação beta**: fluxo, arquivos, banco de dados, comandos nativos e contrato com o site. Serve de referência para manutenção e para implementar o backend no site.

> **Stack atual:** o app desktop usa **Tauri 2** (Rust + WebView). Não há mais processo Electron nem `preload`; a UI fala com o backend via **`invoke`** exposto como `window.desktopAPI` (ver [`src/lib/desktop-bridge.ts`](../src/lib/desktop-bridge.ts)). Pré-requisitos de desenvolvimento: [MIGRACAO_TAURI.md](./MIGRACAO_TAURI.md).

> **Estado da implementação Rust:** confirme em [`src-tauri/src/main.rs`](../src-tauri/src/main.rs) se os comandos `license_*` já persistem no SQLite e chamam o site — enquanto retornarem *not implemented*, o fluxo de licença no app empacotado precisará dessa lógica completada no backend.

---

## O que foi implementado

- **Gate no app:** ao abrir o **app desktop (Tauri)**, se não houver ativação salva localmente, o usuário vê apenas a tela de ativação (e-mail). Após ativar (ou pular em dev), o app segue no **Hive** (`/`).
- **Persistência local:** e-mail e estado de ativação ficam no **SQLite** (tabela `activation`), no mesmo banco que o restante dos dados do app — acesso via **comandos Rust** (`window.db`) / implementação em `src-tauri`.
- **Backend remoto:** o app chama o site (https://www.devcommandcenter.com) para registrar a ativação. O site deve expor a API descrita mais abaixo.
- **Modo desenvolvimento:** link “Pular ativação (só desenvolvimento)” só em dev; em build de produção esse link não aparece.

A ativação é uma camada **antes** do shell principal (Hive).

---

## Fluxo (resumido)

1. Usuário abre o app desktop.
2. O app chama `license.getStatus()` (via `invoke` → Rust / SQLite).
3. Se **não ativado:** mostra a tela “Ative o Dev Command Center” (e-mail + botão Ativar).
4. Usuário informa e-mail e clica em Ativar.
5. O app obtém `machineId` (identificador estável da máquina), faz **POST** para `https://www.devcommandcenter.com/api/beta-activate` com `{ email, machineId }`.
6. Se o servidor responder sucesso (`ok: true`), o app grava no SQLite (e-mail, `machineId`, ativado) e chama `onActivated()` → entra no app.
7. Nas próximas aberturas, `getStatus()` retorna ativado e o app abre direto no **Hive** (`/`).

---

## Onde está no código

| Parte | Arquivos |
|-------|----------|
| **Schema e migração (DDL)** | [`lib/database/schema.sql`](../lib/database/schema.sql) (tabela `activation`); migração em [`lib/database/connection.ts`](../lib/database/connection.ts) (`migrateActivationTable`) — útil para tooling Node; o runtime Tauri usa o mesmo schema via backend Rust. |
| **Leitura/escrita local (camada TS legada / Node)** | [`lib/database/activation.ts`](../lib/database/activation.ts) (`getActivation`, `setActivation`) — referência de lógica; o app empacotado usa **SQLite no Rust**. |
| **Identificador de máquina** | Comando Rust `license_get_machine_id` em [`src-tauri/src/main.rs`](../src-tauri/src/main.rs) (não há mais `electron/services/machine-id.ts`). |
| **Comandos nativos (backend)** | [`src-tauri/src/main.rs`](../src-tauri/src/main.rs) — `license_get_status`, `license_get_machine_id`, `license_activate`, `license_skip_activation` (`#[tauri::command]`, registados no `invoke_handler`). |
| **Ponte com a UI** | [`src/lib/desktop-bridge.ts`](../src/lib/desktop-bridge.ts) — expõe `window.desktopAPI.license.*` → `invoke("license_*", …)`. |
| **Tipos globais** | [`types/app.d.ts`](../types/app.d.ts) — `desktopAPI.license` |
| **Gate e loading** | [`src/App.tsx`](../src/App.tsx) — verificação de ativação no mount, loading ou tela de ativação |
| **Tela de ativação** | [`src/pages/ActivationPage.tsx`](../src/pages/ActivationPage.tsx) — formulário, estados, link “Pular ativação” em dev (`window.desktopAPI?.license`) |

---

## Banco de dados (SQLite)

- **Tabela:** `activation`
- **Uso:** uma única linha por instalação (singleton), identificada por `id = 1`.
- **Colunas principais:** `email`, `machine_id`, `activated` (0/1), `token` (opcional), `activated_at`, `created_at`, `updated_at`.
- A tabela é criada pelo schema e pela migração em `connection.ts` para bases criadas pelo pipeline legado; **no app Tauri**, a criação/leitura final deve estar alinhada com o que o Rust (`src-tauri`) executa.

---

## Comandos de licença (Tauri)

Do ponto de vista da UI, a API continua **orientada a objetos** (`desktopAPI.license.*`). Por baixo, o bridge chama os comandos Tauri (nomes em **snake_case** no Rust):

| Uso na UI (`window.desktopAPI.license`) | Comando Rust (`invoke`) | Comportamento esperado |
|----------------------------------------|-------------------------|-------------------------|
| `getStatus()` | `license_get_status` | `{ activated: boolean, email?: string, activatedAt?: string }` — lê do SQLite. |
| `getMachineId()` | `license_get_machine_id` | String estável (ex.: hash de hostname, platform, arch, diretório de dados do app). |
| `activate(email)` | `license_activate` | Obtém `machineId`, faz POST ao site; em sucesso grava no SQLite; retorna `{ success, message? }`. |
| `skipActivation()` | `license_skip_activation` | **Só em dev** (build de desenvolvimento / não empacotado): grava ativação local sem chamar o site. Em produção deve falhar ou não expor a ação na UI. |

Em produção, a condição “só dev” no **Rust** costuma usar `cfg!(debug_assertions)` ou variáveis de ambiente no build, em vez de `app.isPackaged` do Electron.

---

## Contrato da API (site Next.js)

O **app desktop** espera o seguinte no site **https://www.devcommandcenter.com**:

- **POST** `/api/beta-activate`
  - **Body (JSON):** `{ "email": string, "machineId": string }`
  - **Sucesso (200):** `{ "ok": true, "token"?: string }`
  - **Erro (4xx):** `{ "ok": false, "message"?: string }` (ou mensagem legível)

Sugestão de implementação no site: validar e-mail (e opcionalmente `machineId`), persistir em uma tabela de ativações beta (ex.: `beta_activations`: `email`, `machine_id`, `created_at`, origem, etc.) e responder com `ok: true`. Opcional para o futuro: **GET** `/api/license/status?email=...&machineId=...` para validar se o e-mail ainda está ativo ao abrir o app.

---

## Desenvolvimento vs produção

- **Tela “Pular ativação (só desenvolvimento)”:** visível quando `import.meta.env.DEV` (Vite) em [`ActivationPage.tsx`](../src/pages/ActivationPage.tsx). Em build de produção o link não é exibido.
- **Handler `license_skip_activation`:** no Rust, só deve permitir gravação local “fake” em **modo desenvolvimento**; em release empacotado deve retornar erro ou não alterar estado.
- **Ativação real:** em qualquer ambiente, “Ativar e entrar” chama o site. Se a rota `/api/beta-activate` ainda não existir, o usuário verá erro de rede/4xx até o backend estar no ar.

---

## Resumo

- Ativação beta = gate antes do app + tela de e-mail + POST para o site + persistência no SQLite.
- **Tauri:** `main.rs` → comandos `license_*`; **frontend:** `desktop-bridge.ts` + `ActivationPage.tsx` + `App.tsx`.
- Tudo que já existia (projetos, missões, providers, Git, IA) permanece igual; só se interpõe a checagem de ativação na abertura.
- Documentação da migração e instalação: [MIGRACAO_TAURI.md](./MIGRACAO_TAURI.md). Para conceitos de uso do produto, ver [CONCEITOS_E_USO.md](./CONCEITOS_E_USO.md) e demais docs em `docs/`.
