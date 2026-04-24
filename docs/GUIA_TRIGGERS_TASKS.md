# Guia de Configuração: Triggers de Tasks

## Visão Geral

Os **Triggers de Tasks** permitem que você execute ações automatizadas quando uma task agendada completa. Por exemplo:
- Notificar IA quando testes falham
- Gerar resumos de builds
- Analisar logs de deploy

## Fluxo de Funcionamento

```
Task executada → Completa (exit code) → Avalia condição do trigger →
→ Envia prompt para IA → Recebe resposta → Log da execução
```

## Passo 1: Configurar um Provider de IA

Você precisa ter um provider configurado no banco de dados SQLite do DCC.

### Opção A: Anthropic Claude

```sql
INSERT INTO providers (id, provider_type, name, api_key, model)
VALUES (
  'claude-sonnet',
  'anthropic',
  'Claude 3.5 Sonnet',
  'sk-ant-api03-xxxxxxxxxxxxx',  -- Sua API key da Anthropic
  'claude-3-5-sonnet-20241022'
);
```

**Como obter API key:**
1. Acesse https://console.anthropic.com/
2. Crie uma conta ou faça login
3. Vá em "API Keys" e crie uma nova key
4. Copie a key que começa com `sk-ant-`

**Modelos disponíveis:**
- `claude-3-5-sonnet-20241022` (default, balanceado)
- `claude-3-5-haiku-20241022` (rápido, econômico)
- `claude-opus-4-20250514` (mais capaz, mais caro)

### Opção B: OpenAI GPT

```sql
INSERT INTO providers (id, provider_type, name, api_key, model)
VALUES (
  'gpt-4',
  'openai',
  'GPT-4',
  'sk-proj-xxxxxxxxxxxxx',  -- Sua API key da OpenAI
  'gpt-4'
);
```

**Como obter API key:**
1. Acesse https://platform.openai.com/
2. Faça login e vá em "API Keys"
3. Crie uma nova key
4. Copie a key que começa com `sk-`

**Modelos disponíveis:**
- `gpt-4` (default, mais capaz)
- `gpt-4-turbo` (mais rápido, mais barato)
- `gpt-3.5-turbo` (rápido, econômico)

### Opção C: LLM Local ou Proxy

Para usar um LLM auto-hospedado (ex: LiteLLM, Ollama via OpenAI API):

```sql
INSERT INTO providers (id, provider_type, name, api_key, base_url, model)
VALUES (
  'local-llm',
  'openai',  -- Usa formato de API OpenAI
  'LiteLLM Local',
  'sk-local',
  'http://localhost:8000',  -- URL do seu servidor
  'gpt-3.5-turbo'
);
```

---

## Passo 2: Configurar Task com Trigger

Edite o arquivo `.dcc.toml` no diretório raiz do seu projeto:

### Exemplo 1: Trigger quando testes falham

```toml
[[tasks]]
id = "ci-tests"
name = "Run CI Tests"
command = "npm run test:ci"
schedule = "0 */4 * * *"  # A cada 4 horas
enabled = true

[tasks.trigger]
when = "failure"  # Dispara apenas se falhar
prompt = """
Os testes de CI falharam.

Comando: {{command}}
Exit code: {{exit_code}}
Output: {{output}}

Analise o erro e sugira uma correção.
"""
provider_id = "claude-sonnet"
```

### Exemplo 2: Resumo de build (sempre)

```toml
[[tasks]]
id = "nightly-build"
name = "Build Noturno"
command = "npm run build"
schedule = "0 2 * * *"  # 2h da manhã todo dia

[tasks.trigger]
when = "complete"  # Sempre dispara (sucesso ou falha)
prompt = """
Build noturno completou com status: {{status}}

Exit code: {{exit_code}}
Output: {{output}}

Gere um resumo executivo de 2-3 linhas para o time.
"""
provider_id = "gpt-4"
```

### Exemplo 3: Deploy bem-sucedido

```toml
[[tasks]]
id = "deploy-staging"
name = "Deploy para Staging"
command = "./scripts/deploy.sh staging"
schedule = "0 14 * * 1-5"  # 14h em dias úteis

[tasks.trigger]
when = "success"  # Apenas quando sucesso
prompt = """
Deploy para staging concluído com sucesso!

Gere uma checklist de QA para validar o deploy.
"""
provider_id = "claude-sonnet"
```

### Exemplo 4: Teste simples para debug

```toml
[[tasks]]
id = "test-trigger"
name = "Test Trigger"
command = "exit 1"  # Força falha para testar
schedule = "*/2 * * * *"  # A cada 2 minutos

[tasks.trigger]
when = "failure"
prompt = "Task {{task_name}} falhou com código {{exit_code}}. Output: {{output}}"
provider_id = "claude-sonnet"
```

---

## Passo 3: Verificar Logs do Daemon

Após configurar, o daemon executará as tasks conforme o schedule. Quando um trigger disparar, você verá logs como:

```
[DCC][trigger] Task 'Run CI Tests' trigger executado: provider=claude-sonnet when=failure response="O erro ocorreu porque o módulo 'validator' não foi encontrado. Sugiro executar 'npm install' para instalar as dependências... (truncated)"
```

### Como ver logs:

**Se o daemon está rodando como sidecar:**
```bash
# Logs aparecem no console do Tauri
```

**Se o daemon está rodando standalone:**
```bash
# Executar daemon manualmente
/caminho/para/dccd

# Logs aparecem no terminal
```

---

## Variáveis Disponíveis no Prompt

Você pode usar estas variáveis nos templates de prompt:

| Variável | Descrição | Exemplo de Valor |
|----------|-----------|------------------|
| `{{task_name}}` | Nome da task | "Run CI Tests" |
| `{{command}}` | Comando executado | "npm run test:ci" |
| `{{exit_code}}` | Código de saída | "0" (sucesso) ou "1" (erro) |
| `{{output}}` | Última linha do output (max 240 chars) | "✓ All tests passed!" |
| `{{status}}` | Status legível | "success" ou "failure" |

**Exemplo de uso:**
```toml
prompt = """
A task '{{task_name}}' terminou com status {{status}}.

Detalhes:
- Comando: {{command}}
- Exit code: {{exit_code}}
- Output: {{output}}

O que isso significa?
"""
```

---

## Condições do Trigger (`when`)

O campo `when` determina quando o trigger será executado:

| Valor | Descrição | Executa quando |
|-------|-----------|----------------|
| `"success"` | Apenas em sucesso | exit_code == 0 |
| `"failure"` | Apenas em falha | exit_code != 0 |
| `"complete"` | Sempre | Qualquer exit code |

**Exemplo:**
```toml
[tasks.trigger]
when = "failure"  # Só executa se a task falhar
```

---

## Configuração de Schedule (Cron)

O campo `schedule` usa sintaxe cron com **5 ou 6 campos**:

### Formato 5 campos (minuto, hora, dia, mês, dia da semana)
```
┌───────────── minuto (0-59)
│ ┌───────────── hora (0-23)
│ │ ┌───────────── dia do mês (1-31)
│ │ │ ┌───────────── mês (1-12)
│ │ │ │ ┌───────────── dia da semana (0-6, 0=Domingo)
│ │ │ │ │
* * * * *
```

### Formato 6 campos (segundo, minuto, hora, dia, mês, dia da semana)
```
┌───────────── segundo (0-59)
│ ┌───────────── minuto (0-59)
│ │ ┌───────────── hora (0-23)
│ │ │ ┌───────────── dia do mês (1-31)
│ │ │ │ ┌───────────── mês (1-12)
│ │ │ │ │ ┌───────────── dia da semana (0-6)
│ │ │ │ │ │
* * * * * *
```

### Exemplos comuns:

```toml
# A cada minuto
schedule = "* * * * *"

# A cada 2 minutos
schedule = "*/2 * * * *"

# A cada hora (no minuto 0)
schedule = "0 * * * *"

# Todo dia às 2h da manhã
schedule = "0 2 * * *"

# De segunda a sexta às 14h
schedule = "0 14 * * 1-5"

# Primeiro dia do mês às 9h
schedule = "0 9 1 * *"

# A cada 30 segundos (6 campos)
schedule = "*/30 * * * * *"
```

**Ferramentas úteis:**
- https://crontab.guru/ (visualizar expressões cron)

---

## Solução de Problemas

### Trigger não está executando

1. **Verifique se a task está habilitada:**
   ```toml
   enabled = true  # Deve estar true ou omitido (default é true)
   ```

2. **Verifique se o provider existe no banco:**
   ```sql
   SELECT * FROM providers WHERE id = 'seu-provider-id';
   ```

3. **Verifique se a condição `when` está correta:**
   - `"success"` só dispara se exit_code == 0
   - `"failure"` só dispara se exit_code != 0
   - `"complete"` sempre dispara

4. **Veja os logs do daemon:**
   - Se houver erro, aparecerá: `[DCC][trigger] Erro ao executar trigger para task '...'`

### Erro de autenticação (401/403)

```
[DCC][trigger] Erro: Anthropic API erro 401: {"error":{"type":"authentication_error"}}
```

**Solução:** API key inválida ou expirada. Verifique:
```sql
SELECT api_key FROM providers WHERE id = 'seu-provider-id';
```

Atualize a key se necessário:
```sql
UPDATE providers SET api_key = 'sk-ant-nova-key' WHERE id = 'seu-provider-id';
```

### Erro de parsing de response

```
[DCC][trigger] Erro: Response sem campo 'content[0].text'
```

**Solução:** A API retornou formato inesperado. Possíveis causas:
- Modelo inválido
- Base URL incorreta
- Provider type errado (ex: usar "anthropic" mas base_url de OpenAI)

### Task não está executando no schedule

1. **Verifique se o daemon está rodando:**
   ```bash
   # Ver processos do daemon
   ps aux | grep dccd
   ```

2. **Verifique o schedule cron:**
   - Use https://crontab.guru/ para validar a expressão

3. **Verifique o campo `next_run_at` no banco:**
   ```sql
   SELECT task_id, task_name, next_run_at, status
   FROM daemon_task_runs
   WHERE project_id = 'seu-project-id';
   ```

---

## Exemplos Avançados

### 1. Análise de cobertura de testes

```toml
[[tasks]]
id = "coverage-report"
name = "Test Coverage Report"
command = "npm run test:coverage"
schedule = "0 3 * * *"

[tasks.trigger]
when = "complete"
prompt = """
Relatório de cobertura de testes:

{{output}}

Analise a cobertura e sugira áreas que precisam de mais testes.
Se a cobertura caiu, identifique quais arquivos perderam cobertura.
"""
provider_id = "claude-sonnet"
```

### 2. Monitoramento de API externa

```toml
[[tasks]]
id = "api-health-check"
name = "API Health Check"
command = "curl -s -o /dev/null -w '%{http_code}' https://api.example.com/health"
schedule = "*/5 * * * *"

[tasks.trigger]
when = "failure"
prompt = """
API health check falhou!

Status code: {{output}}
Exit code: {{exit_code}}

A API está fora do ar. Gere um plano de ação imediato:
1. Verificar status do servidor
2. Verificar logs
3. Notificar equipe
"""
provider_id = "gpt-4"
```

### 3. Limpeza de cache com confirmação

```toml
[[tasks]]
id = "cache-cleanup"
name = "Clean Cache"
command = "rm -rf /tmp/app-cache/* && echo 'Cache cleared'"
schedule = "0 0 * * 0"  # Domingo à meia-noite

[tasks.trigger]
when = "success"
prompt = """
Cache limpo com sucesso.

Output: {{output}}

Gere um relatório de quanto espaço foi liberado e se a limpeza foi necessária.
"""
provider_id = "claude-haiku"
```

---

## Boas Práticas

### 1. Prompts Específicos e Acionáveis

❌ **Ruim:**
```toml
prompt = "A task falhou."
```

✅ **Bom:**
```toml
prompt = """
A task '{{task_name}}' falhou com exit code {{exit_code}}.

Output: {{output}}

Por favor:
1. Identifique a causa raiz do erro
2. Sugira uma correção específica
3. Indique se é necessário intervenção humana
"""
```

### 2. Escolha o Provider Adequado

- **Claude Sonnet**: Tarefas complexas de análise
- **Claude Haiku**: Resumos rápidos e simples
- **GPT-4**: Raciocínio profundo
- **GPT-3.5 Turbo**: Tarefas simples e econômicas

### 3. Use `when` Estrategicamente

- `"failure"`: Para notificações de erro (menos ruído)
- `"success"`: Para confirmações e relatórios
- `"complete"`: Para análise independente de sucesso/falha

### 4. Limite o Tamanho dos Prompts

O output está limitado a 240 caracteres. Se precisar de mais contexto:
- Peça para a IA focar em aspectos específicos
- Use múltiplas tasks com triggers diferentes
- Considere salvar logs completos em arquivo e passar caminho

### 5. Segurança de API Keys

⚠️ **IMPORTANTE:** API keys são atualmente armazenadas em texto plano no SQLite.

- **NÃO** compartilhe o arquivo do banco de dados
- **NÃO** commite o banco no Git
- Use variáveis de ambiente quando possível (futuro recurso)

---

## Roadmap Futuro

Melhorias planejadas para Fase 2:

- [ ] Tabela `trigger_executions` para histórico persistente
- [ ] UI para ver execuções de triggers
- [ ] Retry automático com backoff exponencial
- [ ] Suporte para Google AI, Ollama, Azure OpenAI
- [ ] Criptografia de API keys com keychain/credential manager
- [ ] Output completo (múltiplas linhas) disponível para triggers
- [ ] Webhooks (enviar resultado para Slack/Discord)
- [ ] Triggers encadeados (trigger A → executa task B)
- [ ] Streaming de respostas longas

---

## Suporte

Se encontrar problemas ou tiver sugestões:
1. Verifique os logs do daemon
2. Valide a configuração do `.dcc.toml`
3. Teste o provider manualmente (curl para a API)
4. Abra uma issue no repositório

---

**Última atualização:** 2026-04-11
**Versão do DCC:** 0.1.0
**Status:** ✅ MVP Completo (Anthropic + OpenAI)
