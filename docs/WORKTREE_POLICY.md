# Worktree Policy

Política de governança para worktrees Git criados pelo Dev Command Center. Define regras para evitar bagunça no disco e garantir uso seguro.

## 1. Nome padrão

Worktrees criados pelo DCC devem seguir o padrão:

```
dcc-{identificador}-{timestamp}
```

- **identificador**: `{missionId}` ou `{branch}-{shortHash}` (ex: `main-a1b2c3d`, `feature-xyz`)
- **timestamp**: `YYYYMMDD-HHmmss` (ex: `20250130-143022`)

Exemplos:
- `dcc-mission-abc123-20250130-143022`
- `dcc-main-a1b2c3d-20250130-143022`

Objetivo: evitar colisões e permitir identificação rápida da origem.

## 2. Limpeza automática

- Worktrees DCC com mais de **7 dias** sem uso podem ser candidatos à remoção.
- Antes de remover: **confirmação do usuário** ou configuração de auto-cleanup.
- Listar worktrees antigos: `git worktree list` + verificar data de último acesso (via `worktree/<path>/gitdir` ou metadados próprios).
- Comando sugerido: `git worktree prune` remove referências a worktrees já deletados; a remoção real do diretório é responsabilidade do app.

## 3. Listagem e reaproveitamento

- Expor `git worktree list --porcelain` via API para a UI.
- Permitir: listar, abrir no Explorer/Finder, remover worktree.
- Reaproveitar: ao criar nova missão, verificar se já existe worktree para o branch desejado; oferecer reutilizar em vez de criar novo.

## 4. Lock para missão em execução

- Enquanto uma missão está em estado ativo (`planning`, `plan_generated`, `generating_code`, `code_ready`, `applying`), o worktree associado fica **locked**.
- Opções de implementação:
  - Arquivo `.dcc-worktree-lock` no root do worktree com `{ missionId, lockedAt }`.
  - Ou flag em banco local (missions.worktreePath + status).
- Ao tentar remover worktree locked: bloquear e avisar o usuário.
- Ao completar/falhar/cancelar missão: remover lock.

## 5. Localização no disco

- Padrão recomendado e adotado: worktrees em pasta global do app (fora do repositório), por exemplo:
  - macOS: `~/Library/Application Support/dev-command-center/worktrees/{repoHash}/{branch}`
  - Linux: `~/.local/share/dev-command-center/worktrees/{repoHash}/{branch}`
  - Windows: `%APPDATA%/dev-command-center/worktrees/{repoHash}/{branch}`
- Objetivo: evitar poluição do projeto e reduzir risco de confusão em commit/push.
- Se houver modo avançado local no futuro, manter como opt-in e nunca como padrão.

## 6. Referência de implementação futura

Quando implementar criação de worktrees:

1. Usar constantes de [worktree-policy.ts](../electron/services/worktree-policy.ts).
2. Seguir o fluxo: `git worktree add <path> -b <branch> [<start-point>]`.
3. Registrar worktree no banco (missions.worktreePath ou tabela dedicada).
4. Aplicar lock ao iniciar missão, liberar ao finalizar.
5. Expor listagem e remoção na UI de configurações ou na página do projeto.
