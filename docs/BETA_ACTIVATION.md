# Ativação Beta (Licença) — Dev Command Center

Este documento descreve o que foi implementado na **camada de ativação beta**: fluxo, arquivos, banco de dados, IPC e contrato com o site. Serve de referência para manutenção e para implementar o backend no site.

---

## O que foi implementado

- **Gate no app:** ao abrir o Electron, se não houver ativação salva localmente, o usuário vê apenas a tela de ativação (e-mail). Após ativar (ou pular em dev), o app segue normal (Home, Projetos, Missões, Configurações).
- **Persistência local:** e-mail e estado de ativação ficam no SQLite (tabela `activation`), junto com o restante dos dados do app.
- **Backend remoto:** o app chama o site (https://www.devcommandcenter.com) para registrar a ativação. O site deve expor a API descrita mais abaixo.
- **Modo desenvolvimento:** link “Pular ativação (só desenvolvimento)” só em dev; em build de produção esse link não aparece.

Nenhuma rota, layout ou fluxo existente (projetos, missões, providers) foi alterado; a ativação é uma camada **antes** do app principal.

---

## Fluxo (resumido)

1. Usuário abre o app Electron.
2. O app chama `license.getStatus()` (lê do SQLite).
3. Se **não ativado:** mostra a tela “Ative o Dev Command Center” (e-mail + botão Ativar).
4. Usuário informa e-mail e clica em Ativar.
5. O app obtém `machineId` (identificador estável da máquina), faz **POST** para `https://www.devcommandcenter.com/api/beta-activate` com `{ email, machineId }`.
6. Se o servidor responder sucesso (`ok: true`), o app grava no SQLite (e-mail, `machineId`, ativado) e chama `onActivated()` → entra no app.
7. Nas próximas aberturas, `getStatus()` retorna ativado e o app abre direto no fluxo normal (MainLayout + rotas).

---

## Onde está no código

| Parte | Arquivos |
|-------|----------|
| **Schema e migração** | `lib/database/schema.sql` (tabela `activation`), `lib/database/connection.ts` (função `migrateActivationTable`) |
| **Leitura/escrita local** | `lib/database/activation.ts` (`getActivation`, `setActivation`) |
| **Identificador de máquina** | `electron/services/machine-id.ts` (`getMachineId`) |
| **IPC (main)** | `electron/ipc-handlers.ts` — handlers `license:getStatus`, `license:getMachineId`, `license:activate`, `license:skipActivation` |
| **Preload e tipos** | `electron/preload.ts` (exposição de `license.*`), `types/electron.d.ts` (tipos de `license`) |
| **Gate e loading** | `src/App.tsx` — verificação de ativação no mount, exibição de loading ou tela de ativação |
| **Tela de ativação** | `src/pages/ActivationPage.tsx` — formulário de e-mail, estados de loading/erro/sucesso, link “Pular ativação” em dev |

---

## Banco de dados (SQLite)

- **Tabela:** `activation`
- **Uso:** uma única linha por instalação (singleton), identificada por `id = 1`.
- **Colunas principais:** `email`, `machine_id`, `activated` (0/1), `token` (opcional), `activated_at`, `created_at`, `updated_at`.
- A tabela é criada pelo schema e pela migração em `connection.ts`; bancos já existentes ganham a tabela na primeira inicialização após o deploy.

---

## IPC de licença (Electron)

- **`license:getStatus`** — Retorna `{ activated: boolean, email?: string, activatedAt?: string }`. Em dev não altera a lógica; em produção lê do SQLite.
- **`license:getMachineId`** — Retorna string estável (hash de hostname, platform, arch, userData path).
- **`license:activate`** — Recebe `email`; obtém `machineId`, faz POST para o site, em sucesso grava no SQLite e retorna `{ success: true }`; em falha retorna `{ success: false, message?: string }`.
- **`license:skipActivation`** — Só em dev (`NODE_ENV === "development"` ou `!app.isPackaged`). Grava ativação local (e-mail dev) sem chamar o site; retorna `{ success: true }`. Em produção retorna `{ success: false }`.

---

## Contrato da API (site Next.js)

O app Electron espera o seguinte no site **https://www.devcommandcenter.com**:

- **POST** `/api/beta-activate`
  - **Body (JSON):** `{ "email": string, "machineId": string }`
  - **Sucesso (200):** `{ "ok": true, "token"?: string }`
  - **Erro (4xx):** `{ "ok": false, "message"?: string }` (ou mensagem legível)

Sugestão de implementação no site: validar e-mail (e opcionalmente `machineId`), persistir em uma tabela de ativações beta (ex.: `beta_activations`: `email`, `machine_id`, `created_at`, origem, etc.) e responder com `ok: true`. Opcional para o futuro: **GET** `/api/license/status?email=...&machineId=...` para validar se o e-mail ainda está ativo ao abrir o app.

---

## Desenvolvimento vs produção

- **Tela “Pular ativação (só desenvolvimento)”:** visível apenas quando `import.meta.env.DEV` (ou equivalente) é true, ou seja, em modo desenvolvimento (Vite). Em build de produção o link não é exibido.
- **Handler `license:skipActivation`:** só executa a gravação local quando o processo main está em dev (`process.env.NODE_ENV === "development"` ou `!app.isPackaged`). Em app empacotado retorna `success: false`.
- **Ativação real:** em qualquer ambiente, “Ativar e entrar” chama o site. Se a rota `/api/beta-activate` ainda não existir, o usuário verá erro de rede/4xx até o backend estar no ar.

---

## Resumo

- Ativação beta = gate antes do app + tela de e-mail + POST para o site + persistência no SQLite.
- Tudo que já existia (projetos, missões, providers, Git, IA) permanece igual; só se interpõe a checagem de ativação na abertura.
- Documentação técnica está neste arquivo; para conceitos de uso do produto, ver `CONCEITOS_E_USO.md` e demais docs em `docs/`.
