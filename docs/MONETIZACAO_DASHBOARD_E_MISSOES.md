# Monetizacao baseada em valor: Missoes, Review e Dashboard

## Contexto

Com modelos mais baratos e mais fortes, cobrar apenas "acesso ao modelo" perde forca rapidamente.
No DevCommandCenter, o valor diferencial atual esta no fluxo operacional:

- missao/worktree isolada;
- execucao com agents e terminais;
- review de diff;
- commit/push/merge.

A estrategia recomendada e cobrar por **resultado de entrega e governanca do fluxo**, nao por "chat/token" isolado.

---

## O que o banco ja grava hoje (alto potencial)

Fonte: `lib/database/schema.sql`

### 1) Estrutura principal de operacao

- `projects`: repositorios locais, provider padrao, URL remota, ultimo acesso.
- `combs`: unidade de trabalho em worktree (nome, branch base, branch, status, ultimo acesso).
- `panes`: terminais e agents por comb (tipo, provider, status, atividade, layout).

### 2) Estrutura de missoes (pipeline)

Tabela `missions` ja registra:

- identificacao do fluxo (`project_id`, `mission_type`);
- providers usados (`provider_id`, `plan_provider_id`, `code_provider_id`);
- estado do ciclo (`status`, `wall_status`);
- conteudo gerado (`plan`, `generated_code`, `context`);
- rastreio operacional (`pending_commands`, `last_output_summary`, `last_git_summary`);
- rastreio de branch/worktree (`worktree_path`, `worktree_branch`, `base_branch`, `target_branch`);
- qualidade de execucao (`error_message`, `code_generation_attempts`);
- resultado de entrega (`is_committed`, `is_pushed`);
- tempo (`started_at`, `completed_at`, `created_at`, `updated_at`).

### 3) Logs detalhados

Tabela `mission_logs`:

- tipos de evento: `info`, `prompt`, `response`, `error`, `action`, `user_input`;
- `metadata` em JSON para tokens, tempo, etc.;
- trilha temporal (`created_at`).

### 4) Estados de comb para funil de entrega

`combs.status` ja cobre:

- `active`
- `ready_for_review`
- `applied`
- `discarded`
- `archived`
- `error`

Isso permite montar funil operacional sem alterar schema.

---

## Conclusao direta de negocio

Hoje **ja existe base de dados suficiente** para cobrar por valor com dashboard.
Nao precisa esperar telemetria complexa para comecar.

Em termos de proposta comercial:

- fraco: "pague para usar o modelo X";
- forte: "pague para acelerar entrega com previsibilidade, visibilidade e controle do fluxo".

---

## Dashboard MVP (o que mostrar para justificar preco)

## 1) Throughput de entrega

- Missoes criadas por periodo;
- Missoes concluidas por periodo;
- Combs aplicadas/mergeadas por periodo.

Por que importa: mostra velocidade de producao.

## 2) Lead time e cycle time

- Tempo `created_at -> completed_at` (missoes);
- Tempo `active -> ready_for_review -> applied` (combs, aproximado por timestamps de atualizacao).

Por que importa: mostra previsibilidade.

## 3) Funil de status

- Distribuicao de `missions.status`;
- Distribuicao de `combs.status`;
- Taxa de erro/cancelamento.

Por que importa: mostra gargalos e perda.

## 4) Eficiencia por provider/agent

- Taxa de missao concluida por provider;
- Tentativas medias (`code_generation_attempts`) por provider;
- Erros por provider (`error_message`/logs).

Por que importa: evita custo oculto e mostra "qual stack entrega mais".

## 5) Intensidade de mudanca

Com `last_git_summary`:

- arquivos alterados;
- insercoes/delecoes por missao;
- distribuicao de tamanho de mudanca (small/medium/large).

Por que importa: demonstra volume real de trabalho entregue.

## 6) Adoção de workflow disciplinado

- `% missoes com commit`;
- `% missoes com push`;
- `% combs que chegam em review`;
- `% combs aplicadas`.

Por que importa: converte uso da ferramenta em impacto no processo.

---

## SQL inicial (exemplos praticos)

Observacao: `last_git_summary` e JSON texto; em SQLite use `json_extract`.

### Throughput por dia (missoes criadas)

```sql
SELECT
  date(created_at) AS day,
  COUNT(*) AS missions_created
FROM missions
GROUP BY date(created_at)
ORDER BY day DESC;
```

### Taxa de conclusao de missoes

```sql
SELECT
  COUNT(*) AS total,
  SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) AS completed,
  ROUND(
    100.0 * SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) / NULLIF(COUNT(*), 0),
    2
  ) AS completion_rate_pct
FROM missions;
```

### Lead time medio (minutos)

```sql
SELECT
  ROUND(AVG((julianday(completed_at) - julianday(created_at)) * 24 * 60), 2) AS avg_lead_time_min
FROM missions
WHERE completed_at IS NOT NULL;
```

### Funil de combs

```sql
SELECT
  status,
  COUNT(*) AS qty
FROM combs
GROUP BY status
ORDER BY qty DESC;
```

### Eficiencia por provider (missoes)

```sql
SELECT
  COALESCE(code_provider_id, provider_id, 'unknown') AS provider_ref,
  COUNT(*) AS total,
  SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) AS completed,
  ROUND(AVG(code_generation_attempts), 2) AS avg_attempts
FROM missions
GROUP BY COALESCE(code_provider_id, provider_id, 'unknown')
ORDER BY total DESC;
```

### Volume de mudanca (quando houver JSON valido em `last_git_summary`)

```sql
SELECT
  ROUND(AVG(COALESCE(json_extract(last_git_summary, '$.changedFiles'), 0)), 2) AS avg_changed_files,
  ROUND(AVG(COALESCE(json_extract(last_git_summary, '$.insertions'), 0)), 2) AS avg_insertions,
  ROUND(AVG(COALESCE(json_extract(last_git_summary, '$.deletions'), 0)), 2) AS avg_deletions
FROM missions
WHERE last_git_summary IS NOT NULL;
```

---

## Gaps para aumentar valor comercial (pequenos ajustes)

Mesmo com base forte, alguns campos elevariam muito a clareza do dashboard:

1. `merged_at` (missao/comb): separa "aplicado" de "mergeado em branch alvo".
2. `review_started_at` e `review_completed_at`: mede tempo de review de verdade.
3. `pr_url`/`pr_number` (quando houver): liga produtividade com entrega no Git.
4. `cost_estimate` por missao (derivado de tokens + provider): habilita ROI.
5. `team_id`/`user_id` opcional: habilita visao por time e accountability.

Sugestao: manter isso como migration incremental, sem quebrar o fluxo atual.

---

## Estrategia de monetizacao recomendada

## Free

- Missoes e combs basicas;
- Review local;
- Sem historico analitico avancado (janela curta).

## Pro

- Dashboard completo (throughput, lead time, funil, provider efficiency);
- Historico de 90+ dias;
- Relatorios exportaveis (CSV/JSON).

## Team

- Visao por projeto/time;
- Auditoria de fluxo (logs e trilha de acoes);
- Metas/SLA de entrega e alertas de gargalo.

Mensagem comercial:
"Nao vendemos acesso a modelo; vendemos velocidade com previsibilidade."

---

## Plano de execucao em 3 fases

## Fase 1 (rapida: 1-2 sprints)

- montar consultas SQL de metricas principais;
- criar dashboard inicial no app (cards + series + funil);
- filtrar por projeto e periodo.

## Fase 2

- adicionar campos de rastreio de review/merge;
- relatorio de eficiencia por provider;
- exportacao de relatorios.

## Fase 3

- benchmark por time/projeto;
- score de saude operacional;
- insights automatizados ("seu gargalo esta em review", etc.).

---

## Decisao pratica para agora

Sim, faz sentido cobrar.
Mas o racional precisa ser:

- **nao**: "porque tem modelo novo";
- **sim**: "porque a plataforma prova ganho operacional com dados".

Com o schema atual, voces ja conseguem entregar o primeiro nivel dessa promessa.
