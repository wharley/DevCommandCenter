# DCC Code — superfície de código leve, agent-first

> Contexto de desenvolvimento. Atualize a cada fase concluída.

## Tese

O DCC **não** vira IDE. "DCC Code" é só **mais uma porta de entrada** para o pipeline
agent-first que já existe (anotar trecho → enviar ao agente / editar no composer /
adicionar à revisão). O eixo do produto continua worktree-first / mission-based
(ver `memory/product-focus.md`). Toda peça é avaliada por: _reforça o eixo
worktree→agente, ou começa a competir com um editor de propósito geral?_

Regra de ouro de performance: **tudo sob demanda**. Monaco só carrega quando o
usuário entra na superfície (`import()` já lazy em `lib/monaco-runtime.ts`). Busca só
roda quando pedida. Nada de índice/watcher em background.

## O que já existia antes deste plano

- `lib/monaco-runtime.ts` → runtime Monaco lazy, com:
  - `createDiffEditor` (read-only) com affordance "enviar ao agente",
    machine-annotations do CodeRabbit, temas dcc-dark/light.
  - `createFileEditor` — controller de **edição** completo (getValue/setValue/
    switchFile/onDidChangeModelContent). **Estava órfão** (ninguém montava).
- `features/editor/WorkspaceEditorSurface.tsx` → superfície de **diff** read-only +
  `DiffAnnotationPopover` (o popover é genérico, não é específico de diff).
- União de superfícies em `features/panel/workspace-surface.ts`:
  `kind: "git-diff" | "mission-spec"`, montada em `WorkspacePanel.tsx`.
- Backend (zero Rust novo necessário p/ Quick Open / leitura):
  - `list_git_tracked_files` (Quick Open) — já exposto via `listGitTrackedFiles`.
  - `workspace_git_file_preview_content` — devolve `modified_text` = **arquivo
    inteiro** da working tree (helper `read_worktree_file_text`).
  - `fs::write` confinado à raiz do worktree já é padrão (comandos de mission spec).

## Decisões fechadas

- **Reusar 100% o diff.** O editor interno é mais uma _origem_ ("arquivo alterado no
  DCC"), nunca um diff paralelo.
- **Save = reconciliar antes de gravar.** Ao abrir, guardamos um hash do conteúdo do
  disco. No salvar, re-lemos o disco; se divergiu (ex.: um agente mexeu no arquivo,
  CWD é compartilhado), abrimos o **mesmo** `createDiffEditor` com
  `original = disco atual` / `modified = sua edição` e os botões
  `[Sobrescrever] [Mesclar] [Cancelar]`. A reconciliação **não é código de diff novo**.

## Fases

### Fase 1 — superfície de arquivo read-only + select→agente  ✅ (este passo)

Objetivo: abrir o **arquivo inteiro** (não só os hunks do diff) e selecionar qualquer
linha — inclusive contexto que o diff colapsa via `hideUnchangedRegions` — para
enviar ao agente. Reusa header, popover e handlers de anotação existentes.

Entrada da Fase 1 (baixo risco, zero plumbing até o App): **toggle local** no
`WorkspacePanel` entre `Diff` e `Arquivo inteiro`, sobre a _mesma_ seleção de arquivo.
O `WorkspaceFileSurface` lê via `useWorkspaceGitFilePreviewContent` (mesmo hook do
diff), usando `modifiedText || originalText` como corpo. Read-only por enquanto.

Arquivos:
- `lib/monaco-runtime.ts` — `createFileEditor` ganha `readOnly` + affordance de
  annotate (reusa `attachAnnotateButton`, que já opera sobre um editor único).
- `features/editor/diff-annotation.tsx` — **novo**; extrai `DiffAnnotationPopover` +
  tipos (`DiffAnnotationRequest`/`DiffAnnotationSubmit`/`PendingAnnotation`) para
  compartilhar entre as duas superfícies. `WorkspaceEditorSurface` re-exporta os
  tipos p/ compatibilidade.
- `features/editor/file-view-toggle.tsx` — **novo**; segmented control `Diff |
  Arquivo inteiro`, usado nos dois headers.
- `features/editor/WorkspaceFileSurface.tsx` — **novo**; superfície read-only.
- `features/editor/WorkspaceEditorSurface.tsx` — header ganha o toggle (via
  `onOpenWholeFile?`), nada mais muda no fluxo de diff.
- `features/panel/WorkspacePanel.tsx` — estado local `wholeFileView` + render
  condicional; reusa os mesmos handlers de anotação.
- i18n: bloco `fileSurface` em `pt-BR` e `en`.

Não toca: `createDiffEditor`, fluxo CodeRabbit, handlers de anotação, união de
superfícies (App permanece intacto).

### Fase 2 — Quick Open (Cmd/Ctrl+P)  ✅

Entry para abrir arquivo **qualquer** (não só os modificados), zero Rust novo (sem
rebuild/codegen — respeita "não quebrar o que já existe").

- `features/editor/file-quick-open.tsx` — `CommandDialog` (cmdk) com filtro fuzzy
  próprio (subsequência, prioriza basename), **limitado a 60 resultados** p/ não
  montar milhares de linhas. `shouldFilter={false}` repassado pelo `CommandDialog`
  (extensão aditiva em `components/ui/command.tsx`) p/ preservar a ordenação própria.
- `workspace-surface.ts` — variante `kind: "file-edit"` (path + name + focusLine),
  driven pelo App (`handleOpenFileFromQuickOpen`).
- `WorkspaceFileSurface` — agora recebe um `source` discriminado
  (`{kind:"git"|"path"}`); o toggle Diff só aparece no `git`.
- `features/editor/use-workspace-file-content.ts` — lê o corpo via o comando Rust
  dedicado `read_workspace_file` (sem stopgap).
- Atalho `isQuickOpenShortcut` (Cmd/Ctrl+P) em `shortcut-utils.ts` (+ teste). Vence o
  guard de foco em input (chord com modificador não é texto).

**Comando Rust `read_workspace_file`** (`crates/dcc-tauri/.../workspace_commands.rs`):
reusa `read_worktree_file_text` + `validate_git_relative_path` (confinado à raiz, sem
`..`). Registrado em `src-tauri/src/workspace_commands.rs` (wrapper), `main.rs`
(`use` + `generate_handler!`) e `build.rs` (`.typ` + `WorkspaceMethods`). Bindings
regeneram no build (`tauri_specta` em build.rs). Front: `readWorkspaceFile` em
`workspace-api.ts`.

### Fase 3 — edição + save (reconciliar)  ✅

Edição habilitada **só na origem `path`** (Quick Open); a origem `git` (toggle de
review) segue read-only. `editable` é passado pelo `WorkspacePanel`.

- `createFileEditor` ganhou modo editável; `WorkspaceFileEditor` virou `forwardRef`
  expondo `getValue`/`setValue` + `onChange`. Em modo edição o buffer é do usuário —
  o effect de sync de conteúdo só roda em read-only (não clobbera edição).
- Dirty-indicator (ponto) no header + botão **Salvar** + atalho **Cmd/Ctrl+S** (vence o
  guard de foco em input). Fechar com pendências pede confirmação.
- Comando Rust **`write_workspace_file`** (confinado à raiz, mesmo registro do `read`).
  Front: `writeWorkspaceFile`.
- **Reconciliação (a decisão fechada):** ao salvar, re-lê o disco; se divergiu do
  baseline (disco no open/último save) → abre `ReconcileDialog` com o **diff disco↔sua
  edição** (reusa `createDiffEditor`) e força escolha:
  - **Sobrescrever com a minha** → grava por cima;
  - **Recarregar do disco** → carrega a versão do agente no editor (descarta a minha);
  - **Cancelar** → mantém editando.
  Substituí o "Mesclar" (auto-merge 3-way, pesado) por "Recarregar do disco" — opção
  segura e clara. Auto-merge fica como possível Fase 4+.
- Limitação conhecida: a reconciliação é client-side (read→compare→write); há uma
  janela TOCTOU mínima. Endurecer = `write_workspace_file` receber um hash esperado e
  fazer compare-and-swap no Rust.

### Fase 4 — busca no workspace  ✅ (parte 1)

Sem índice em background — roda sob demanda. **`git grep`** (não ripgrep: sem
dependência/sidecar, escopo do worktree, consistente com o Quick Open) via comando
Rust `search_workspace` (fixed string, case-insensitive, `-z` p/ paths com `:`, cap
de 200, `truncated` flag). Front: `searchWorkspace`.

- `features/editor/workspace-search.tsx` — dialog `Cmd/Ctrl+Shift+F` (cmdk,
  `shouldFilter={false}`), input debounced (`useDeferredValue`, mín. 2 chars),
  resultados agrupados por arquivo, match destacado, abre no surface no `focusLine`.
- Atalho `isWorkspaceSearchShortcut` (+ teste). App: estado + keydown + mount +
  `handleOpenSearchMatch` (abre `file-edit` com `focusLine` = linha do match).

**Context basket** ✅ — entregue por reuso: é o `DiffReviewTray` + buffer de review
(`onAddToReview`/`reviewAnnotations` no `WorkspacePanel`), já fiado no file surface.
Coleta trechos com path+linha de vários arquivos, sobrevive à navegação e envia tudo
num turno só. Não foi duplicado (fiel à tese de não forkar o pipeline).

**TOCTOU hardening do save** ✅ — `write_workspace_file` virou compare-and-swap:
recebe `expected_previous` e, no Rust, re-lê+compara+escreve no mesmo comando; se o
disco divergiu, devolve `conflicted` + `disk_content` (nada é escrito). O front salva
em um round-trip (sem read+write separados) e abre a reconciliação no `conflicted`.
"Sobrescrever" força com `expected_previous = null`. Janela TOCTOU residual mínima
(sem lock de arquivo — `fs2` seria overkill).

**Decisões de escopo (não fazer):** ripgrep sidecar (peso/risco de build > ganho sobre
`git grep`) e árvore do projeto (scope creep horizontal; Quick Open cobre navegação).

### Fase 4 — abas + preview-tab  ✅

- `WorkspaceFileSurface` virou `forwardRef` com modo **`embedded`** (sem header
  próprio, sem keydown próprio); expõe handle `{ save, reveal }` e reporta
  `{dirty, saving}` via `onStateChange`. O caminho standalone (toggle git) segue
  igual (props novas todas opcionais — zero regressão).
- `features/editor/file-tabs-surface.tsx` — wrapper dono de `openFiles[]` +
  `activePath`. **Keep-alive**: renderiza todos os surfaces embedded, só o ativo
  visível (`display:none` nos outros) → preserva edição não salva por aba. Header
  compartilhado (traffic lights + tab strip + save/close do ativo). Um único keydown
  global (Cmd/Ctrl+S salva o ativo; Esc fecha tudo c/ confirm se houver pendência).
- Preview-tab: abrir via Quick Open/busca = preview (itálico), substituído pelo
  próximo preview; **editar fixa** (pin) e **duplo-clique fixa**. Dirty-dot por aba.
- `reveal()` (layout+focus) no `requestAnimationFrame` ao trocar de aba — corrige o
  Monaco renderizar a 0px enquanto escondido no WKWebView.
- `WorkspacePanel` usa `FileTabsSurface` no `file-edit`; o toggle git segue no
  `WorkspaceFileSurface`. Fechar aba com pendência confirma (reusa o guard).

### Fora de escopo (sirenes de IDE)

LSP, language server sempre ligado, indexação global, find references real, debugger,
extensões, file tree completa, go-to-import, breadcrumbs, outline do projeto. Reavaliar
só com uso real na mão.
