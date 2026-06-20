# CodeRabbit no DCC - contrato backend

Status: backend Rust implementado para health/check, fingerprint de diff, review sincrono legado, review em background com polling/cancelamento, eventos Tauri por JSONL e persistencia SQLite do ultimo review + historico por workspace. Frontend implementado para auth, review no Inspector, stale detection, selecao de findings, historico, destaque inline e handoff para o Composer.

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
  - Selecao de findings e envio do prompt estruturado para o Composer.
- `apps/desktop/src/features/inspector/use-workspace-coderabbit-review.ts`
  - Persistencia do ultimo review por workspace via SQLite, com migracao/fallback de `localStorage`.

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

### `workspaceCoderabbitReviewStart`

Metodo: `workspace_coderabbit_review_start`

Input: `WorkspaceCodeRabbitReviewInput`

Comportamento:

- Valida workspace e cria um job em memoria.
- Retorna imediatamente `{ jobId, status, startedAt }`.
- O job roda `cr review --agent` fora do fluxo de UI e atualiza o snapshot quando termina.
- O job le stdout em stream e emite eventos Tauri enquanto recebe JSONL do CLI.
- O comando sincrono `workspaceCoderabbitReview` continua existindo para compatibilidade, mas a UI do Inspector usa o fluxo de job.

Output: `WorkspaceCodeRabbitReviewStartOutput`

```ts
{
  jobId: string;
  status: "starting" | "running" | "succeeded" | "failed" | "canceled";
  startedAt: string;
}
```

### `workspaceCoderabbitReviewJob`

Metodo: `workspace_coderabbit_review_job`

Input:

```ts
{ jobId: string }
```

Output: `WorkspaceCodeRabbitReviewJobSnapshot`

Campos principais:

- `status`
- `pid`
- `startedAt`
- `updatedAt`
- `completedAt`
- `cancelRequested`
- `message`
- `result: WorkspaceCodeRabbitReviewOutput | null`
- `errors`

Uso esperado:

- Polling no frontend enquanto `status` for `starting | running`.
- Eventos Tauri atualizam a mensagem e aceleram refetch; polling continua como fallback.
- Quando `status=succeeded`, persistir `result`.
- Quando `status=failed`, mostrar `errors[0]` e persistir `result` se existir.
- Quando `status=canceled`, encerrar estado de progresso sem substituir o ultimo review salvo.

### `workspaceCoderabbitReviewCancel`

Metodo: `workspace_coderabbit_review_cancel`

Input:

```ts
{ jobId: string }
```

Comportamento:

- Marca `cancelRequested=true`.
- Mata o process group do CLI quando o PID ja existe.
- O job finaliza como `canceled` quando o processo retorna.
- Retorna o snapshot atual do job.

## Evento Tauri

Nome: `dcc/coderabbit/review/event`

Tipo: `CodeRabbitReviewStreamEvent`

```ts
{
  jobId: string;
  workspaceRoot: string;
  eventType: string;
  status: string | null;
  message: string | null;
  finding: CodeRabbitFinding | null;
  complete: CodeRabbitReviewComplete | null;
  result: WorkspaceCodeRabbitReviewOutput | null;
  errors: string[];
}
```

Eventos emitidos:

- `running`: job iniciou o processo do CLI.
- `status`: status incremental vindo do `cr review --agent`.
- `finding`: finding incremental vindo do JSONL.
- `complete`: evento complete do CodeRabbit.
- `succeeded | failed | canceled`: estado terminal com `result` quando disponivel.

Uso no frontend:

- Escutar via `listenCodeRabbitReviewEvents`.
- Filtrar por `jobId`.
- Atualizar mensagem visivel com `message/status`.
- Refazer `workspaceCoderabbitReviewJob` quando chegar `finding`, `complete` ou estado terminal.

### `workspaceCoderabbitReviewLoad`

Metodo: `workspace_coderabbit_review_load`

Input:

```ts
{ workspaceRoot: string }
```

Output: `WorkspaceCodeRabbitStoredReviewOutput`

```ts
{
  workspaceRoot: string;
  review: WorkspaceCodeRabbitReviewOutput | null;
  updatedAt: string | null;
}
```

Comportamento:

- Le o ultimo review persistido em `workspace_coderabbit_reviews`.
- A chave primaria e `workspace_root`.
- Retorna `review=null` quando ainda nao ha review salvo.

### `workspaceCoderabbitReviewSave`

Metodo: `workspace_coderabbit_review_save`

Input:

```ts
{
  workspaceRoot: string;
  review: WorkspaceCodeRabbitReviewOutput;
}
```

Comportamento:

- Persiste o review serializado como JSON.
- Salva tambem `fingerprint_hash` e `completed_at` para indexacao/diagnostico.
- Faz upsert por `workspace_root`.
- Insere uma linha em `workspace_coderabbit_review_history` para manter historico.

### `workspaceCoderabbitReviewHistory`

Metodo: `workspace_coderabbit_review_history`

Input:

```ts
{
  workspaceRoot: string;
  limit: number | null;
}
```

Output: `WorkspaceCodeRabbitReviewHistoryOutput`

```ts
{
  workspaceRoot: string;
  entries: Array<{
    reviewId: string;
    workspaceRoot: string;
    review: WorkspaceCodeRabbitReviewOutput;
    reviewType: string | null;
    success: boolean;
    findingsCount: number;
    fingerprintHash: string | null;
    completedAt: string | null;
    savedAt: string;
  }>;
}
```

Comportamento:

- Lista os reviews historicos do workspace ordenados por `saved_at DESC`.
- `limit` e clampado entre 1 e 100; default 20.
- A UI atual consulta ate 12 entradas e permite reabrir um snapshot antigo sem duplicar o historico.

### `workspaceCoderabbitReviewClear`

Metodo: `workspace_coderabbit_review_clear`

Input:

```ts
{ workspaceRoot: string }
```

Comportamento:

- Remove o ultimo review salvo daquele workspace.

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
4. Rodar `workspaceCoderabbitReviewStart` em acao explicita do usuario.
5. Fazer polling com `workspaceCoderabbitReviewJob` enquanto o status for `starting | running`.
6. Permitir cancelamento via `workspaceCoderabbitReviewCancel`.
7. Persistir o `result` por workspace com `workspaceCoderabbitReviewSave`.
8. Recalcular `workspaceCoderabbitDiffFingerprint` quando `WORKSPACE_GIT_STATUS_QUERY_KEY` ou branch diff invalidar.
9. Se `combinedHash` divergir, marcar UI como stale: "O diff mudou desde este review".
10. Renderizar findings agrupados por severidade.
11. Clique em finding seleciona arquivo/linha no diff preview.
12. Handoff implementado: findings selecionados geram um prompt estruturado e entram no Composer via `prefill`.
13. Handoff futuro opcional: findings podem evoluir para `DiffAnnotation { source: "coderabbit" }` quando o contrato incluir snippet/side suficiente para reusar o tray humano sem perder contexto.

## Fluxo implementado no Inspector

- O bloco `CodeRabbit review` aparece abaixo de staged/unstaged/branch diff.
- O usuario escolhe escopo `all | uncommitted | committed`.
- Se o CLI nao estiver autenticado, o bloco mostra a mensagem de status e abre `CodeRabbitConnectDialog`.
- `Run review` chama `workspaceCoderabbitReviewStart`.
- Enquanto o job roda, a UI faz polling de `workspaceCoderabbitReviewJob`.
- O usuario pode cancelar com `workspaceCoderabbitReviewCancel`.
- O resultado e persistido em SQLite por `workspaceRoot`.
- O hook migra automaticamente um review legado de `localStorage` quando o backend ainda nao tem registro.
- O bloco recalcula `workspaceCoderabbitDiffFingerprint` e marca `Diff changed` quando o `combinedHash` diverge.
- Findings sao agrupados por severidade.
- Clique no finding seleciona o arquivo no diff preview:
  - se o arquivo esta em staged, abre preview staged;
  - se esta em unstaged, abre preview unstaged;
  - caso contrario, abre preview committed contra a base.
- Checkbox no finding seleciona itens para acao.
- `Selecionar tudo`, `Limpar` e `Enviar ao Composer` ficam no topo da lista de findings.
- `Enviar ao Composer` injeta um prompt contendo escopo, horario do review, aviso de stale quando aplicavel, severidade, arquivo/linha, comentario, instrucoes de codegen e sugestoes do CodeRabbit.
- Ao clicar em um finding, a selecao enviada ao editor inclui `focusLine` e `machineAnnotations`.
- O Monaco diff renderiza as linhas apontadas pelo CodeRabbit com destaque por severidade e hover `CodeRabbit`.

## Fora do escopo desta fase

- Login/auth browser flow no backend.
- Comparacao visual entre dois reviews historicos.
- Aplicacao automatica de fixes.
- persistencia do ultimo resultado por workspace

O comando `workspaceCoderabbitReview` atual ja fornece o contrato de dados e o parser; a camada de job pode reutilizar essa base.
