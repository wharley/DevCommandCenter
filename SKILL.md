# Dev Command Center - Alpha Skill Document (v0.3+)

> **Patch & Git Execution UI**: missão → plano → diffs → apply → commit, com vários providers (BYOK). Não é plataforma genérica de agentes.

## Visão Geral

O Dev Command Center é um aplicativo desktop cross-platform (macOS, Windows, Linux) desenvolvido em **Electron + Vite + React + TypeScript**. O produto é um **fluxo de execução de patches e Git**: o usuário descreve uma missão em linguagem natural, revisa plano e diffs gerados por IA e aplica as mudanças no repositório com controle (dry-run, backup, commit). O app não hospeda modelo próprio; suporta vários provedores (Claude Code, Codex, OpenAI, Anthropic, Cursor Agent CLI, etc.) via CLIs/APIs, todos entrando no mesmo funil **Plan + Unified Diff + Apply + Git**.

### Proposta de Valor

- **Um fluxo único** para missões de código: plano → alterações → review de diffs → apply → commit
- **Multi-provider** (CLI e API) com BYOK — usuário traz suas próprias chaves
- **Preview de diffs** antes de aplicar mudanças
- **Worktrees Git** (policy definida, implementação pendente) para tarefas paralelas sem trocar branch
- **Aplicação controlada** e transparência (logs, métricas)

### Guardrails de Produto

Manifesto técnico — princípios que impedem o produto de ir para o lado errado:

- ✅ **Não somos agent platform** — Patch & Git Execution UI, não orquestrador genérico
- ✅ **Diff-first apply** — `git apply` preferido; escrita de arquivo só como fallback
- ✅ **Segurança BYOK** — API keys criptografadas (safeStorage em Alpha; keytar em Beta)
- ✅ **Policy de worktree** — nome padrão, limpeza, lock; regras em [WORKTREE_POLICY.md](docs/WORKTREE_POLICY.md)
- ✅ **Sem ações Git sem confirmação do usuário** — apply, commit, push só após ação explícita na UI

---

## Posicionamento e Referências

O Dev Command Center toma como **referência** o fluxo de ferramentas como Commander (missão → plano → código → apply) e o ecossistema de agentes de código, mas **não é clone** de Commander nem de orquestradores genéricos (ex.: Compozy). A meta é ser um **Commander melhorado**: mesma ideia de fluxo estruturado (Plan + Diff + Apply + Git), com multi-provider real, worktrees e UX local-first, sem virar “plataforma de agentes” com formatos de saída abertos ou triagem infinita. O produto é **Patch & Git Execution UI**, não agent platform.

### Fluxo principal (norte do produto)

1. **Selecionar repositório** e contexto (branch/worktree)
2. **Descrever a missão** em linguagem natural
3. **Gerar plano** e revisar passos
4. **Gerar alterações** e revisar diffs
5. **Aplicar mudanças** e **commitar** com segurança

### Diferenciais pretendidos

- **Multi-provider real** (CLI e API) com fallback e validação
- **Worktrees como feature de primeira classe** para paralelismo de tarefas
- **Aplicação controlada** de mudanças (dry-run, backup, logs)
- **Transparência**: logs detalhados de execução e métricas de tempo/tokens

### Princípios de Aplicação

- **Diff-first:** O apply sempre tenta `git apply --check` + `git apply` quando há unified diff válido. Escrita direta de arquivo (`suggestedContent`) é fallback apenas quando patch não é aplicável ou está ausente. Isso garante aplicação precisa e rastreável via Git.

### Worktree Policy

Antes de implementar criação de worktrees, as regras de governança estão definidas em:

- **Documento**: [docs/WORKTREE_POLICY.md](docs/WORKTREE_POLICY.md)
- **Constantes**: [electron/services/worktree-policy.ts](electron/services/worktree-policy.ts)

Resumo: nome padrão `dcc-{id}-{timestamp}`, limpeza de worktrees antigos (>7 dias), listagem/reaproveitamento, lock durante missão em execução.

---

## Proposta de Monetização (hipótese, pós-MVP)

Esta seção é **apenas uma hipótese** para validar depois do MVP. A ideia é testar valor percebido sem travar o produto cedo demais.

### Estratégia sugerida

- **Free**: experiência completa para uso individual, com limites leves (ex.: número de projetos ativos, histórico curto).
- **Pro (simbólico)**: remove limites, traz histórico completo e recursos avançados.

### Possíveis recursos Pro

- **Worktrees avançados** (templates, presets, gerenciamento em lote)
- **Histórico expandido** de missões/diffs/logs
- **Automations** (apply + tests + commit com confirmação)
- **Perfis de providers** por projeto e presets de missão

### Observações

- O modelo **BYOK** reduz custos operacionais e facilita preço baixo.
- Se a percepção de valor for alta, considerar planos mensais/anuais simples.

---

## Stack Tecnológico

### Core

| Camada             | Tecnologia           | Versão   |
| ------------------ | -------------------- | -------- |
| Desktop Runtime    | Electron             | ^33.0.0  |
| Build Tool         | Vite                 | ^6.0.0   |
| Frontend Framework | React + React Router | React 19.2.0 / Router 7.x |
| Linguagem          | TypeScript           | ^5       |
| Database           | better-sqlite3       | ^11.8.0  |
| State Management   | Zustand              | 5.0.10   |
| Node.js (runtime)  | Node.js              | >=22.0.0 |

### UI/UX

| Componente    | Tecnologia            |
| ------------- | --------------------- |
| Design System | Radix UI Primitives   |
| Styling       | Tailwind CSS v4       |
| Icons         | Lucide React          |
| Forms         | React Hook Form + Zod |
| Notifications | Sonner                |
| Charts        | Recharts              |

### Build & Dev

| Ferramenta       | Uso                           |
| ---------------- | ----------------------------- |
| electron-builder | Empacotamento multiplataforma |
| concurrently     | Dev server paralelo           |
| wait-on          | Sincronização de startup      |

---

## Estrutura de Pastas

```
DevCommandCenter/
├── src/                          # Código fonte React (Vite)
│   ├── App.tsx                   # Componente raiz com rotas
│   ├── main.tsx                  # Entry point
│   ├── globals.css               # Estilos globais
│   └── pages/                    # Páginas (React Router)
│       ├── HomePage.tsx          # Página inicial (lista projetos)
│       ├── ProjectPage.tsx       # Detalhe do projeto
│       ├── MissionPage.tsx       # Detalhe da missão
│       └── SettingsPage.tsx      # Configurações/Providers
│
├── components/
│   ├── app-sidebar.tsx           # Sidebar principal
│   ├── theme-provider.tsx        # Provider de tema
│   ├── dialogs/
│   │   ├── add-project-dialog.tsx
│   │   ├── add-provider-dialog.tsx
│   │   ├── commit-dialog.tsx
│   │   ├── edit-provider-dialog.tsx
│   │   └── new-mission-dialog.tsx
│   ├── layouts/
│   │   └── main-layout.tsx       # Layout com sidebar
│   └── ui/                       # Componentes Radix/shadcn
│       ├── button.tsx
│       ├── card.tsx
│       ├── dialog.tsx
│       ├── empty.tsx             # Estado vazio
│       └── ... (50+ componentes)
│
├── electron/
│   ├── main.ts                   # Processo principal Electron
│   ├── preload.ts                # Bridge IPC seguro
│   ├── ipc-handlers.ts           # Handlers IPC (DB + AI + Git)
│   ├── tsconfig.json             # Config TS do Electron
│   └── services/                 # Serviços do backend
│       ├── types.ts              # Tipos compartilhados
│       ├── schemas/              # Schemas Zod (MissionPlan, GeneratedCode)
│       ├── git-service.ts        # Operações Git
│       ├── worktree-policy.ts    # Constantes Worktree Policy
│       ├── ai-orchestrator.ts    # Orquestrador de IA
│       └── adapters/             # Adapters de providers
│           ├── base.ts           # Classe base abstrata
│           ├── claude-code.ts    # Claude Code CLI
│           ├── codex.ts          # OpenAI Codex CLI
│           ├── openai.ts         # OpenAI API direta
│           ├── anthropic.ts      # Anthropic API direta
│           ├── cursor.ts         # Cursor Agent CLI
│           └── index.ts          # Factory e registry
│
├── hooks/
│   ├── use-app-store.ts          # Estado global Zustand
│   ├── use-data.ts               # Hook de dados
│   ├── use-electron.ts           # Hook para APIs Electron
│   └── use-toast.ts              # Hook de notificações
│
├── lib/
│   ├── database/
│   │   ├── connection.ts         # Conexão SQLite
│   │   ├── index.ts              # Exports do database
│   │   ├── normalize.ts          # Normalização de dados
│   │   ├── schema.sql            # Schema DDL
│   │   ├── types.ts              # Tipos TypeScript
│   │   └── repositories/
│   │       ├── providers.ts      # CRUD Providers
│   │       ├── projects.ts       # CRUD Projects
│   │       ├── missions.ts       # CRUD Missions
│   │       └── mission-logs.ts   # CRUD Logs
│   ├── services/
│   │   └── ai-service.tsx        # Serviço de IA (frontend)
│   └── utils.ts                  # Utilitários gerais
│
├── types/
│   └── electron.d.ts             # Tipos globais Electron
│
├── docs/                         # Documentação
│   └── WORKTREE_POLICY.md        # Política de worktrees
│
├── public/                       # Assets estáticos
├── styles/
│   └── globals.css               # CSS adicional
│
├── electron-builder.yml          # Config de build
├── vite.config.ts                # Config Vite
├── package.json
├── tsconfig.json
└── postcss.config.mjs
```

---

## Arquitetura do Sistema

### Camadas da Aplicação

```
┌─────────────────────────────────────────────────────────────┐
│                      UI Layer (React)                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Pages     │  │  Dialogs    │  │    Components       │  │
│  │ (React Router) │  │  (Radix)    │  │   (shadcn/ui)       │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│                    State Layer (Zustand)                     │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ useAppStore: projects, providers, missions, logs     │   │
│  └──────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│                    Service Layer                             │
│  ┌─────────────────┐  ┌─────────────────────────────────┐   │
│  │   AI Service    │  │     IPC Bridge (preload.ts)     │   │
│  │  (mock/real)    │  │                                  │   │
│  └─────────────────┘  └─────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│                    Electron Main Process                     │
│  ┌─────────────────┐  ┌─────────────────────────────────┐   │
│  │  IPC Handlers   │  │     Database (SQLite)            │   │
│  │                 │  │     - Repositories               │   │
│  └─────────────────┘  └─────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### Fluxo de Dados

```
User Action → React Component → useAppStore (Zustand)
                                      ↓
                              IPC via preload.ts
                                      ↓
                         Electron Main Process
                                      ↓
                         ipc-handlers.ts → Database
                                      ↓
                              Repository (SQLite)
```

---

## Modelo de Dados

### Schema SQLite

```sql
-- Providers de IA (Claude Code, OpenAI, etc.)
CREATE TABLE providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('claude-code', 'codex', 'openai', 'anthropic', 'cursor', 'custom')),
  api_key TEXT,
  cli_path TEXT,
  config TEXT,           -- JSON
  is_active INTEGER DEFAULT 1,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);

-- Projetos (repositórios locais)
CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  path TEXT NOT NULL UNIQUE,
  description TEXT,
  default_provider_id TEXT,
  git_remote_url TEXT,
  last_opened_at TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (default_provider_id) REFERENCES providers(id) ON DELETE SET NULL
);

-- Missões de Código
CREATE TABLE missions (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  provider_id TEXT,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'created',
  plan TEXT,             -- JSON: MissionPlan
  generated_code TEXT,   -- JSON: GeneratedCode
  context TEXT,          -- JSON: MissionContext
  error_message TEXT,
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE SET NULL
);

-- Logs das Missões
CREATE TABLE mission_logs (
  id TEXT PRIMARY KEY,
  mission_id TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('info', 'prompt', 'response', 'error', 'action', 'user_input')),
  content TEXT NOT NULL,
  metadata TEXT,         -- JSON
  created_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (mission_id) REFERENCES missions(id) ON DELETE CASCADE
);
```

### Tipos TypeScript

```typescript
// Provider Types (deve corresponder ao CHECK constraint em schema.sql)
type ProviderType =
  | "claude-code"
  | "codex"
  | "openai"
  | "anthropic"
  | "cursor"
  | "custom";

interface Provider {
  id: string;
  name: string;
  type: ProviderType;
  apiKey?: string | null;
  cliPath?: string | null;
  config?: ProviderConfig | null;
  isActive: boolean;
  createdAt: Date;
  updatedAt: Date;
}

// Mission Status Flow
type MissionStatus =
  | "created" // Inicial
  | "planning" // Gerando plano
  | "plan_generated" // Plano pronto
  | "generating_code" // Gerando código
  | "code_ready" // Código pronto para review
  | "applying" // Aplicando mudanças
  | "completed" // Concluído
  | "failed" // Erro
  | "cancelled"; // Cancelado

interface Mission {
  id: string;
  projectId: string;
  providerId?: string | null;
  title: string;
  description: string;
  status: MissionStatus;
  plan?: MissionPlan | null;
  generatedCode?: GeneratedCode | null;
  context?: MissionContext | null;
  errorMessage?: string | null;
  startedAt?: Date | null;
  completedAt?: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

interface MissionPlan {
  steps: PlanStep[];
  summary?: string;
  estimatedComplexity?: "low" | "medium" | "high";
}

interface PlanStep {
  id: string;
  order: number;
  title: string;
  description: string;
  files?: string[];
  status?: "pending" | "in_progress" | "completed" | "skipped";
}

interface GeneratedCode {
  files: CodeSuggestion[];
  summary?: string;
}

interface CodeSuggestion {
  path: string;
  action: "create" | "modify" | "delete";
  originalContent?: string;
  suggestedContent?: string;
  diff?: string;
}
```

---

## Fluxo de uma Missão

### Estados e Transições

```
┌─────────────┐
│   CREATED   │ ─── User creates mission
└──────┬──────┘
       │ "Generate Plan"
       ▼
┌─────────────┐
│  PLANNING   │ ─── AI generating plan
└──────┬──────┘
       │
       ▼
┌─────────────────┐
│ PLAN_GENERATED  │ ─── Plan ready for review
└───────┬─────────┘
        │ "Generate Code"
        ▼
┌───────────────────┐
│ GENERATING_CODE   │ ─── AI generating code
└─────────┬─────────┘
          │
          ▼
┌─────────────┐
│ CODE_READY  │ ─── Code ready for review
└──────┬──────┘
       │ "Apply Changes"
       ▼
┌─────────────┐
│  APPLYING   │ ─── Applying to repo
└──────┬──────┘
       │
       ▼
┌─────────────┐
│  COMPLETED  │ ─── Done!
└─────────────┘

       │ (any error)
       ▼
┌─────────────┐
│   FAILED    │
└─────────────┘
```

### Fluxo de UX Completo

1. **Usuário seleciona projeto** → Abre página do projeto
2. **Clica "Nova Missão"** → Dialog com título, descrição, provider
3. **Missão criada** → Status: `created`
4. **Clica "Gerar Plano"** → AI Service é chamado
5. **Plano exibido** → Lista de passos com arquivos afetados
6. **Clica "Gerar Código"** → AI gera sugestões
7. **Review de diffs** → Usuário vê mudanças propostas
8. **Clica "Aplicar"** → Mudanças aplicadas ao repo

---

## API de Comunicação IPC

### Canais Disponíveis

O `preload.ts` expõe APIs seguras via `contextBridge`:

```typescript
// window.electronAPI
electronAPI: {
  platform: 'darwin' | 'win32' | 'linux',
  dialog: {
    selectDirectory(): Promise<string | null>,
    showMessage(options): Promise<number>,
    confirm(message: string): Promise<boolean>,
  },
  shell: {
    openExternal(url: string): Promise<void>,
    openPath(path: string): Promise<void>,
    showItemInFolder(path: string): void,
  },
  window: {
    minimize(): Promise<void>,
    maximize(): Promise<void>,
    close(): Promise<void>,
    isMaximized(): Promise<boolean>,
  },
}

// window.db
db: {
  providers: {
    findAll(): Promise<Provider[]>,
    findById(id: string): Promise<Provider | null>,
    findActive(): Promise<Provider[]>,
    create(data: CreateProviderDTO): Promise<Provider>,
    update(id: string, data: UpdateProviderDTO): Promise<Provider>,
    delete(id: string): Promise<boolean>,
    setActive(id: string, isActive: boolean): Promise<Provider>,
    testConnection(id: string): Promise<boolean>,
  },
  projects: { /* similar CRUD */ },
  missions: {
    /* CRUD + */
    updateStatus(id, status): Promise<Mission>,
    updatePlan(id, plan): Promise<Mission>,
    updateGeneratedCode(id, code): Promise<Mission>,
    start(id): Promise<Mission>,
    complete(id, summary?): Promise<Mission>,
    fail(id, error): Promise<Mission>,
    cancel(id): Promise<Mission>,
    getFullMission(id): Promise<Mission & { logs: MissionLog[] }>,
  },
  missionLogs: {
    /* CRUD + */
    logInfo(missionId, message, metadata?): Promise<MissionLog>,
    logError(missionId, message, metadata?): Promise<MissionLog>,
    logAgentAction(missionId, action, details?): Promise<MissionLog>,
    getStats(missionId): Promise<LogStats>,
    getLatest(missionId, count?): Promise<MissionLog[]>,
  },
  utils: {
    backup(destPath: string): Promise<boolean>,
    getPath(): Promise<string>,
    getSize(): Promise<number>,
  },
}
```

A API completa (incluindo `electronAPI.ai` — generatePlan, generateCode, applyChanges, testConnection, validateProvider — e `electronAPI.git` — getInfo, getStatus, getCurrentBranch, listFiles, commit, push, etc. — além de métodos extras de `db` como findByType, findByPath, search, getStats, logWarning, logDebug) está definida em [electron/preload.ts](electron/preload.ts).

---

## Serviço de IA

### Estrutura do AIService

```typescript
// lib/services/ai-service.tsx

interface AIServiceConfig {
  provider: Provider;
  mission: Mission;
  projectContext?: {
    files: string[];
    fileContents?: Record<string, string>;
  };
}

interface AIResponse {
  success: boolean;
  data?: MissionPlan | GeneratedCode;
  error?: string;
  metadata?: {
    tokensUsed?: number;
    durationMs?: number;
    model?: string;
  };
}

class AIService {
  constructor(config: AIServiceConfig);

  // Gera plano de ação
  async generatePlan(): Promise<AIResponse>;

  // Gera sugestões de código
  async generateCode(): Promise<AIResponse>;

  // Valida configuração do provider
  static validateProvider(provider: Provider): {
    valid: boolean;
    errors: string[];
  };
}

// Factory
function createAIService(config: AIServiceConfig): AIService;
```

### Pontos de Extensão para Integração Real

O MVP usa respostas mockadas. Para integrar providers reais:

```typescript
// 1. Implementar adapter para cada provider
interface AIProviderAdapter {
  name: string;
  type: ProviderType;

  // Validação específica do provider
  validate(config: ProviderConfig): ValidationResult;

  // Chamada real à API/CLI
  generatePlan(mission: Mission, context: MissionContext): Promise<MissionPlan>;
  generateCode(mission: Mission, plan: MissionPlan): Promise<GeneratedCode>;
}

// 2. Adapters específicos
class ClaudeCodeAdapter implements AIProviderAdapter {
  // Usa CLI: `claude --print --output-format stream-json`
  async generatePlan(mission, context) {
    const result = await execAsync(`claude "${buildPrompt(mission)}"`, {
      cwd: context.projectPath,
    });
    return parseClaudeResponse(result);
  }
}

class OpenAIAdapter implements AIProviderAdapter {
  // Usa API REST
  async generatePlan(mission, context) {
    const response = await fetch("https://api.openai.com/v1/chat/completions", {
      headers: { Authorization: `Bearer ${this.apiKey}` },
      body: JSON.stringify({
        model: "gpt-4-turbo",
        messages: buildMessages(mission, context),
      }),
    });
    return parseOpenAIResponse(response);
  }
}

// 3. Registry de providers
const providerRegistry = new Map<ProviderType, AIProviderAdapter>();
providerRegistry.set("claude-code", new ClaudeCodeAdapter());
providerRegistry.set("openai", new OpenAIAdapter());
```

---

## Telas do MVP

### 1. Tela Inicial (Projects)

**Rota:** `/`

**Funcionalidades:**

- Lista de projetos recentes ordenados por `lastOpenedAt`
- Busca por nome/descrição
- Badge com contagem de missões ativas
- Botão "Add Project" → Dialog para selecionar pasta
- Cards com: nome, descrição, branch, última abertura
- Dropdown menu: Open, Delete

**Componentes:**

- `AddProjectDialog` - seleção de pasta via `dialog:selectDirectory`

### 2. Tela do Projeto

**Rota:** `/project/[id]`

**Funcionalidades:**

- Header com nome, path, branch, provider padrão
- Cards de estatísticas: Total, Ativas, Concluídas
- Lista de missões ordenadas (ativas primeiro)
- Status badges com ícones animados
- Botão "Nova Missão" → Dialog

**Componentes:**

- `NewMissionDialog` - título, descrição, seleção de provider

### 3. Tela da Missão

**Rota:** `/project/[id]/mission/[missionId]`

**Funcionalidades:**

- Header com título, status badge, descrição
- Barra de progresso (steps completados)
- Tabs: Plan | Code | Logs
- Botões contextuais:
  - `created` → "Generate Plan"
  - `plan_generated` → "Generate Code"
  - `code_ready` → "Apply Changes"

**Sub-views:**

- **PlanView**: Summary + lista de steps com status
- **CodeView**: Lista de arquivos + diffs em code blocks
- **LogsView**: Timeline de logs com ícones por tipo

### 4. Tela de Settings

**Rota:** `/settings`

**Funcionalidades:**

- Lista de providers configurados
- Toggle de ativo/inativo
- Indicadores: API Key, CLI Path, Model
- Dropdown menu: Edit, Delete
- Botão "Add Provider" → Dialog
- Seção "About" com versão

**Componentes:**

- `AddProviderDialog` - tipo, nome, API key, CLI path, config
- `EditProviderDialog` - edição de provider existente

---

## Configuração de Desenvolvimento

### Variáveis de Ambiente

```bash
# .env.local (não commitado)
NODE_ENV=development

# Providers (exemplo)
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
CLAUDE_CLI_PATH=/usr/local/bin/claude
```

### Autenticação de providers CLI

Para **Cursor CLI**, **Claude Code CLI** e **Codex CLI**, o login é feito **no terminal**, não dentro do app. O usuário deve abrir o terminal e rodar o comando de autenticação do respectivo CLI (ex.: `claude login`, ou o equivalente do Cursor/Codex) quando necessário; o Dev Command Center apenas invoca o binário já autenticado. O app não gerencia credenciais dos CLIs — se precisar logar no CLI, faça-o no terminal.

### Scripts NPM

```bash
# Desenvolvimento
yarn dev                # Vite dev server (http://localhost:5173)
yarn electron:dev       # Electron + Vite em paralelo (wait-on porta 5173)

# Build
yarn build              # Vite build (saída em dist/)
yarn electron:compile   # Compila electron/*.ts para .electron/ e copia schema.sql
yarn electron:build     # electron:compile + vite build + electron-builder
yarn electron:rebuild   # Recompila módulos nativos (better-sqlite3)

# Produção
yarn electron:start     # Roda app compilado localmente
```

### Processo de Build

1. `yarn electron:compile` → Compila `electron/*.ts` para `.electron/` e copia `lib/database/schema.sql` para `.electron/lib/database/`
2. `yarn build` (Vite) → Gera saída em `dist/`
3. `electron-builder` → Empacota para cada plataforma usando `dist/` e main em `.electron/electron/main.js`

**Nota:** Electron 33 usa Node.js 22+. Certifique-se de usar Node.js >=22 no desenvolvimento.

---

## Pontos de Extensão

### 1. Adicionar Novo Provider

```typescript
// 1. Adicionar tipo em lib/database/types.ts
export type ProviderType =
  | "claude-code"
  | "codex"
  | "openai"
  | "anthropic"
  | "cursor"
  | "gemini" // ← novo
  | "custom";

// 2. Atualizar schema.sql (CHECK constraint) — OBRIGATÓRIO para alinhar com TS
// 3. Criar adapter em electron/services/adapters/ (ex.: gemini.ts)
// 4. Registrar no createAdapter e adapterRegistry em electron/services/adapters/index.ts
// 5. Adicionar UI em components/dialogs/add-provider-dialog.tsx
```

#### Cursor Agent CLI (tipo `cursor`)

- **Necessidades:** CLI `agent` (needsCli: true, needsApiKey: false no app; autenticação é feita no terminal via Cursor instalado).
- **Instalação do CLI:** `curl https://cursor.com/install -fsSL | bash` (ver cursor.com/docs/cli).
- **Exemplo de uso no terminal:** `agent chat "descrição da missão"` (ou equivalente conforme documentação oficial).
- O adapter em `electron/services/adapters/cursor.ts` invoca o binário `agent` (ou caminho configurável em `cliPath`) com a descrição da missão/contexto e parseia a resposta.
- **Formato de saída com `--output-format json`:** O CLI pode retornar um único JSON no stdout, possivelmente envolvido em `{ "result": ... }` ou `{ "output": ... }` (string ou objeto). O adapter normaliza esse wrapper (string ou objeto com `.files`/`.steps`) e aplica correção de newlines literais (via `parseJSONResponse` na base) antes de interpretar como `GeneratedCode` ou `MissionPlan`.

### 2. Implementar Diff Real

```typescript
// lib/services/git-service.ts
import { exec } from "child_process";

class GitService {
  constructor(private projectPath: string) {}

  async applyDiff(filePath: string, diff: string): Promise<boolean> {
    // Usar `git apply` ou escrita direta
  }

  async createBranch(name: string): Promise<void> {
    await exec(`git checkout -b ${name}`, { cwd: this.projectPath });
  }

  async commit(message: string): Promise<void> {
    await exec(`git commit -am "${message}"`, { cwd: this.projectPath });
  }
}
```

### 3. Backend SaaS (Multi-usuário)

```
Arquitetura futura:
┌─────────────────────────────────────┐
│           Desktop Client            │
│  (Electron - mantém UX atual)       │
└──────────────┬──────────────────────┘
               │ REST/WebSocket
               ▼
┌─────────────────────────────────────┐
│          Backend API                │
│  - Auth (OAuth, API Keys)           │
│  - Sync de projetos/missões         │
│  - Queue de jobs (Bull/Redis)       │
│  - PostgreSQL (multi-tenant)        │
└─────────────────────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│        AI Provider Workers          │
│  - Rate limiting por usuário        │
│  - Billing/Usage tracking           │
└─────────────────────────────────────┘
```

### 4. Melhorias de UX Planejadas

- [ ] **Real-time diff viewer** com syntax highlighting
- [ ] **Markdown preview** para planos
- [ ] **Drag & drop** para reordenar steps
- [ ] **Keyboard shortcuts** (Cmd+N, Cmd+G, etc.)
- [ ] **Tema system** (resposta ao prefers-color-scheme)
- [ ] **Notificações nativas** quando missão completa
- [ ] **Histórico de versões** para cada missão
- [ ] **Templates de missão** (refactor, migrate, test, etc.)

---

## Testes

### Estrutura Planejada

```
__tests__/
├── unit/
│   ├── database/
│   │   └── repositories.test.ts
│   ├── services/
│   │   └── ai-service.test.ts
│   └── hooks/
│       └── use-app-store.test.ts
├── integration/
│   ├── ipc-handlers.test.ts
│   └── mission-flow.test.ts
└── e2e/
    ├── create-project.spec.ts
    └── mission-workflow.spec.ts
```

### Ferramentas Recomendadas

- **Unit/Integration:** Vitest
- **E2E:** Playwright (com @playwright/test)
- **Mocking:** MSW (Mock Service Worker)

---

## Segurança

### Práticas Implementadas

1. **Context Isolation:** `contextIsolation: true` no Electron
2. **Node Integration Disabled:** `nodeIntegration: false`
3. **Preload Script:** Bridge seguro via `contextBridge`
4. **Sandbox Mode:** `sandbox: false` (necessário para better-sqlite3)
5. **External Links:** Abertos no browser padrão via `shell.openExternal`

### Armazenamento de Credenciais

**Status:** API keys atualmente em texto plano no SQLite. Criptografia é requisito de Alpha.

**Roadmap de segurança:**
- **Alpha (requisito):** `@electron/safeStorage` — encrypt/decrypt no main process antes de persistir
- **Beta:** `keytar` — integração com Keychain (macOS), Secret Service (Linux), Credential Manager (Windows)

```typescript
// Implementação planejada para Alpha
import { safeStorage } from "electron";

function encryptApiKey(key: string): Buffer {
  return safeStorage.encryptString(key);
}

function decryptApiKey(encrypted: Buffer): string {
  return safeStorage.decryptString(encrypted);
}

// Fluxo: UI envia key → main encripta → SQLite armazena blob → main decripta ao usar
```

---

## Troubleshooting

### Problemas Comuns

**1. Database não inicializa**

```bash
# Verificar se better-sqlite3 está compilado para versão correta do Electron
yarn electron:rebuild
# Ou manualmente:
npx @electron/rebuild -f -w better-sqlite3
```

**2. IPC não responde**

```typescript
// Verificar se handler está registrado em ipc-handlers.ts
// Verificar se canal está exposto em preload.ts
```

**3. Vite não carrega no Electron**

```bash
# Verificar se dev server está rodando (porta 5173)
curl http://localhost:5173
# Verificar console do Electron (DevTools)
# Em produção, verificar se dist/ foi gerado e se o main aponta para .electron/electron/main.js
```

**4. Build falha no Windows**

```bash
# Instalar Visual Studio Build Tools
npm install --global windows-build-tools
```

---

## Referências

- [Electron Documentation](https://www.electronjs.org/docs)
- [Vite](https://vitejs.dev/)
- [React Router](https://reactrouter.com/)
- [better-sqlite3](https://github.com/WiseLibs/better-sqlite3)
- [Zustand](https://github.com/pmndrs/zustand)
- [shadcn/ui](https://ui.shadcn.com/)
- [Radix UI](https://www.radix-ui.com/)

---

## Checklist: Feito vs Pendente

### Feito (condizente com o código hoje)

- **Stack:** Electron 33, Vite 6, React 19, React Router 7, TypeScript 5, better-sqlite3, Zustand, Node 22+.
- **Build:** Vite (`dist/`), electron:compile → `.electron/`, scripts documentados.
- **Estrutura:** `src/`, `components/`, `electron/`, `hooks/`, `lib/`, rotas atuais.
- **Providers com adapter real:** claude-code, codex, openai, anthropic, cursor (+ custom com fallback).
- **DB:** schema SQLite (providers, projects, missions, mission_logs), repositórios, tipos TS.
- **IPC:** electronAPI (dialog, shell, window, ai, git), db (providers, projects, missions, missionLogs, utils).
- **Fluxo de missão:** estados, plano, código, apply, logs.
- **UI:** Home, Project, Mission, Settings; dialogs (add/edit provider, add project, new mission, commit).
- **Migração Next.js → Vite + React Router** concluída.

### Pendente / a fazer

- Opcional: expandir lista completa de métodos na seção API IPC (referência a preload.ts já incluída).
- Itens já listados em "Melhorias de UX Planejadas" e "Pontos de Extensão" que ainda não foram implementados.

---

## Changelog

### v0.3.0 (AI Integrations)

- ✅ **Migração para Vite + React Router** (substituindo Next.js)
- ✅ **Integração real com Claude Code CLI** - Adapter completo
- ✅ **Integração real com OpenAI Codex CLI** - Adapter completo
- ✅ **Integração real com Cursor Agent CLI** - Adapter completo (binário `agent`, auth no terminal)
- ✅ **Integração real com OpenAI API** - Chamadas REST diretas (GPT-4)
- ✅ **Integração real com Anthropic API** - Chamadas REST diretas (Claude)
- ✅ **Git Service** - Detecção de branch, status, commits recentes
- ✅ **AI Orchestrator** - Gerenciamento centralizado de adapters
- ✅ **Aplicação real de diffs** - `git apply` (preferido) + fallback para escrita de arquivo com backup
- ✅ **Contexto de projeto** - Lista de arquivos e info Git enviada à IA
- ✅ Arquitetura de adapters extensível para novos providers
- ✅ IPC handlers para todas as operações de IA
- ✅ Fallback para mock no browser (dev sem Electron)

### v0.2.0 (Electron Update)

- ✅ Atualização para Electron ^33.0.0
- ✅ Atualização para better-sqlite3 ^11.8.0
- ✅ Suporte a Node.js 22+
- ✅ Melhoria na detecção de ambiente (dev/prod)
- ✅ Integração correta com app.getPath('userData')
- ✅ Scripts de build atualizados

### v0.1.0 (MVP)

- ✅ Estrutura base Electron + Vite + React
- ✅ Sistema de providers (CRUD completo)
- ✅ Sistema de projetos (CRUD completo)
- ✅ Sistema de missões com estados
- ✅ Geração de planos (mockada)
- ✅ Geração de código (mockada)
- ✅ Sistema de logs
- ✅ Persistência SQLite
- ✅ UI completa com sidebar, dialogs, tabs

---

## Licença

MIT License - Uso livre para projetos pessoais e comerciais.
