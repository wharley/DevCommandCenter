# Analise de viabilidade: adaptar o DCC para preservar a proposta atual do Claude

## Contexto

Este documento parte da premissa descrita no problema: o uso de Claude via SDK oficial e via `claude --print --output-format stream-json` passara a ser tratado como uso programatico, deixando de aproveitar o regime de assinatura interativa do Claude Pro/Team.

A analise abaixo nao valida a politica da Anthropic externamente. Ela responde a pergunta de engenharia com base no codigo atual do DCC e no impacto assumido dessa mudanca.

## Conclusao executiva

Sim, e possivel adaptar o DCC para continuar relevante para usuarios que ja pagam Claude Pro.

Nao, isso nao e resolvido apenas com ajuste de regra, prompt ou UX.

Se a premissa de custo estiver correta, o DCC so preserva sua proposta de valor migrando o runtime do Claude para um fluxo interativo via PTY, tratando o CLI original como sessao humana automatizada. Regra e UX ajudam como mitigacao temporaria, mas nao mantem o produto atual por si so.

## Resposta curta

O que da para fazer apenas com regra/UX:

- Alertar custo e risco no provider `claude_code`.
- Despriorizar ou bloquear o composer Claude para uso pesado.
- Redirecionar o usuario para o `Terminal Panel`.
- Introduzir um modo "Claude Interactive" visivel na UX.

O que nao da para resolver apenas com regra/UX:

- Fazer `ClaudeSdkSidecarAdapter` parar de ser programatico.
- Fazer `HeadlessCliProviderAdapter` com `--print --output-format stream-json` parar de ser programatico.
- Preservar o stream estruturado atual de reasoning, tool calls, approvals e user input sem trocar a arquitetura.

## Como o DCC esta acoplado hoje

### 1. O provider principal de Claude depende do sidecar + SDK

O provider exposto como `claude_code` usa `ClaudeSdkSidecarAdapter`, que sobe um processo sidecar Node e conversa por `stdin/stdout` com ele. Referencias:

- `crates/dcc-providers/src/claude_code.rs`
- `crates/dcc-providers/src/claude_sdk_sidecar.rs:27-239`

No sidecar, o DCC chama diretamente `query()` do pacote `@anthropic-ai/claude-agent-sdk`:

- `sidecar/src/index.mjs:331-368`

Esse sidecar nao so envia prompt e recebe texto. Ele tambem intercepta ferramentas do Claude e as converte em eventos proprios do DCC:

- `dcc_user_input_request` / `dcc_user_input_resolved`: `sidecar/src/index.mjs:207-266`
- `dcc_permission_request` / `dcc_permission_resolved`: `sidecar/src/index.mjs:268-326`
- captura de plano via `ExitPlanMode`: `sidecar/src/index.mjs:346-365`

Depois o parser comum do provider transforma esses envelopes em `ProviderEvent`:

- `crates/dcc-providers/src/common.rs:515-608`

### 2. O fallback/headless do Claude tambem depende de uso programatico

O adapter headless para Claude monta explicitamente:

- `--print`
- `--output-format stream-json`
- `--include-partial-messages`
- `--dangerously-skip-permissions`

Referencia:

- `crates/dcc-providers/src/headless_cli.rs:157-185`

Ou seja: mesmo sem o SDK, o caminho headless do DCC tambem esta no grupo de automacao programatica.

### 3. A UX central do DCC depende de eventos estruturados

O bridge de sessao em Tauri consome `ProviderEvent` e persiste tudo como eventos de sessao:

- `crates/dcc-tauri/src/state.rs:192-247`
- `crates/dcc-tauri/src/state.rs:251-420`

O frontend reconstrui a thread com anotacoes de:

- reasoning
- tool-call
- user-input
- approval

Referencia:

- `apps/desktop/src/features/sessions/session-thread-history.logic.ts:592-820`

Entao o valor atual do DCC para Claude nao e apenas "rodar Claude". E "rodar Claude com semantica de sessao e UX anotada".

### 4. O DCC ja possui a base de PTY

O backend Tauri ja usa `portable-pty` para terminais persistentes:

- spawn PTY: `src-tauri/src/main.rs:2609-2725`
- leitura, backlog e emissao de `terminal-output`: `src-tauri/src/main.rs:5647-5768`

Tambem ja existe deteccao de espera por input humano:

- `terminal-attention` / `terminal-activity`: `src-tauri/src/main.rs:5718-5758`

Isso reduz o custo de infraestrutura para um harness interativo.

### 5. O DCC ja possui configuracao de runtime por provider

O frontend e o core ja carregam `homePath` e `shadowHomePath` por provider:

- `apps/desktop/src/features/providers/provider-runtime-settings.ts:3-141`
- `crates/dcc-tauri/src/state.rs:90-113`
- `crates/dcc-tauri/src/state.rs:220-230`

Isso e importante porque um runtime interativo do Claude provavelmente vai precisar de isolamento de home, auth e continuidade de sessao.

## O que muda no produto se nada for feito

Se a mudanca de politica realmente classificar esses fluxos como programaticos, o problema nao e cosmetico.

Impactos diretos:

- o provider `claude_code` deixa de ser economicamente seguro para uso intenso;
- o fallback headless tambem deixa de ser uma alternativa economica;
- o usuario continua vendo no DCC uma UX rica, mas sustentada por um canal que pode consumir credito rapidamente;
- isso gera reclamacao de "o app me fez gastar meu plano", que e um problema de produto, nao apenas de implementacao.

## Regra e UX sozinhas resolvem?

## Nao resolvem o problema principal

Motivo:

- o problema esta no transporte/protocolo de execucao, nao no texto do prompt;
- o SDK e o `claude -p` continuam sendo chamados de forma automatizada;
- o DCC atual depende de stream estruturado vindo justamente desses caminhos.

## O que regra e UX conseguem fazer

Como mitigacao de curto prazo, sim:

- trocar o default do Claude para um modo explicitamente "light" ou "manual";
- mostrar badge de risco de custo no catalogo de provider;
- separar "Claude Structured" de "Claude Interactive";
- redirecionar o usuario pesado para o terminal embutido;
- exigir confirmacao antes de iniciar sessoes Claude no composer;
- reduzir o numero de turnos encadeados automaticos;
- desativar por default o provider Claude programatico em novas instalacoes.

Essas medidas diminuem dano e suporte, mas nao preservam a experiencia atual do DCC.

## Avaliacao das opcoes

## Opcao A: manter SDK/headless e ajustar UX

Vantagens:

- menor esforco imediato;
- quase nenhum impacto de arquitetura;
- protege parcialmente novos usuarios com avisos e defaults.

Desvantagens:

- nao resolve o custo;
- mantem a reclamacao estrutural;
- o DCC passa a depender de educacao do usuario para nao falhar economicamente.

Veredito:

- util apenas como hotfix temporario.

## Opcao B: desviar Claude para o Terminal Panel

Vantagens:

- aproveita o canal interativo que tende a continuar valido;
- infraestrutura de PTY ja existe;
- entrega valor rapido para usuarios avancados.

Desvantagens:

- perde a UX semantica do composer;
- o Claude fica "fora" do fluxo principal de sessao;
- approvals, reasoning e tool calls deixam de alimentar o historico estruturado.

Veredito:

- bom como fallback operacional de emergencia;
- insuficiente como substituto definitivo do provider atual.

## Opcao C: migrar Claude para harness interativo em PTY

Vantagens:

- alinha o produto ao caminho interativo;
- preserva a proposta economica para quem ja paga Claude;
- reaproveita a base de PTY do Tauri;
- permite convergir terminal e provider em uma unica runtime strategy.

Desvantagens:

- parser e controle de sessao ficam mais frageis;
- a UX estruturada atual nao sai "de graca";
- exige nova camada de orquestracao no backend.

Veredito:

- unica opcao estruturalmente correta se o objetivo e manter o DCC competitivo em Claude.

## O ponto central: migrar para PTY nao e trocar so o transporte

Hoje o DCC recebe eventos semanticos de alto nivel:

- `ReasoningStarted`
- `ToolCallStarted`
- `UserInputRequested`
- `PermissionRequested`
- `Completed`
- `Failed`

No modelo PTY puro, o que chega primeiro sao bytes ANSI.

Isso significa que a migracao real e:

1. sair de protocolo estruturado do SDK/JSON;
2. entrar em protocolo de terminal;
3. reconstruir a semantica necessaria para o produto.

Esse e o motivo pelo qual "regra/UX" nao basta.

## O que eu recomendo construir

## 1. Introduzir um runtime Claude interativo novo

Em vez de tentar encaixar PTY dentro do sidecar atual, a mudanca deve explicitar um novo runtime, por exemplo:

- manter o id `claude_code` e trocar internamente a engine; ou
- criar `claude_interactive` e depois promover para default.

Minha recomendacao:

- criar primeiro um runtime paralelo para reduzir risco;
- so depois promover para default.

## 2. Fazer o runtime interativo ser dono da sessao

Esse runtime deve:

- abrir um PTY para `claude`;
- manter a sessao viva por workspace/thread;
- escrever prompts no PTY;
- observar output e detectar:
  - inicio de resposta;
  - espera por input humano;
  - fim de turno;
  - erros;
- publicar `ProviderEvent` no maximo que for confiavel.

## 3. Comecar com um modelo de eventos degradado, mas honesto

Nao tente reconstruir toda a semantica do SDK no primeiro corte.

Primeiro corte viavel:

- `TextDelta`
- `Completed`
- `Failed`
- algum evento coarse de `waiting for input`

Segundo corte:

- heuristicas para approvals;
- heuristicas para tool execution;
- possivel identificacao de blocos de reasoning, se houver marcadores confiaveis.

Se a heuristica nao for confiavel, a UX deve assumir isso explicitamente.

## 4. Reusar `homePath` e `shadowHomePath`

Isso ja existe no DCC e deve virar parte do design do runtime Claude interativo:

- sessao compartilhada opcional;
- auth isolada por ambiente;
- shadow home para evitar vazar estado entre workspaces, quando necessario.

## O que pode ser adaptado no fluxo e na UX para manter valor

## Fluxo recomendado

### Curto prazo

- rotular o provider atual como `Claude (Structured, programmatic)` ou equivalente;
- adicionar um novo `Claude (Interactive, subscription-backed)` como recomendado;
- usar o terminal drawer como superficie de transparencia quando o runtime entrar em estado de espera;
- mostrar claramente quando a sessao esta em "interactive mode".

### Medio prazo

- integrar a sessao PTY ao mesmo thread/composer;
- quando o parser tiver baixa confianca, renderizar a resposta como stream textual simples;
- quando detectar prompt de aprovacao, abrir UX de approval ou focar o terminal com CTA claro.

## UX que eu recomendo

- Badge de modo:
  - `Interactive`
  - `Structured`
  - `Cost risk`

- Empty / warning states:
  - se o usuario selecionar Claude estruturado, mostrar aviso de consumo programatico;
  - se selecionar Claude interativo, explicar que a sessao usa o CLI original e pode ter menor granularidade de eventos.

- Fallback operacional:
  - se o parser falhar, continuar exibindo o stream bruto no thread e espelhar o PTY no drawer;
  - nao abortar a sessao so porque a semantica fina falhou.

Essa abordagem preserva o mais importante: confianca do usuario de que o DCC nao sabotou a assinatura dele.

## O que o DCC consegue manter e o que provavelmente vai perder

## Mantem

- uso do Claude dentro do DCC;
- sessao persistente por workspace;
- terminal embutido e backlog;
- possibilidade de experiencia baseada em assinatura interativa;
- historico de texto e continuidade operacional.

## Mantem parcialmente

- approvals em UX propria;
- detecao de bloqueio por input humano;
- resumibilidade de sessao.

## Provavelmente perde no primeiro corte

- reasoning estruturado com alta fidelidade;
- tool call tracking com granularidade equivalente ao SDK;
- confianca total no parser de estados do agente;
- paridade exata com o fluxo atual do composer Claude.

Isso nao invalida a migracao. So muda a ordem de entrega.

## Riscos tecnicos reais

1. Parsing ANSI e fragil.
   Mudancas no output do Claude podem quebrar heuristicas.

2. Deteccao de fim de turno e ambigua.
   Sem protocolo oficial, o runtime precisa definir criterios robustos.

3. Multi-plataforma.
   O PTY do Tauri ja ajuda, mas Claude pode se comportar diferente entre macOS, Linux e Windows.

4. Approval automation.
   Alguns prompts podem variar por versao, idioma ou contexto.

5. Resume de sessao.
   O sidecar hoje usa `resumeSessionId`; no modo PTY isso pode exigir outra estrategia.

## Recomendacao de entrega por fases

## Fase 0: mitigacao antes da migracao

- adicionar aviso de custo no provider atual;
- marcar `claude_code` atual como fluxo programatico;
- oferecer CTA para abrir Claude no terminal embutido;
- mudar default para um caminho nao surpreendente para o usuario.

## Fase 1: runtime Claude interativo minimo

- spawn de `claude` em PTY dedicado;
- envio de prompt por sessao;
- stream textual para o thread;
- deteccao de waiting/input;
- conclusao e erro de turno.

Meta:

- manter utilidade do DCC para assinantes Claude mesmo com UX degradada.

## Fase 2: recuperacao de UX

- mapear approvals para eventos do thread;
- mapear prompts de pergunta ao usuario;
- abrir drawer/overlay quando sessao exigir acao humana;
- persistir estado operacional da sessao interativa.

## Fase 3: semantica enriquecida

- heuristicas de tool call;
- possiveis marcadores adicionais por prompt systemico;
- unificacao de terminal e thread como duas views da mesma sessao.

## Decisao recomendada

Se a mudanca de custo/politica estiver correta, a prioridade deve ser:

1. proteger o usuario imediatamente por UX e defaults;
2. iniciar migracao para harness PTY interativo no backend Rust/Tauri;
3. tratar a perda de eventos estruturados como problema de produto a ser reconstruido por fases;
4. apos estabilizar o modo interativo, aposentar ou esconder o caminho SDK/headless para Claude.

## Posicionamento final

E possivel manter o DCC como boa solucao para usuarios Claude Pro, mas nao mantendo o fluxo atual intacto.

O que pode ser preservado:

- a proposta economica;
- o uso dentro do DCC;
- a persistencia de sessao;
- parte importante da UX.

O que inevitavelmente muda:

- o backend do Claude precisa sair de SDK/`claude -p` e ir para PTY interativo;
- a UX do composer precisa aceitar um periodo de "semantica parcial";
- terminal e sessao estruturada vao se aproximar arquiteturalmente.

Em termos de decisao de produto/engenharia:

- regra e UX sao mitigacao;
- PTY interativo e a solucao.
