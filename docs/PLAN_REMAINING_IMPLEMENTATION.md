# O que falta para 100% do plano (dmux vs DCC)

Este documento lista o que ainda não foi implementado das seções 7 (Híbrido) e 8 (Layout/UI/UX) do plano, em ordem sugerida de entrega.

---

## 1. Seção 7 – Híbrido (runtime)

### 1.1 Worktrees + paralelo no pipeline (prioridade alta)

**Objetivo:** Várias missões do mesmo projeto rodando ao mesmo tempo (Gerar plano / Gerar código), cada uma em sua própria worktree, sem conflito de Git.

**O que fazer:**

| Item                     | Descrição                                                                                                                                                                                                                                                       |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Serviço de worktrees** | Criar `electron/services/worktree-service.ts` (ou estender o git-service): criar worktree por missão (`git worktree add <path> -b <branch>`), obter path da worktree, limpar ao fechar/cancelar missão.                                                         |
| **Branch por missão**    | Convenção de branch (ex.: `dcc-mission-<missionId>` ou slug) e persistir `worktreePath` / `worktreeBranch` na missão (ou em tabela/cache).                                                                                                                      |
| **Orquestrador**         | No `ai-orchestrator`, ao invés de bloquear “uma missão por projeto” para gerar plano/código, permitir N missões em paralelo: cada uma usa `projectContext`/cwd da **sua** worktree. Ou seja: `getProjectContext(mission.worktreePath)` quando existir worktree. |
| **Criação de worktree**  | Ao criar missão ou ao clicar “Gerar plano”, se não houver worktree para essa missão, criar worktree + branch antes de chamar o adapter.                                                                                                                         |
| **Merge / descarte**     | Ao concluir ou cancelar missão, opção de merge da branch da worktree no branch principal ou só remover worktree. Menu ou botão “Incorporar alterações” / “Descartar worktree”.                                                                                  |

**Dependências:** Git 2.20+ com `worktree`; projeto já deve ser um repositório Git.

---

### 1.2 Modo “agente ao vivo” (prioridade média)

**Objetivo:** Abrir o CLI (Codex, Claude, Cursor, Gemini) no contexto da missão/projeto e trabalhar de forma interativa no app (terminal embutido) ou no terminal externo.

**Recomendação do plano: Opção B (sem lib) primeiro.**

Implementar **“Abrir no terminal”** (worktree + abrir o Terminal do OS no path + comando sugerido). Deixar terminal embutido (xterm + node-pty) como **opcional/fase 2**, só se houver demanda. Motivos: (1) entrega rápida, sem dependência nativa (node-pty costuma dar trabalho de build no Electron); (2) quem já usa Codex/Claude no terminal está acostumado ao app do sistema; (3) alinhado ao dmux, onde o agente roda em terminal real (tmux); (4) se depois surgir demanda por “tudo na mesma janela”, avalia-se xterm+node-pty.

**Por que o dmux não usa xterm/node-pty:** O dmux é uma TUI (Ink + React) que roda dentro do terminal; os panes são panes reais do tmux. Por isso as dependências do dmux são chalk, ink, react etc. — não há widget de terminal embutido. No DCC (GUI Electron), “terminal dentro do app” exigiria xterm+node-pty; “abrir no terminal” do OS não exige lib extra.

---

**Opção B – “Abrir no terminal” (recomendada, sem lib)**

| Item                    | Descrição                                                                                                                                                                                                                 |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Botão na missão**     | “Abrir no terminal”: cria worktree (se não existir), abre o terminal do OS (macOS Terminal, Windows WT, etc.) no path da worktree e opcionalmente cola comando sugerido (ex.: `codex` ou `claude "descrição da missão"`). |
| **Sem terminal no app** | Não precisa de xterm/node-pty; usa `shell.openPath` ou `child_process.spawn` com terminal externo.                                                                                                                        |

**Opção A – Terminal embutido (fase 2, opcional)**

| Item                      | Descrição                                                                                                                                                             |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **xterm.js + node-pty**   | Instalar `xterm` e `node-pty` no Electron; criar componente React que renderiza o terminal e inicia processo (ex.: `codex` ou `claude`) com cwd = worktree da missão. |
| **Abas/painéis**          | Uma aba ou painel por “sessão ao vivo”; barra de abas com nome da missão; botão “Fechar sessão” e opcional “Abrir em terminal externo”.                               |
| **Integração com missão** | Botão “Abrir com agente ao vivo” na missão (ou no board) que cria worktree (se ainda não existir), abre aba/painel e lança o CLI nessa worktree.                      |
| **Estado**                | Persistir “sessões abertas” (missionId + worktreePath) para restaurar ao reabrir o app, se desejado.                                                                  |

---

## 2. Seção 8 – Layout e UI/UX (identidade e modo foco)

### 2.1 Identidade visual (prioridade média)

| Item                   | Descrição                                                                                                                                           |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Tipografia**         | Trocar fontes genéricas por uma para títulos (ex.: Geist, Satoshi, Plus Jakarta) e uma para corpo/UI; configurar em `index.html` ou Tailwind/theme. |
| **Grid e espaçamento** | Padronizar padding/gaps (ex.: 4/8px); garantir que colunas do Kanban usem o mesmo grid.                                                             |
| **Paleta semântica**   | Revisar variáveis CSS (primária, sucesso, aviso, erro, neutros) e garantir contraste acessível em dark/light.                                       |
| **Microanimações**     | Transições curtas (ex.: 200–300 ms) para troca de coluna no board, aparecer/desaparecer card, loading; evitar animações longas.                     |

**Onde:** `tailwind.config`, `index.html`, componentes do board e cards; possivelmente um `design-tokens.css` ou tema centralizado.

---

### 2.2 Modo foco (prioridade baixa)

**Objetivo:** Vista “uma missão em foco” — detalhe em tela cheia ou painel principal, sensação de “commander”.

| Item               | Descrição                                                                                                                                                                        |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Rota ou estado** | Ex.: rota `/project/:id/mission/:missionId/focus` ou query `?view=focus` na MissionPage.                                                                                         |
| **Layout**         | Na MissionPage, quando em “modo foco”: esconder sidebar ou reduzir; conteúdo da missão (plano, código, ações) ocupa a área principal; opção de “sair do foco” e voltar ao board. |
| **Atalho**         | Ex.: ⌘F ou botão “Modo foco” na barra da missão.                                                                                                                                 |

---

### 2.3 Ajuda e atalhos (prioridade baixa)

| Item                     | Descrição                                                                                         |
| ------------------------ | ------------------------------------------------------------------------------------------------- |
| **Menu ou página Ajuda** | Listar atalhos (ex.: ⌘N nova missão, ⌘F modo foco quando existir) e opcionalmente link para docs. |
| **Tooltips**             | Manter/estender tooltips em botões principais com o atalho (ex.: “Nova missão (⌘N)”).             |

---

## 3. Resumo em checklist

- [x] **Worktrees:** serviço + branch por missão + uso no orchestrator
- [x] **Paralelo no pipeline:** N missões gerando ao mesmo tempo (cada uma na sua worktree)
- [x] **Merge/descarte:** UI para incorporar ou descartar worktree ao concluir/cancelar
- [x] **Agente ao vivo (recomendado):** botão “Abrir no terminal” + worktree + comando sugerido — **sem lib** (Opção B)
- [ ] **Agente ao vivo (opcional/fase 2):** terminal embutido (xterm + node-pty) + abas por sessão
- [x] **Identidade visual:** tipografia, grid, paleta, microanimações
- [x] **Modo foco:** vista “uma missão em foco” (rota ou estado + layout)
- [x] **Ajuda:** menu/página com lista de atalhos

O plano considera **100%** com a Opção B (agente ao vivo sem lib). O terminal embutido (xterm+node-pty) é evolução opcional.
