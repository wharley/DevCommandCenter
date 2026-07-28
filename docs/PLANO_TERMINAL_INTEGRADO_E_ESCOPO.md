# Plano — Terminal integrado (uso geral) + abas, e guarda de escopo

> Documento de planejamento. Lista **apenas os pontos que fazem sentido para o DCC**, respeitando o
> foco do produto (vertical, *worktree-first / mission-based*). Pontos avaliados e descartados estão
> na seção [Fora de escopo](#fora-de-escopo).

---

## 0. Princípio que guia tudo

O DCC é um **gerenciador de estado de engenharia** vertical (worktree/mission), **não** um canivete
suíço horizontal. Toda evolução abaixo reforça esse eixo ou tira atrito sem inventar um produto novo.

---

## 1. Terminal integrado de uso geral (fora do worktree) com abas — **prioridade**

### 1.1 Objetivo

Hoje, para tarefas de projeto que não são da missão isolada (rodar script geral, `git` na main,
inspecionar a raiz do repo), o usuário **precisa sair do DCC e abrir o terminal do sistema**. A ideia
é trazer esse terminal para dentro do app.

**Decisão de escopo (CWD):** o terminal embutido abre na **raiz do projeto (`rootPath`)**, **não** no
worktree.
- O contexto do *worktree* já é coberto pelo recurso **"abrir IDE no worktree"** — a IDE escolhida
  pelo usuário abre no worktree e tem o próprio terminal lá dentro. Duplicar isso no DCC seria
  redundante.
- A raiz do projeto (`rootPath`) é o nível "acima" do worktree — é onde o usuário faria `cd` ao abrir
  um terminal de sistema no projeto.
- Bônus de robustez: `rootPath` sempre existe; `worktreePath` pode ser `null` antes da missão criar o
  worktree. Abrir em `rootPath` deixa o terminal disponível sempre.

### 1.2 Modelo de abas

- Vários PTYs simultâneos, exibidos em abas dentro do drawer inferior já existente.
- **Escopo das abas: por projeto.** Ao trocar de projeto, vê-se o conjunto de terminais daquele
  projeto (cada um com cwd = `rootPath` do projeto). É o equivalente a "abri um terminal já dentro da
  pasta do projeto".
- Ações por aba: abrir (`+`), fechar (`x`, mata o PTY), focar, limpar. Renomear aba é opcional (nice
  to have).

### 1.3 Por que é barato (a fundação já existe)

| Peça | Estado atual | O que muda |
|------|--------------|------------|
| Backend PTY (`getOrCreateTerminalByOwner`, `apps/desktop/src/lib/terminal-api.ts`) | Já aceita **owner key arbitrária** → já suporta N PTYs | Nada |
| `terminal-store.ts` | Chaveia por `workspaceEntryKey = scope:workspaceId`; owner `workspace:${id}`; cwd = worktree | Chavear por **terminalId**; owner `project:${projectId}#${terminalId}`; cwd = `rootPath` |
| `terminal-panel.tsx` | Renderiza **1** terminal por workspace | Renderiza o terminal da **aba ativa** |
| `WorkspaceTerminalDrawer` | Drawer com resize, 1 terminal | + **tab strip** no topo (já existe `components/ui/tabs.tsx`) |
| Gatilho | `terminalDrawerOpen` em `session-workbench.tsx:91` | Botão `+ Terminal` na toolbar do composer/workbench abre o drawer e spawna aba |
| CWD origem | `terminalWorkspacePath = selectedWorkspacePath` (`= worktreePath ?? rootPath`) em `App.tsx:524`/`2131` | Passar `selectedWorkspace?.rootPath` |

### 1.4 Mudanças técnicas (passo a passo)

1. **`terminal-store.ts`** — generalizar o chaveamento:
   - Substituir a chave `workspaceId` por um `terminalId` (uuid por aba). Owner key:
     `project:${projectId}#${terminalId}` com `cwd = rootPath`.
   - As funções (`ensure*`, `attach*`, `detach*`, `write*`, `resize*`, `kill*`, `clear*`) passam a
     receber `terminalId`. A lógica interna (`appendChunk`, `ptyToWorkspace` → `ptyToTerminal`,
     replay, truncamento) já é genérica — só troca a identidade.
   - Manter um índice `projectId -> terminalId[]` para listar as abas de um projeto.
2. **Tab strip** no `WorkspaceTerminalDrawer` usando `components/ui/tabs.tsx`: lista de abas + `+` + `x`.
   Persistir a lista de abas por projeto em `localStorage` (mesmo padrão da altura do drawer).
3. **`terminal-panel.tsx`** — receber `terminalId` em vez de derivar de `workspaceId`; renderizar a aba
   ativa. (Cada aba mantém seu próprio `xterm` montado para preservar o buffer ao alternar.)
4. **Gatilho no composer** — botão `+ Terminal` na toolbar do composer (`features/composer/`) que abre
   o drawer e cria uma aba nova com cwd = `rootPath`. O composer é o *gatilho*; o drawer com abas é a
   *superfície* (não embutir o terminal dentro do campo de input — briga com digitação e resize).
5. **`App.tsx` / `session-workbench.tsx`** — passar `rootPath` (em vez de `worktreePath`) como cwd do
   terminal embutido.

### 1.5 Cuidados / limites

- **PTYs órfãos:** ao fechar/trocar projeto, matar (ou suspender) os PTYs das abas para não vazar
  processos. Decidir: matar ao fechar projeto vs. manter vivos em background (recomendado: manter o
  conjunto do projeto vivo enquanto o app está aberto, matar ao sair).
- **Limite suave:** avisar a partir de ~5 abas por projeto.
- **Não remover** o conceito de terminal por worktree do backend caso futuramente uma aba precise
  apontar para o worktree — mas isso **não** é o caso de uso atual.

### 1.6 Pronto quando

- Consigo abrir ≥2 abas de terminal dentro do DCC, ambas em `rootPath` do projeto selecionado.
- Trocar de projeto troca o conjunto de abas; voltar reanexa aos PTYs (buffer preservado).
- Fechar o drawer **não** mata os shells; reabrir reanexa.
- Não preciso mais abrir o terminal do sistema para tarefas de projeto.

---

## 2. MCP em duas direções

### 2.1 MCP-server expondo primitivas do DCC — **implementado**

Validado pelo Claude (que expõe 45+ tools via MCP). Alinhado ao foco: expor as **primitivas próprias
do DCC** (worktrees, missions, status/atenção de agentes) como servidor MCP para *outros* agentes
consumirem. Diferenciado e dentro do eixo.

### 2.2 DCC consumindo MCPs externos — **roadmap próprio**

É a direção inversa: cadastrar um servidor externo por URL ou comando e disponibilizá-lo, com escopo,
permissões e lifecycle confiáveis, para cada provider cujo adapter tenha compatibilidade comprovada.
Figma, gateways de pagamento e ferramentas de observabilidade são exemplos desse fluxo.

O plano técnico, o modelo de confiança open source e os critérios que impedem suporte por heurística
estão em [`MCP_INTEGRATIONS_ROADMAP.md`](MCP_INTEGRATIONS_ROADMAP.md).

---

## 3. Skills como objeto de primeira classe — UI estilo Zed — **faz sentido**

> Análise técnica completa já existe em [`docs/SKILLS_POR_PROJETO_ANALISE.md`](SKILLS_POR_PROJETO_ANALISE.md).
> Este ponto **não a repete** — só registra a decisão e amarra o modal do Zed (referência de UI
> trazida pelo usuário) ao roadmap que já está naquele doc.

### 3.1 Princípio: skill é **provider-neutra**, não Claude-cêntrica

O DCC já orquestra Claude, Codex, Droid, Cursor, Gemini — e outros virão. Portanto a skill **não pode**
nascer amarrada a `.claude/skills/`. Esse foi o motivo de olhar o Zed: o modal dele salva num caminho
**neutro** (`.agents/skills/<name>/skill.md`), não no formato de uma ferramenta específica.

No DCC isso vira a tese central da [análise](SKILLS_POR_PROJETO_ANALISE.md): **uma fonte de verdade
única** (a pasta que o DCC possui, `.devcommandcenter/skills/`) que o DCC **compila/projeta** para o
caminho nativo de cada agente. O usuário escreve a skill **uma vez**; o DCC entrega no formato que cada
agente entende. Esse é o diferencial — e está dentro do foco ("abstração completa para CLI Agents").

```
FONTE ÚNICA (neutra)                  COMPILADOR (DCC)            ALVOS NATIVOS (lidos pelos agentes)
.devcommandcenter/skills/<name>/  ─►  no setup-worktree.sh  ─┬─►  .claude/skills/<name>/SKILL.md   (cópia fiel)
  SKILL.md  (name/desc + corpo)                              ├─►  AGENTS.md   (Codex, Droid, e o padrão emergente)
  manifest (agentes-alvo, escopo)                            ├─►  GEMINI.md
                                                             └─►  .cursor/rules/<name>.mdc
```

**Insight que torna isto tratável:** não são "5 transpilers". `AGENTS.md` está virando padrão
cross-tool (Codex, Droid e provavelmente os futuros). Então **3 alvos** — `.claude/skills/` (cópia,
custo zero), `AGENTS.md` (flatten, cobre a maioria) e `GEMINI.md` — já cobrem quase todo mundo;
`.cursor/rules/` é opcional.

### 3.2 O que muda por agente (e a perda honesta)

| Agente | Caminho nativo | Progressive disclosure | Como o DCC entrega |
|---|---|---|---|
| Claude | `.claude/skills/<name>/SKILL.md` | ✅ sim (já ativo: `settingSources` inclui `project`) | cópia fiel — **sem perda** |
| Codex / Droid | `AGENTS.md` | ❌ não | seção delimitada e idempotente (`<!-- dcc:skills -->`), instrução sempre presente |
| Gemini | `GEMINI.md` | ❌ não | idem |
| Cursor | `.cursor/rules/<name>.mdc` | ⚠️ parcial (por glob) | regra gerada (hoje `.cursor` está no `.gitignore` — reavaliar) |

⚠️ **Compilação lossy:** só o Claude tem disclosure. Para os outros, a skill vira instrução estática
*sempre no contexto*. Por isso o manifesto deve permitir ativar **poucas skills por Comb por agente** —
não despejar todas em todo `AGENTS.md`.

### 3.3 Mapeamento campo a campo (modal do Zed → DCC) — e onde o DCC vai **além** do Zed

| Campo no Zed | Equivalente no DCC |
|---|---|
| **Name** / **Description** / **Content** | frontmatter (`name`/`description`) + corpo do `SKILL.md` na fonte neutra |
| **Disable model invocation** ("esconder do catálogo, ainda invocável por slash") | flag no manifesto. O DCC **já tem slash-commands no composer** (`features/composer/default-slash-commands.ts` + `slash-command-plugin.tsx`), então pode expor a skill como `/comando` no próprio composer mesmo fora do auto-catálogo. ⚠️ Validar por agente se há como tirar do auto-discovery (Claude: via o que se compila; outros: simplesmente não escrever no `AGENTS.md`) |
| **Scope** ("só neste projeto · salvo em `.agents/skills/<name>/skill.md`") | **projeto** → fonte em `.devcommandcenter/skills/` (versionada), compilada nos alvos do worktree; **global** → injetada no HOME-sombra por agente via `shadowHomePath`, sem tocar o repo |
| — *(não existe no Zed, single-agent)* | ➕ **Agentes-alvo**: seletor de para quais agentes esta skill compila (Claude / Codex / Gemini / Cursor / …). **É aqui que o DCC supera o Zed** e justifica a feature |

### 3.4 Recomendação de escopo deste ponto

O **design e a UX são provider-neutros desde o dia 1** (fonte única + seletor de agentes-alvo) — isso
é barato e é o que dá sentido à feature dado o uso multi-agente real. O que se faz **incrementalmente**
são os *alvos de compilação*, do mais barato ao mais caro:

- **Fase 0 (pré-req):** teste de fumaça garantindo que o Claude mantém `settingSources` com `"project"`.
- **Fase 1:** modal estilo Zed (nome/desc/conteúdo/escopo/**agentes-alvo**) gravando na fonte neutra
  `.devcommandcenter/skills/`; primeiro alvo = `.claude/skills/` (cópia fiel, custo zero) para validar
  o pipeline ponta a ponta.
- **Fase 2:** alvo `AGENTS.md` (cobre Codex/Droid/futuros) com marcadores idempotentes.
- **Fase 3:** `GEMINI.md` e, se houver demanda, `.cursor/rules/`.
- **Fase 4:** skills globais via `shadowHomePath`.

⚠️ **Honestidade (análise §7):** manter transpilers para formatos em evolução rápida é manutenção
contínua, e se a indústria padronizar um formato de *skill* portável (não só de instrução) parte disto
vira redundante. Por isso: camada **fina**, alvos incrementais, sem construir o transpiler completo de
antemão.

---

## Fora de escopo

Avaliado e **descartado** por diluir o foco vertical do DCC (vira "mais um canivete suíço"):

- ❌ **Cliente REST** (estilo Postman) — produto inteiro, fora do eixo worktree.
- ❌ **Clientes SQL / NoSQL** (PostgreSQL, MySQL, Mongo, Redis…) — idem.
- ❌ **Explorer de storage** (S3/Azure/SFTP/FTP) — idem.
- ❌ **Terminal preso ao worktree como recurso principal** — redundante com "abrir IDE no worktree".
- ⚠️ **SSH / terminal remoto** — só faria sentido reenquadrado como "worktree remoto / dev em máquina
  remota". Não agora.

---

## Checklist

- [x] Generalizar `terminal-store.ts` para chave por `terminalId` (owner `terminal:${terminalId}`, cwd `rootPath`)
- [x] `terminal-tabs-store.ts`: abas + `activeId` por projeto, persistência `localStorage`, hook `useProjectTerminals`
- [x] Tab strip no `WorkspaceTerminalDrawer` (`+` / `x` / trocar de aba)
- [x] `terminal-panel.tsx` recebe `terminalId`/`cwd` e renderiza só a aba ativa (`key={terminalId}` → buffer replicado pelo store)
- [x] Botão Terminal na toolbar do composer abre o drawer (destravou o `terminalDrawerOpen`, que antes nunca abria)
- [x] `App.tsx`/`session-workbench.tsx` passam `projectId` + `rootPath` (cwd fora do worktree)
- [x] Limite suave de abas (`MAX_TERMINAL_TABS = 8`); `removeTerminal` mata o PTY da aba fechada
- [x] Matar todos os PTYs ao sair do app (`RunEvent::ExitRequested` → `kill_all_terminals` no `main.rs`). **Trocar de projeto NÃO mata** os PTYs (mantém vivos em background, conforme recomendado — preserva processos ao voltar)
- [x] (Skills) Design provider-neutro: fonte única `.devcommandcenter/skills/<name>/SKILL.md` + manifesto `skills.json` (agentes-alvo, escopo, disable-model-invocation) — `src-tauri/src/skills_commands.rs`
- [x] (Skills Fase 0) Teste de fumaça: `settingSources` mantém `"project"` (`setting-sources.smoke.test.ts`)
- [x] (Skills Fase 1) Diálogo próprio (command palette + sidebar) + modal com nome/desc/conteúdo/**agentes-alvo**/disable-model-invocation; alvo `.claude/skills/` (cópia fiel) com tracking `.dcc-managed.json`
- [x] (Skills Fase 1) *disable model invocation* implementado: exclui do `AGENTS.md` (always-on); UI explica que segue invocável por slash. ⚠️ knob nativo do Agent SDK p/ Claude ainda a validar
- [x] (Skills Fase 2) Compilar para `AGENTS.md` com marcadores idempotentes `<!-- dcc:skills:start/end -->` (testado: insere/substitui/remove preservando conteúdo escrito à mão)
- [x] (Skills Fase 3) Compilar para `GEMINI.md` (bloco idempotente, igual AGENTS.md) e `.cursor/rules/<name>.mdc` (1 arquivo por skill, com tracking `.dcc-managed.json`). ⚠️ `.cursor` está no `.gitignore` — o artefato é escrito no worktree ativo e funciona localmente, mas não é versionado/compartilhado
- [ ] (Skills Fase 4) Skills globais via `shadowHomePath`, fora do repo
- [x] (Skills — follow-up) Recompilar por worktree: efeito em `App.tsx` recompila ao selecionar workspace (worktrees novos recebem skills sem reeditar). Writes idempotentes (`write_if_changed`) evitam churn. Backend local apenas. _(Resolveu melhor que o hook bash, que não chama o compilador Rust.)_
- [x] MCP-server (`dcc mcp`, stdio JSON-RPC em `src-tauri/src/bin/dcc.rs`) já existia com 8 tools (daemon/combs/panes/diffs). **Adicionadas 5 tools de status de agentes/processos**: `daemon_health`, `processes_list`, `process_start/stop/restart` (mapeiam métodos do daemon que existiam mas não eram expostos). Smoke test stdio: initialize + tools/list → 13 tools ✅
- [ ] (MCP follow-up) Tool dedicada de "atenção" (quais agentes terminaram / precisam de interação) — hoje inferível via `panes_list`/`processes_list`; falta um endpoint que materialize a heurística de atenção
- [ ] (Skills follow-up) Fase 4 global Claude-only via `shadowHomePath` (adiada por decisão)
