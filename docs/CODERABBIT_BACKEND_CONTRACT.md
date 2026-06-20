# CodeRabbit no DCC - contrato backend

Status: backend Rust inicial implementado para health/check, fingerprint de diff e review via JSON Lines.

Atualizacao frontend:

- `apps/desktop/src/lib/coderabbit-cli.ts`
  - Fallback desktop/browser e comando de login.
- `apps/desktop/src/features/settings/coderabbit-cli-queries.ts`
  - Query key e hook de status/auth.
- `apps/desktop/src/features/settings/coderabbit-connect-dialog.tsx`
  - Dialog com PTY embutido executando `cr auth login`.
- `apps/desktop/src/features/settings/SettingsDialog.tsx`
  - Card `CodeRabbit CLI` em Settings > Account.
- `apps/desktop/src/features/inspector/workspace-inspector-sidebar.tsx`
  - Card contextual `CodeRabbit` no Inspector > Context.
- `apps/desktop/src/features/inspector/coderabbit-review-section.tsx`
  - Bloco `CodeRabbit review` dentro do Inspector > Changes.
- `apps/desktop/src/features/inspector/use-workspace-coderabbit-review.ts`
  - Persistencia local do ultimo review por workspace.

Fontes oficiais usadas:

- https://docs.coderabbit.ai/cli
- https://docs.coderabbit.ai/cli/reference

## Objetivo

CodeRabbit entra no DCC como camada de review derivada do estado Git, nao como chat e nao como aplicador automatico de fixes. O backend expoe comandos Tauri estruturados; o frontend decide UX, auth/login e handoff para Composer.

## Modulos

- `crates/dcc-tauri/src/commands/coderabbit/mod.rs`
  - Tipos Specta, comandos Tauri e montagem de argumentos do CLI.
- `crates/dcc-tauri/src/commands/coderabbit/process.rs`
  - Deteccao de `cr`/`coderabbit` e execucao com timeout/process kill.
- `crates/dcc-tauri/src/commands/coderabbit/parser.rs`
  - Parser de `cr review --agent` como JSON Lines.
- `crates/dcc-tauri/src/commands/coderabbit/fingerprint.rs`
  - Fingerprint Git para stale detection.
- `src-tauri/src/coderabbit_commands.rs`
  - Wrapper fino do app Tauri.
- `packages/contracts/src/generated/bindings.ts`
  - Contrato TypeScript gerado.

## Metodos Tauri

Todos estao em `WORKSPACE_METHODS`.

### `workspaceCoderabbitCliStatus`

Metodo: `workspace_coderabbit_cli_status`

Input: `WorkspaceCodeRabbitCliStatusInput`

```ts
{
  workspaceRoot: string | null;
  cliPath: string | null;
  includeAuthStatus: boolean | null;
}
```

Comportamento:

- Detecta `cr` primeiro e depois `coderabbit`.
- Se `cliPath` vier preenchido, usa esse binario explicitamente.
- Se `includeAuthStatus` for `true`, roda `cr auth status --agent`.
- Nao faz login e nao abre browser. O frontend deve orientar `cr auth login`, seguindo o padrao do GitHub/GitLab CLI.

Output: `WorkspaceCodeRabbitCliStatusOutput`

Campos principais:

- `installed`: CLI encontrado.
- `status`: `"ready" | "unavailable" | "error"`.
- `version`: saida de `--version`.
- `loginCommand`: sempre `"cr auth login"`.
- `auth`: resultado opcional de `auth status --agent`.

### `workspaceCoderabbitDoctor`

Metodo: `workspace_coderabbit_doctor`

Input: `WorkspaceCodeRabbitDoctorInput`

```ts
{
  workspaceRoot: string;
  cliPath: string | null;
  timeoutSeconds: number | null;
}
```

Comportamento:

- Roda `cr doctor` no workspace.
- Retorna stdout/stderr e exit code.
- Timeout default: 120 segundos.

Uso esperado:

- Botao/acao de diagnostico quando status/auth/review falhar.
- Mostrar stdout/stderr compactado no Inspector ou Settings.

### `workspaceCoderabbitDiffFingerprint`

Metodo: `workspace_coderabbit_diff_fingerprint`

Input: `WorkspaceCodeRabbitFingerprintInput`

```ts
{
  workspaceRoot: string;
  reviewType: "all" | "committed" | "uncommitted" | null;
  base: string | null;
  baseCommit: string | null;
}
```

Output: `CodeRabbitDiffFingerprint`

Campos principais:

- `reviewType`
- `head`
- `currentBranch`
- `baseRef`
- `mergeBase`
- `stagedDiffHash`
- `unstagedDiffHash`
- `untrackedFilesHash`
- `committedDiffHash`
- `combinedHash`
- `generatedAt`

Uso esperado:

- Salvar `fingerprint.combinedHash` junto ao ultimo resultado do review.
- Recalcular quando Git status/branch diff invalidar.
- Marcar o review como stale quando o hash atual divergir do hash salvo.

### `workspaceCoderabbitReview`

Metodo: `workspace_coderabbit_review`

Input: `WorkspaceCodeRabbitReviewInput`

```ts
{
  workspaceRoot: string;
  cliPath: string | null;
  reviewType: "all" | "committed" | "uncommitted" | null;
  base: string | null;
  baseCommit: string | null;
  light: boolean | null;
  configPaths: string[];
  timeoutSeconds: number | null;
}
```

Comando gerado:

```sh
cr review --agent --dir <workspaceRoot> --type <all|committed|uncommitted>
```

Com opcoes fechadas:

- `--light`
- `--base <base>`
- `--base-commit <baseCommit>`
- `--config <relativePath>`

Observacoes:

- `configPaths` deve ser relativo ao workspace. Caminhos absolutos e `..` sao rejeitados.
- Nao ha argumento livre vindo do frontend.
- Timeout default: 40 minutos.
- O comando retorna `Ok(output)` mesmo se o CLI sair com exit code != 0; nesse caso `success=false` e `errors` contem o detalhe.

Output: `WorkspaceCodeRabbitReviewOutput`

Campos principais:

- `success`
- `exitCode`
- `reviewType`
- `fingerprint`
- `findings`
- `statuses`
- `complete`
- `errors`
- `eventCount`
- `stdout`
- `stderr`
- `startedAt`
- `completedAt`

## Finding normalizado

Tipo: `CodeRabbitFinding`

```ts
{
  id: string;
  severity: "critical" | "major" | "minor" | "trivial" | "info" | "unknown";
  severityRaw: string;
  path: string;
  startLine: number | null;
  endLine: number | null;
  side: string | null;
  comment: string | null;
  codegenInstructions: string | null;
  suggestions: string[];
}
```

Mapeamento do parser:

- `fileName` -> `path`
- `severity` -> `severity` + `severityRaw`
- `codegenInstructions` -> instrucao primaria para handoff ao agente
- `comment` -> fallback humano
- `suggestions` -> lista de sugestoes serializadas
- `startLine`/`line`/`lineNumber` -> `startLine`
- `endLine` -> `endLine`

## Fluxo frontend recomendado

1. Chamar `workspaceCoderabbitCliStatus({ workspaceRoot, includeAuthStatus: true })`, via `useCodeRabbitCliStatus`.
2. Se `installed=false`, mostrar instrucao de instalacao.
3. Se auth falhar, abrir `CodeRabbitConnectDialog`, que executa `cr auth login` em PTY embutido e reconsulta o status no fechamento.
4. Antes de review, chamar ou confiar no fingerprint retornado por `workspaceCoderabbitReview`.
5. Rodar `workspaceCoderabbitReview` em acao explicita do usuario.
6. Persistir resultado por workspace com `fingerprint.combinedHash`.
7. Recalcular `workspaceCoderabbitDiffFingerprint` quando `WORKSPACE_GIT_STATUS_QUERY_KEY` ou branch diff invalidar.
8. Se `combinedHash` divergir, marcar UI como stale: "O diff mudou desde este review".
9. Renderizar findings agrupados por severidade.
10. Clique em finding seleciona arquivo/linha no diff preview.
11. Handoff posterior: findings selecionados viram `DiffAnnotation { source: "coderabbit" }` e podem ir ao tray/Composer.

## Fluxo implementado no Inspector

- O bloco `CodeRabbit review` aparece abaixo de staged/unstaged/branch diff.
- O usuario escolhe escopo `all | uncommitted | committed`.
- Se o CLI nao estiver autenticado, o bloco mostra a mensagem de status e abre `CodeRabbitConnectDialog`.
- `Run review` chama `workspaceCoderabbitReview`.
- O resultado e persistido em `localStorage` por `workspaceRoot`.
- O bloco recalcula `workspaceCoderabbitDiffFingerprint` e marca `Diff changed` quando o `combinedHash` diverge.
- Findings sao agrupados por severidade.
- Clique no finding seleciona o arquivo no diff preview:
  - se o arquivo esta em staged, abre preview staged;
  - se esta em unstaged, abre preview unstaged;
  - caso contrario, abre preview committed contra a base.

## Fora do escopo desta fase

- Login/auth browser flow no backend.
- Persistencia SQLite de reviews.
- Job queue com progresso/cancelamento/eventos.
- Aplicacao automatica de fixes.
- UI do Inspector.

## Proxima camada backend recomendada

Para reviews realmente longos, adicionar uma camada de job:

- `workspace_coderabbit_review_start(...) -> { jobId }`
- `workspace_coderabbit_review_status(jobId) -> progress/result`
- `workspace_coderabbit_review_cancel(jobId)`
- eventos Tauri para `status`, `finding`, `complete`, `error`
- persistencia do ultimo resultado por workspace

O comando `workspaceCoderabbitReview` atual ja fornece o contrato de dados e o parser; a camada de job pode reutilizar essa base.
