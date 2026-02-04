# Métricas e Telemetria

> Framework de medição de sucesso para o Dev Command Center

---

## Visão Geral

Este documento define as métricas-chave (KPIs) do produto, como medi-las, e como usar os dados para tomar decisões.

**Princípio fundamental**: _"Decisões baseadas em dados > opiniões de engenheiro"_

---

## KPIs Principais

### 1. Time to Value (TTV)

**Definição**: Tempo médio entre criar uma missão e fazer o commit final.

**Target**: < 5 minutos para missões simples (< 10 arquivos)

**Como medir**:

```typescript
// Eventos de telemetria
analytics.track("mission_created", {
  missionId: string,
  timestamp: number,
  provider: string,
  projectId: string,
});

analytics.track("code_committed", {
  missionId: string,
  timestamp: number,
  filesCount: number,
  durationMs: number,
});

// Cálculo
TTV = avg(code_committed.timestamp - mission_created.timestamp);
```

**Segmentação**:

- Por provider (Cursor vs Claude vs Codex)
- Por tamanho de missão (pequena < 5 arquivos, média 5-20, grande > 20)
- Por tipo de alteração (bug fix, feature, refactor)

**Ações baseadas em dados**:

- Se TTV > 10 min: Investigar gargalos (geração lenta? review muito detalhado?)
- Se TTV aumentando: Algo piorou (performance regression)
- Se TTV diminuindo: Melhorias funcionando ✅

---

### 2. Success Rate

**Definição**: % de missões criadas que resultam em commit.

**Target**: > 70%

**Como medir**:

```typescript
// Eventos
analytics.track('mission_created', { ... });
analytics.track('mission_abandoned', {
  reason: 'code_quality' | 'took_too_long' | 'unclear_prompt' | 'other'
});
analytics.track('code_committed', { ... });

// Cálculo
successRate = commits / (commits + abandoned)
```

**Segmentação**:

- Por provider
- Por complexidade de missão
- Por experiência do usuário (novo vs experiente)

**Ações baseadas em dados**:

- < 50%: Problema crítico (código ruim ou UX confusa)
- 50-70%: Normal, pode melhorar
- > 70%: Excelente ✅

**Drill-down por razão de abandono**:

- Se maioria abandona por `code_quality`: Melhorar prompts/providers
- Se maioria abandona por `took_too_long`: Melhorar performance
- Se maioria abandona por `unclear_prompt`: Melhorar onboarding/exemplos

---

### 3. User Confidence Score

**Definição**: % de usuários que aplicam código sem editar.

**Target**: 50% (sweet spot)

**Como medir**:

```typescript
analytics.track('code_applied', {
  missionId: string,
  filesTotal: number,
  filesEdited: number,
  filesSelected: number,
  editedContent: boolean
});

// Cálculo
confidenceScore = % de missões onde filesEdited === 0 && filesSelected === filesTotal
```

**Interpretação**:

```
< 30%  → IA gerando código ruim
30-50% → Usuários cautelosos (normal)
50-70% → Alta confiança (ideal)
> 70%  → Talvez preview seja desnecessário?
```

**Ações baseadas em dados**:

- < 30%: Investigar qualidade de código gerado
- > 70%: Considerar modo "fast track" opcional

---

### 4. Error Recovery Rate

**Definição**: % de erros que usuário resolve sem abandonar missão.

**Target**: > 90%

**Como medir**:

```typescript
analytics.track("error_occurred", {
  type: "parse_error" | "timeout" | "provider_error" | "git_error",
  recovered: boolean,
  recoveryMethod: "git_diff" | "retry" | "manual" | null,
});

// Cálculo
recoveryRate = errors_recovered / total_errors;
```

**Ações baseadas em dados**:

- < 80%: Erros não recuperáveis (problema crítico)
- 80-90%: Pode melhorar recovery flows
- > 90%: Recovery working well ✅

---

### 5. Provider Performance

**Definição**: Métricas comparativas entre providers.

**Métricas por provider**:

```typescript
interface ProviderMetrics {
  avgResponseTime: number; // ms
  successRate: number; // %
  truncationRate: number; // %
  userRating: number; // 1-5 stars
  codeQuality: number; // % de código aplicado sem edição
  costPerMission: number; // USD (se API paga)
}
```

**Como usar**:

- Dashboard mostrando comparação lado a lado
- Recomendação automática de provider baseada em histórico
- Alerts se provider específico está com problema

---

## Eventos de Telemetria

### Eventos Obrigatórios

#### Lifecycle da Missão

```typescript
// 1. Missão criada
analytics.track("mission_created", {
  missionId: string,
  projectId: string,
  provider: string,
  hasPlanFeedback: boolean,
  hasPreserveInstructions: boolean,
});

// 2. Plano gerado
analytics.track("plan_generated", {
  missionId: string,
  provider: string,
  stepsCount: number,
  filesCount: number,
  complexity: "low" | "medium" | "high",
  durationMs: number,
  tokensUsed: number,
});

// 3. Código gerado
analytics.track("code_generated", {
  missionId: string,
  provider: string,
  filesCount: number,
  responseSize: number,
  parseSuccess: boolean,
  durationMs: number,
  tokensUsed: number,
});

// 4. Código revisado
analytics.track("code_reviewed", {
  missionId: string,
  filesEdited: number,
  filesUnselected: number,
  timeSpentReviewing: number,
});

// 5. Código aplicado
analytics.track("code_applied", {
  missionId: string,
  filesApplied: number,
  withBackup: boolean,
  durationMs: number,
});

// 6. Commit feito
analytics.track("code_committed", {
  missionId: string,
  commitHash: string,
  totalDurationMs: number,
});

// 7. Missão abandonada
analytics.track("mission_abandoned", {
  missionId: string,
  lastStatus: string,
  reason: string,
  timeSpent: number,
});
```

#### Eventos de Erro

```typescript
analytics.track("error_occurred", {
  type:
    "parse_error" |
    "timeout" |
    "provider_error" |
    "git_error" |
    "network_error",
  provider: string,
  message: string,
  recovered: boolean,
  recoveryMethod: string | null,
  responseSize: number,
});
```

#### Eventos de Recovery

```typescript
analytics.track("recovery_triggered", {
  originalError: string,
  method: "git_diff" | "retry" | "discard",
  filesRecovered: number,
  success: boolean,
});
```

---

## Setup de Telemetria

### Opções Recomendadas

#### 1. Posthog (Recomendado)

**Por quê**:

- Open source
- Free tier generoso (1M events/mês)
- Privacy-friendly (pode self-host)
- Session replay (ver o que usuário fez)
- Feature flags integrado

**Setup**:

```typescript
// electron/services/telemetry.ts
import posthog from "posthog-js";

posthog.init("YOUR_PROJECT_KEY", {
  api_host: "https://app.posthog.com",
  autocapture: false, // Queremos controle manual
  capture_pageview: false,
  disable_session_recording: false,
  anonymize_ip: true, // Privacy
  opt_out_capturing_by_default: false,
  loaded: (posthog) => {
    if (process.env.NODE_ENV === "development") {
      posthog.opt_out_capturing();
    }
  },
});

export const analytics = {
  track: (event: string, properties: Record<string, any>) => {
    if (userOptedIn()) {
      posthog.capture(event, properties);
    }
  },
  identify: (userId: string) => {
    if (userOptedIn()) {
      posthog.identify(userId);
    }
  },
};
```

#### 2. Mixpanel (Alternativa)

**Por quê**:

- Muito maduro
- Analytics poderosas
- Free tier OK (100K events/mês)

**Setup**: Similar ao Posthog

#### 3. Self-hosted (Para empresas)

**Opções**:

- Plausible Analytics (privacy-first)
- Umami (super simples)
- Matomo (mais completo)

---

## Dashboards Recomendados

### Dashboard 1: Product Health

**Métricas principais**:

- Time to Value (trend line)
- Success Rate (%)
- Active Users (DAU/WAU/MAU)
- Missions per User

**Objetivo**: Ver saúde geral do produto rapidamente.

### Dashboard 2: Provider Comparison

**Comparação lado a lado**:

- Response time
- Success rate
- Truncation rate
- User rating
- Cost per mission

**Objetivo**: Identificar melhor provider para cada caso.

### Dashboard 3: Funnel de Conversão

**Stages**:

```
Mission Created → Plan Generated → Code Generated → Code Reviewed → Code Applied → Committed
     100%              95%              85%             75%             70%         70%
```

**Objetivo**: Identificar onde usuários abandonam.

### Dashboard 4: Error Tracking

**Métricas**:

- Errors por tipo
- Error rate (%)
- Recovery rate (%)
- Most common errors

**Objetivo**: Identificar e resolver erros rapidamente.

---

## Privacy e Compliance

### Dados que NÃO coletamos

- ❌ Código fonte do usuário
- ❌ Prompts completos (só tamanho)
- ❌ Caminhos de arquivos completos
- ❌ IPs não-anonimizados
- ❌ Emails ou dados pessoais

### Dados que coletamos

- ✅ IDs anônimos (gerados localmente)
- ✅ Métricas de performance
- ✅ Eventos de lifecycle
- ✅ Tipos de erro (sem mensagens sensíveis)
- ✅ Provider utilizado
- ✅ Contadores (arquivos, linhas, etc)

### Opt-in/Opt-out

**Implementação**:

```typescript
// Primeira vez que abre app
if (!settings.telemetryDecided) {
  showTelemetryDialog({
    title: "Ajude a melhorar o Dev Command Center",
    message: "Coletamos métricas anônimas de uso para...",
    optInButton: "Sim, enviar dados anônimos",
    optOutButton: "Não, obrigado",
    learnMore: "https://docs.devcommandcenter.app/privacy",
  });
}

// Usuário pode mudar a qualquer momento em Settings
```

---

## Análise de Dados

### Queries Úteis

#### 1. Qual provider é mais rápido?

```sql
SELECT
  provider,
  AVG(durationMs) as avg_duration,
  COUNT(*) as total_missions
FROM code_generated
GROUP BY provider
ORDER BY avg_duration ASC;
```

#### 2. Onde usuários mais abandonam?

```sql
SELECT
  lastStatus,
  COUNT(*) as abandons,
  AVG(timeSpent) as avg_time_before_abandon
FROM mission_abandoned
GROUP BY lastStatus
ORDER BY abandons DESC;
```

#### 3. Taxa de truncamento por provider

```sql
SELECT
  provider,
  COUNT(CASE WHEN parseSuccess = false THEN 1 END) as truncations,
  COUNT(*) as total,
  (truncations * 100.0 / total) as truncation_rate
FROM code_generated
GROUP BY provider;
```

---

## Alertas e Monitoring

### Alerts Críticos

Configure alerts para:

1. **Success rate < 50%** → Problema crítico
2. **Error rate > 10%** → Muitos erros
3. **Avg response time > 60s** → Performance ruim
4. **Zero missions in 24h** → Telemetria quebrou?

### Monitoring Diário

Checar todo dia:

- Success rate vs target
- Novos tipos de erro
- Provider performance
- User feedback (ratings)

---

## A/B Testing

### Como fazer A/B tests

```typescript
// Usar feature flags do Posthog
const showNewUI = posthog.isFeatureEnabled("new-code-review-ui");

if (showNewUI) {
  return <NewCodeReviewUI />;
} else {
  return <OldCodeReviewUI />;
}

// Track qual versão performou melhor
analytics.track("code_reviewed", {
  variant: showNewUI ? "new" : "old",
  timeSpent: duration,
  filesEdited: count,
});
```

### Testes Recomendados

1. **Onboarding**: Com tour vs sem tour
2. **Progress feedback**: Com timer vs sem timer
3. **Default provider**: Cursor vs Claude
4. **Apply flow**: One-click vs review-then-apply

---

## Conclusão

### Checklist de Implementação

- [ ] Escolher plataforma (Posthog recomendado)
- [ ] Implementar eventos obrigatórios
- [ ] Adicionar opt-in dialog
- [ ] Criar dashboards principais
- [ ] Configurar alerts críticos
- [ ] Documentar no PRIVACY.md

### Princípios para Lembrar

1. **Privacy first** - Sempre anonimizar
2. **Opt-in transparente** - Explicar o porquê
3. **Data-driven decisions** - Métricas guiam roadmap
4. **Iterate based on data** - Não em opiniões

### Next Steps

1. Setup Posthog (1-2h)
2. Implementar eventos lifecycle (2-3h)
3. Criar dashboard Product Health (1h)
4. Adicionar opt-in dialog (1h)
5. Documentar privacy policy (1h)

**Total**: ~1 dia de trabalho para telemetria completa.

---

_Última atualização: 2026-02-04_
