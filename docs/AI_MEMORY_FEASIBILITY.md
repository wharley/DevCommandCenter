# ai-memory no DCC: viabilidade e decisão de integração

Data: 17/09/2026. Status: análise, smoke de protocolo e adaptador experimental concluídos; prova com uma sessão real ainda pendente.

## Decisão recomendada

Avançar com **memória de domínio controlada pelo DCC, usando ai-memory como motor substituível**.
Há evidência suficiente para uma prova integrada. Ainda não há evidência para distribuir o motor
como dependência obrigatória nem prometer continuidade correta em todos os providers.

Não começar por um fork, por incorporar todas as crates Rust ou por reconstruir busca e wiki.
Também não reduzir o produto a cadastrar um servidor MCP: isso oferece ferramentas, mas não garante
captura, recuperação automática, isolamento de tarefas ou relevância do contexto.

Prioridade: continuidade entre sessões → troca/retomada com memória → avaliar coordenação entre
providers com casos reais. A orquestração nativa continua independente.

## Evidência e escopo

- DCC examinado: `9494ab95eceaed8426481f144927e49913b06df2`.
- O código-fonte examinado anuncia a linha **2.3.0**, commit `a200127c5866e23f6068823091111c01de8f49d5`.
  A release publicada mais recente disponível para instalação é a **v2.2.2**; o piloto deve usar
  essa release até que a v2.3.0 seja publicada.
- Histórico recuperado: tarefa **Avaliar alternativa ao Delegar**, incluindo a discussão sobre
  priorizar memória e testar Claude → nova sessão Codex, preservando Local/Worktree.
  Isso também está registrado em [SKILL_PRESETS.md](SKILL_PRESETS.md#next-shared-memory).
- Código upstream clonado para inspeção; binário oficial macOS ARM64 executado fora do projeto.
- SHA-256 do arquivo `ai-memory-macos-aarch64.tar.gz` conferido com o checksum da release:
  `472e690405928e348c517f2d789d6c873fdf042c5d76e44b741ad7a74216f8a4`.
- Nenhuma CLI de agente ou modelo foi executado no smoke. Nenhuma credencial foi herdada.
  Sem instalação de hooks, alterações de configuração de providers ou migração de conversas reais.

## O que já existe no DCC

| Peça atual | Consequência para memória |
|---|---|
| `SessionEventRecord`, sequência, identificador de evento e SQLite | Base para captura independente do provider e reprocessamento idempotente |
| Projeto, workspace, sessão e diretório efetivo | Identidade mais confiável que nome de pasta ou “último projeto ativo” |
| Planos, objetivos persistentes, revisões e resultados | Fontes para separar intenção, execução e evidência de validação |
| Handoff com limite de 12.000 caracteres | Já resolve parte da troca dentro da sessão; não recupera por si só decisões antigas de outras sessões |
| Re-anchor de cold attach no backend | Ponto existente de recomposição de contexto ao retomar um runtime |
| Bridges MCP por provider | Podem ser usados no teste, respeitando o contrato real de cada runtime |
| Histórico e busca de sessões | Devem continuar sendo evidência original, não ser substituídos pela wiki |

Fontes locais: [session.rs](../crates/dcc-core/src/domain/session.rs),
[db.rs](../crates/dcc-infra/src/db.rs), [state.rs](../crates/dcc-tauri/src/state.rs),
[provider-handoff-context.ts](../apps/desktop/src/features/sessions/provider-handoff-context.ts),
[PROVIDER_HANDOFF.md](PROVIDER_HANDOFF.md).

O DCC usa Claude pelo Agent SDK e Codex por `app-server`. A presença de hooks/MCP na CLI não
comprova funcionamento nessas rotas. O sidecar Claude carrega configurações user/project/local e
registra hooks próprios; é necessário verificar composição e duplicação com hooks externos.
O fluxo Codex também exige uma prova pelo adaptador de produção.
Fontes: [sidecar](../sidecar/src/index.mjs), [Codex](../crates/dcc-providers/src/codex_app_server.rs).

## Onde aproveitar ai-memory

| Capacidade | Aproveitamento | Limite da conclusão |
|---|---|---|
| HTTP `/hook` com identidade e `ingest_key` | Exportar eventos selecionados do backend | Aceitar HTTP 202 não equivale a conclusão durável de todos os efeitos |
| Wiki Markdown com versões Git | Conhecimento inspecionável e exportável | Não substitui histórico e metadados operacionais do DCC |
| Busca textual, entidades e relações; vetores opcionais | Evitar construir um motor de recuperação inicialmente | Qualidade em português e consultas reais ainda não medida |
| Página de sessão sem LLM | Baseline local, sem chave nem custo de inferência | Síntese heurística não é extração confiável de decisões |
| Consolidação opcional por LLM | Melhorar síntese quando houver ganho mensurável | Custo, privacidade e correção precisam de avaliação própria |
| Briefing, leitura da página e observações | Pacote inicial pequeno e consulta da evidência sob demanda | Exige política de relevância e orçamento do DCC |
| Handoffs identificados e consumíveis | Referência útil para retomada explícita | Não usar uma fila consumível como única memória compartilhada |
| Sanitização, retenção e operação | Reaproveitar mecanismos existentes | Regras de captura do DCC devem agir antes de exportar |

Fontes upstream: [arquitetura](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/docs/ARCHITECTURE.md),
[contratos MCP](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/crates/ai-memory-mcp/src/server.rs),
[payload de captura](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/crates/ai-memory-hooks/src/payload.rs),
[síntese sem LLM](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/crates/ai-memory-hooks/src/synth.rs).

## Lacunas que exigem implementação no DCC

### 1. Projeto não significa tarefa, branch ou checkout

`memory_query` permite projeto/workspace e múltiplos escopos, mas não oferece filtro nativo de
branch, tarefa, worktree ou tags. No smoke, duas páginas com tags de branches distintas foram
retornadas pela mesma busca de projeto. Tags não fornecem isolamento.

`project_strategy = "repo-root"` agrupa worktrees do mesmo repositório; isso resolve identidade
compartilhada, não a validade de uma informação para uma branch. O `cwd` dos handoffs automáticos
também não representa commit, merge ou resultado de teste.

Proposta: manter uma memória de projeto para decisões aplicáveis ao conjunto e memória de tarefa
para trabalho provisório. No primeiro adaptador, mapear essas camadas para projetos upstream
separados, consultados pelo backend apenas quando autorizados. Usar IDs estáveis do DCC, com tabela
de mapeamento, e não basenames. O “workspace” upstream é um namespace e não precisa corresponder
ao workspace Git do DCC.

O registro do DCC deve guardar, conforme aplicável: projeto, tarefa, workspace, provider, modelo,
branch, commit observado, impressão do estado sujo, eventos de origem, versão, estado de validação,
escopo de aplicação e relação `supersedes`. Uma afirmação validada no worktree não vira fato de
`main` até haver evidência de integração. Renomear branch ou remover worktree não deve perder a
origem histórica. Promoção para memória de projeto precisa de critério explícito.

Fontes: [QueryArgs/WritePageArgs](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/crates/ai-memory-mcp/src/server.rs),
[worktrees](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/docs/marker-file.md#git-worktrees--repo-root-identity).

### 2. Captura não equivale a entendimento

O caminho padrão não persiste a resposta final do assistente. Claude/Codex têm uma opção de
captura com consentimento dos dois lados e limite de 2 KB. Uma decisão que apareceu apenas nessa
resposta pode ficar ausente. O smoke confirmou a ausência padrão.

Capturar deltas de streaming também produziria duplicação e texto incompleto. Preferir mensagens
concluídas e resultados selecionados; distinguir proposta, aprovação, teste executado e alegação
do agente. Extrair um candidato não o torna automaticamente regra válida. Preservar links para
eventos originais. Excluir raciocínio interno, credenciais e anexos brutos por padrão.

`TurnCompleted` corresponde ao fim de um turno, não necessariamente ao `SessionEnd` upstream.
Definir sessões de captura por execução/anexo de runtime e checkpoints entre turnos; interrupção,
troca de provider e fechamento precisam de transições próprias. Um mapa de IDs deve conectar
sessão DCC, execução de captura e sessão upstream.

Fontes: [captura do assistente](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/crates/ai-memory-hooks/src/assistant_capture.rs),
[vocabulário dos hooks](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/crates/ai-memory-hooks/src/payload.rs).

### 3. Roteamento não equivale a autorização

O ai-memory é single-tenant: usuários autenticados compartilham as páginas da wiki. Atribuição
por pessoa e ownership de handoffs não são permissões por projeto. A busca `global=true` existe.
O isolamento observado no smoke é de consultas com escopo explícito, não uma prova de ACL.

Um agente com acesso direto ao servidor amplo pode solicitar outros escopos. Portanto, para
memória automática, o backend deve resolver e impor o escopo, sem confiar em argumentos livres
do modelo. Projetos de clientes com fronteiras reais de confiança precisam de instâncias separadas
ou controle de acesso efetivo. Cabeçalhos de sessão ajudam no roteamento, não resolvem isso sozinhos.

Fontes: [usuários](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/docs/users.md),
[auto-scope](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/docs/auto-scope.md).

### 4. Memória recuperada é evidência histórica

Nem sanitização, ranking, pinning ou um namespace `rules` tornam texto uma instrução autorizada.
O compilador de contexto deve rotular fonte, data e escopo, respeitar a mensagem atual e os
arquivos canônicos do projeto. Correções devem substituir o fato anterior preservando sua origem.
Memória de teste anterior não comprova que o código atual passa. Validar também instruções
maliciosas incorporadas em páginas durante a prova integrada.

O upstream já declara essa fronteira nas instruções MCP. A implementação do produto deve
preservá-la: não copiar toda a wiki para `AGENTS.md` nem promover resumo a instrução de sistema.
Fonte: [instruções MCP](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/crates/ai-memory-mcp/src/server.rs).

### 5. Operação, exclusão e distribuição

A reconstrução a partir de Markdown recupera páginas/links/FTS, mas não sessões, observações,
handoffs, usuários, auditoria ou embeddings. Backup deve abranger o estado necessário nos dois
sistemas. Remover uma página não apaga o Git e backups anteriores. A UI deve distinguir excluir
uma conversa, arquivar uma tarefa, remover conhecimento derivado e purgar dados.

O modo automático de melhoria pode aprovar alterações por padrão quando há LLM configurado.
Para a primeira integração, desabilitar esse scheduler; revisão de candidatos deve ser explícita.
Não permitir que esse processo reescreva regras canônicas do projeto.

Um serviço por diretório de dados; lifecycle, autenticação local, versão fixada, atualizações,
assinatura/empacotamento e recuperação são responsabilidade de distribuição. macOS nativo existe;
Windows nativo está declarado experimental. A licença upstream é MIT: ao redistribuir partes ou
binário, incluir o aviso de copyright e o texto da licença entre os avisos distribuídos.

Fontes: [lifecycle](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/docs/lifecycle-ops.md),
[auto-improvement](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/docs/auto-improvement-loop.md),
[plataformas](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/docs/support-matrix.md),
[licença](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/LICENSE).

## Arquitetura proposta

```text
Eventos duráveis + objetivos + revisões do DCC
                 │
        seleção / sanitização / outbox
                 │
       serviço de memória de domínio
       ├── identidade, origem, validade e autorização DCC
       └── MemoryBackend → adaptador ai-memory HTTP/MCP
                                      │
                           wiki + índices próprios

Nova sessão / troca de provider / consulta explícita
                 │
        resolver escopo no backend
                 │
     buscar + validar aplicabilidade + limitar contexto
                 │
       pacote com fontes → runtime selecionado
```

`MemoryBackend` é uma proposta, não uma interface existente. Operações mínimas: ingerir,
consultar, ler origem, registrar/substituir conhecimento, excluir e diagnosticar disponibilidade.

A fila deve usar identidade estável derivada do evento, retry limitado e reconciliação. Como
`/hook` responde 202 antes do processamento, não marcar a exportação como concluída apenas pela
aceitação HTTP. Validar o mecanismo de confirmação antes de prometer ausência de perda. Replays
podem repetir efeitos derivados: a deduplicação de observações não prova exactly-once global.

Priorizar captura a partir do histórico persistido no backend. Não depender exclusivamente do
renderer nem instalar hooks globais. Escolher um único dono da captura para evitar capturar duas
vezes pelo DCC e pela CLI. Não trocar o launcher do DCC por `ai-memory run`: seus workstreams
gerenciados se sobreporiam à gestão de runtimes e worktrees que já existe.

O backend pode usar MCP internamente sem expor todas as 23 ferramentas ao modelo. Uma futura
ferramenta DCC de consulta terá escopo vinculado à sessão. Isso é uma API de memória de domínio;
não exige substituir o modelo de bridges diretos por um gateway MCP genérico, que o
[ADR existente](ADR_EXTERNAL_MCP_INTEGRATIONS.md) deliberadamente não adotou.

O primeiro pacote deve complementar o re-anchor atual e evitar duplicá-lo. Orçamento próprio e
deadline; falha de memória permite o turno seguir, com estado de indisponibilidade distinto de
“não encontramos histórico”. Mensagem atual, estado real do Git e permissões continuam prevalecendo.

Implementação experimental entregue: o cliente HTTP/MCP está em
[`crates/dcc-infra/src/ai_memory.rs`](../crates/dcc-infra/src/ai_memory.rs), a transformação dos
eventos e o envio estão em `SessionCommandState`, e os comandos opt-in são
`sync_session_to_ai_memory` e `query_ai_memory`. O cliente desktop os expõe em
[`apps/desktop/src/lib/session-api.ts`](../apps/desktop/src/lib/session-api.ts). A exportação usa
`/hook/batch`, chaves idempotentes derivadas de `session_id/event_id`, escopo explícito e ignora
raciocínio, deltas e chamadas de ferramenta. Ela não altera o fluxo normal de turnos.

O fluxo atual mantém uma outbox persistida no SQLite: o fechamento registra a sessão antes da
tentativa de rede, faz uma tentativa imediata e o worker do DCC repete pendências a cada 30 segundos
com backoff limitado. O comando explícito continua disponível para diagnóstico e reprocessamento.
O disjuntor da recuperação automática abre após três falhas consecutivas por endpoint e fecha depois
de 30 segundos. O cabeçalho do workbench mostra quando a sessão ainda está pendente ou aguardando
retry; sem pendência, o indicador fica oculto para não sugerir que memória está habilitada quando a
variável de ambiente não foi configurada.

### Nova tarefa e recuperação de memória

Uma tarefa nova deve consultar memória quando ela pertence ao mesmo projeto/workspace e o
`DCC_AI_MEMORY_URL` está configurado. Isso já ocorre no primeiro `send_turn`: o backend usa o prompt
atual como consulta, resolve o escopo a partir da sessão e injeta somente um pacote de evidências
limitado, com origem e rótulo de histórico. Não é necessário copiar a conversa anterior, abrir o
MCP manualmente ou chamar um comando de sincronização antes de começar a tarefa.

O fechamento da tarefa anterior é o que torna seus eventos exportáveis; a tarefa nova apenas lê o
índice. Se a sessão nova estiver em outro projeto/workspace, ela não deve receber os fatos do
projeto anterior. Se o servidor estiver indisponível, o turno segue sem contexto de memória e a
falha aparece no log; a outbox da sessão anterior continua tentando a exportação. Portanto, a
recuperação automática é importante para o valor do DCC, mas deve ser tratada como evidência
opcional, nunca como requisito para criar ou executar uma tarefa.

## Teste executado nesta análise

[Script reproduzível](../scripts/ai-memory-feasibility.py), [resultado histórico do código 2.3.0 em desenvolvimento](AI_MEMORY_FEASIBILITY_RESULTS.json) e [resultado atualizado da release publicada 2.2.2](AI_MEMORY_FEASIBILITY_RESULTS_2_2_2.json). O smoke histórico passou 22 verificações; a validação adicional da release 2.2.2 passou **27 verificações**, incluindo o caminho `/hook/batch`:

- inicialização, descoberta das 23 ferramentas, escrita e recuperação textual;
- separação de projetos com escopo explícito, incluindo 20 leituras concorrentes;
- erro ao consultar projeto inexistente;
- ausência de filtro de branch e retorno conjunto de duas páginas etiquetadas por branch;
- ingestão de hooks, retry da mesma chave sem duplicar a observação e página de sessão sem LLM;
- remoção de uma chave de API fictícia do conteúdo consultado;
- ausência padrão da resposta do assistente;
- criação/aceitação de handoff e remoção da lista de pendentes;
- `/hook/batch` com três eventos, acknowledgement contíguo e retry idempotente;
- indisponibilidade com servidor parado e persistência após reinício;
- existência da página Markdown.

Os checks de ausência/falta de filtro confirmam limites, não funcionalidades desejadas. O teste
usa um cliente Python, escopos fictícios e identificação de origem `claude-code`; isso não significa
que Claude ou Codex foram executados. Não testa worktrees reais, UI, embeddings, qualidade de
síntese, exclusão completa, crash durante escrita, ACL, latência representativa ou consumo de RAM.

Para reproduzir com o binário oficial v2.2.2:

```sh
python3 scripts/ai-memory-feasibility.py --binary /caminho/ai-memory --expected-version 2.2.2
```

Esse binário passou 27 verificações e anuncia 19 ferramentas MCP. O smoke anterior contra o código
2.3.0 em desenvolvimento anunciou 23 ferramentas. O adaptador do DCC usa apenas `/hook/batch` e
`memory_query`, presentes nas duas linhas.

O script cria HOME e armazenamento temporários, não herda credenciais e encerra o servidor ao sair.

## Teste integrado do adaptador DCC

## Modelo operacional antes da próxima fase

O modo manual continua disponível: o usuário instala o binário, inicia `ai-memory serve` em um
terminal e o DCC aponta para `http://127.0.0.1:49374`. Ele serve para validar o contrato e depurar
uma versão diferente; a experiência normal usa o sidecar gerenciado pelo DCC.

O primeiro incremento do sidecar já está disponível: `yarn dev` procura um binário já instalado (por
exemplo, em `~/Applications/ai-memory/ai-memory` ou no `PATH`), define o caminho de desenvolvimento
e inicia o DCC com o sidecar ativo. Ele não baixa nem extrai binários durante o desenvolvimento.
As variáveis `DCC_AI_MEMORY_AUTO_START` e `DCC_AI_MEMORY_BIN` continuam aceitas para sobrescrever
esse comportamento. O DCC executa `init`, inicia o servidor em loopback, aguarda o health-check e
encerra o processo ao sair. Em uma build de release que contém o binário empacotado, o sidecar inicia
por padrão; `DCC_AI_MEMORY_DISABLE=1` desliga esse comportamento. Se
`DCC_AI_MEMORY_URL` já estiver definido, o sidecar não é iniciado e o DCC usa esse servidor (modo
remoto/manual). O download verificado e a inclusão no instalador do DCC são feitas durante
`yarn build` para releases.

O adaptador também divide automaticamente exportações maiores que 256 eventos, preserva os índices
globais do lote e usa timeout de 500 ms na recuperação automática. A fila persistente já está ativa;
a consulta automática e o worker de exportação abrem disjuntores independentes após três falhas por
endpoint e aguardam 30 segundos antes de tentar novamente. A UX básica da fila mostra pendências e o
próximo retry no cabeçalho; o instalador assinado continua como critério de endurecimento.

O primeiro uso também tem uma superfície própria em **Configurações → Memória do DCC**. Ela consulta
somente o estado já mantido pelo processo principal e informa se o modo é `managed`, `remote`,
`disabled` ou `unavailable`, além do endpoint, versão, diretório de dados e log. A tela explica que
o sidecar é um único processo ocioso entre operações, que a consulta automática tem timeout curto e
que o contexto é limitado a 4.000 caracteres. Atualizar o painel não inicia um novo servidor nem
baixa arquivos; em desenvolvimento o binário continua sendo procurado apenas por `yarn dev` nos
caminhos já documentados. Isso cobre descoberta e diagnóstico sem criar uma rotina de polling pesado.

Exemplo para testar sem manter um terminal do ai-memory aberto:

```bash
DCC_AI_MEMORY_DATA_DIR="$HOME/Library/Application Support/DCC/ai-memory" \
DCC_AI_MEMORY_WORKSPACE=dcc-pilot \
DCC_AI_MEMORY_PROJECT=dev-command-center \
yarn dev
```

Na configuração normal, basta instalar o `ai-memory` uma vez e executar `yarn dev`. O script injeta
as variáveis de inicialização automaticamente; as opções acima servem apenas para escolher outro
diretório ou escopo.

O desenho recomendado para o produto é um **sidecar local gerenciado pelo DCC**. O primeiro
incremento já cobre o ciclo de vida básico quando o binário está presente: cria o diretório de dados,
executa `init`, inicia `ai-memory serve` em loopback, faz health-check e encerra o filho quando o
aplicativo fecha. A release também já empacota uma versão fixada do binário, portanto o usuário não
precisa manter um terminal aberto. Ainda falta o instalador/atualizador que baixe uma nova versão em
tempo de execução, verifique assinatura, faça backup/reindex e reinicie com backoff após crash.
Deve existir também um modo **remoto**, em que o DCC não inicia processo algum e usa um URL
configurado para um servidor compartilhado; e um modo **desligado**, que mantém o DCC funcionando
sem memória externa.

O servidor deve rodar em modo zero-LLM por padrão. A captura, o SQLite/FTS5 e a consulta textual não
exigem API de modelo nem embeddings; esses recursos ficam opt-in porque alteram custo, CPU e
privacidade. O DCC continua sendo a fonte da sessão e trata o ai-memory como índice/cópia
reconstruível. Se o processo estiver indisponível, o DCC registra o diagnóstico, mantém a conversa
funcionando; a exportação fica na outbox e pode ser repetida pelo comando explícito de sincronização.
O instalador assinado e a UX de reprocessamento manual ainda são critérios de endurecimento antes de
declarar o fluxo pronto para produção. A memória nunca pode impedir um turno ou o fechamento da
sessão.

### Jev/TypeSafe: complementar, não substituto

O hype recente em torno do Jev descreve outra camada do sistema. A documentação oficial define o Jev
como um modelo para avaliar um `state` contra perguntas tipadas e devolver escolhas, scores,
probabilidades e confiança; ele aceita texto/JSON, mas não gera respostas, explicações ou código.
Também não oferece armazenamento, indexação ou recuperação entre sessões. Portanto, ele não substitui
o ai-memory nem o provider generativo que responde ao usuário no DCC.

Existe uma combinação interessante para uma fase posterior: o ai-memory continua armazenando e
recuperando observações localmente, enquanto o Jev poderia classificar se um evento merece ser salvo,
detectar risco/sensibilidade, reordenar candidatos ou decidir quando encaminhar uma ação para revisão.
Essas chamadas enviariam dados do projeto para um serviço externo e dependeriam de uma chave, latência,
limites e avaliação próprios; não devem entrar no caminho obrigatório de cada prompt sem um piloto.

O teste correto, antes de qualquer integração, é um spike isolado com uma amostra rotulada de sessões
do DCC: intenção/roteamento, valor de memória, risco de alteração e necessidade de revisão. Medir
precisão, cobertura, calibração dos limiares, p50/p95, custo e comportamento em português; comparar
com as regras determinísticas atuais e com um fallback generativo. O Jev deve ser avaliado por esse
papel de decisão e roteamento, não como substituto do fluxo de conversa ou da memória persistente.

### Medição inicial no macOS Apple Silicon

Uma medição local da release v2.2.2, com servidor HTTP em loopback e `--no-watcher`, observou cerca
de **21 MB RSS em repouso** após a inicialização (pico transitório de 34 MB), **48 MB RSS** após
ingerir 1.000 observações sintéticas, **25 ms** para uma consulta FTS e **6,8 MB em disco** para
essa pequena base. A ingestão dos 1.000 eventos em dez lotes levou 4,1 s. São números de referência
de uma máquina e de um corpus artificial, não uma garantia de SLA; o próximo teste de capacidade
deve usar um histórico real do DCC.

O upstream também documenta uma capacidade de escrita aproximada de 700 eventos/s. O DCC envia
lotes no fechamento e não inicia um processo de hook por ferramenta, portanto esse custo não é pago
no caminho de cada prompt. A consulta automática agora usa timeout de 500 ms e o disjuntor evita
repetir chamadas a um servidor fora do ar; ainda precisamos medir o impacto no tempo até o provider
começar com um histórico real.

### Critérios para sair do piloto

Não avançar para curadoria ou UX definitiva antes de validar: sidecar iniciando/parando sem terminal;
reinício após crash; atualização versionada com backup/reindex; fila de exportação para servidor
indisponível; lotes maiores que 256 eventos divididos com retry idempotente; consulta degradando em
menos de 500 ms; disjuntores após falhas repetidas; limite de CPU/RAM em uma sessão real; isolamento por workspace/projeto; e opção clara
para desligar ou usar servidor remoto.

### Piloto manual (duas janelas de terminal)

Esse procedimento continua útil para testar uma versão diferente ou depurar o serviço sem abrir o
DCC empacotado. No modo manual é preciso ter o binário instalado e manter o servidor rodando. O
piloto usa o DCC como dono da captura; portanto, não
execute `install-hooks` nem `install-mcp` para Claude/Codex ainda, pois isso criaria uma segunda
fonte de eventos e poderia duplicar observações. Esses instaladores serão avaliados depois, caso
o fluxo nativo seja escolhido para complementar a exportação do DCC.

No macOS Apple Silicon, instale a release publicada v2.2.2 em um diretório fixo:

```bash
mkdir -p ~/Applications/ai-memory
cd ~/Applications/ai-memory
curl -fsSL -O https://github.com/akitaonrails/ai-memory/releases/download/v2.2.2/ai-memory-macos-aarch64.tar.gz
tar -xzf ai-memory-macos-aarch64.tar.gz
./ai-memory --data-dir /tmp/dcc-ai-memory init
```

Em um segundo terminal, deixe o servidor ativo durante o teste:

```bash
~/Applications/ai-memory/ai-memory \
  --data-dir /tmp/dcc-ai-memory serve \
  --transport http --bind 127.0.0.1:49374
```

Depois, no terminal do DCC:

```bash
yarn dev
```

Para deixar o piloto automático ligado nessa execução, inicie o DCC assim:

```bash
DCC_AI_MEMORY_URL=http://127.0.0.1:49374 \
DCC_AI_MEMORY_WORKSPACE=dcc-pilot \
DCC_AI_MEMORY_PROJECT=dev-command-center \
yarn dev
```

Abra uma sessão normal, faça um prompt que gere uma decisão e feche a sessão pelo botão **X** no
canto direito do cabeçalho da conversa. O fechamento exporta
os eventos persistidos para o ai-memory e escreve um erro no log do DCC se o serviço estiver
indisponível; a conversa não é bloqueada. Para consultar, use a API desktop ou uma tela de teste.
Os dois escopos definidos acima também podem ser usados diretamente no endpoint MCP para conferir
o resultado sem depender da UI.
O caminho explícito continua disponível para diagnóstico e reprocessamento:

```ts
await syncSessionToAiMemory({
  sessionId,
  connection: { baseUrl: "http://127.0.0.1:49374" },
});
```

O `yarn dev` sem `DCC_AI_MEMORY_URL` não exporta nem consulta automaticamente. Nesse modo, o
retorno da sincronização explícita informa `attempted`, `accepted`, `acceptedIndices`, `workspace` e
`project`; use esses valores na consulta seguinte. Com `DCC_AI_MEMORY_URL`, o DCC também consulta o
ai-memory antes de cada novo turno e injeta até seis observações como **evidência histórica** no
contexto do modelo. Esse pacote usa no máximo 4.000 caracteres para complementar o re-anchor de
12.000 caracteres que o DCC já possui; os limites não são somados como dois transcripts. O pacote
não altera o prompt persistido e instrui o modelo a verificar a evidência contra o pedido, arquivos,
permissões e estado Git atuais.

1. Iniciar uma instância local do ai-memory v2.2.2 em uma porta de teste, sem reutilizar o diretório
   de produção. Por exemplo: `ai-memory init --data-dir /tmp/dcc-ai-memory` e depois
   `ai-memory serve --transport http --bind 127.0.0.1:49374`.
2. Executar uma sessão curta no DCC, registrar uma decisão, uma tentativa falha e uma pendência.
   Para o teste de recuperação, use uma frase explícita como “Registre esta decisão para consultas
   futuras: `DCC_MEMORY_UNIQUE_20260917` significa usar Worktree com cache npm isolado”. Evite
   chamar a informação de “secreta” ou pedir que ela não seja registrada: isso parece prompt
   injection para o provider e pode gerar uma recusa, mesmo quando a ingestão estiver correta. A
   sessão precisa estar persistida no histórico antes da sincronização.
3. Chamar `syncSessionToAiMemory({ sessionId, connection: { baseUrl: "http://127.0.0.1:49374" } })`
   pelo cliente desktop. O retorno `attempted/accepted/failedIndex` registra o resultado do lote.
   Ele também devolve os valores efetivos de `workspace` e `project`; use esses mesmos valores na
   consulta seguinte quando a conexão não tiver escopos explícitos.
4. Consultar com `queryAiMemory({ query, connection: { baseUrl: "http://127.0.0.1:49374", workspace, project }, limit: 8 })`.
   O resultado deve trazer snippets do projeto correto; repetir com outro projeto para confirmar
   isolamento.
5. Reiniciar o serviço e repetir a consulta. Depois sincronizar novamente a mesma sessão: as
   chaves estáveis devem impedir duplicação de observações.
6. Com o servidor ainda ativo e as variáveis `DCC_AI_MEMORY_*` definidas, abrir um chat novo pelo
   botão `+` e fazer uma pergunta sobre a decisão. O DCC deve consultar o ai-memory antes de chamar
   o provider; a resposta deve mencionar a evidência recuperada quando ela for relevante. Para uma
   prova mais forte, coloque o fato distintivo apenas na resposta final da primeira sessão e peça ao
   segundo chat para repetir o identificador e o fato. Não conte o próprio prompt da pergunta como
   recuperação.

O comando não envia credenciais para o modelo e não instala hooks. O `baseUrl`, o token opcional e
os escopos são fornecidos explicitamente pela aplicação; em uma UI definitiva eles devem vir de uma
configuração segura e o backend deve resolver os escopos, em vez de aceitar valores livres de um
prompt. No modo de desenvolvimento, o modo automático é opt-in por variável de ambiente para este
piloto; em uma release com o binário empacotado ele fica ativo por padrão. Uma UI definitiva deve
mostrar as fontes recuperadas e permitir corrigir ou rejeitar uma observação antes de promovê-la a
decisão do projeto.

### Validação do empacotamento da release

Em 17/09/2026, `yarn build` executado com Node 22 gerou o `.app` e o `.dmg` do DCC com o binário
`ai-memory` dentro de `Contents/MacOS/ai-memory`; `ai-memory --version` retornou `2.2.2`. Isso
confirma que uma instalação normal pode iniciar o sidecar sem download ou terminal adicional. A
execução local parou somente na etapa de assinatura do updater porque o ambiente não tinha
`TAURI_SIGNING_PRIVATE_KEY`; em CI de release essa variável deve ser fornecida junto da chave pública
já configurada. O binário é fixado em v2.2.2 no script de build e o checksum é verificado antes de
ser incluído no bundle.

O impacto de tamanho foi medido por componente no mesmo artefato: o `.app` atual ocupa **168,2 MiB
descompactado**, dos quais **29,4 MiB** são o executável `ai-memory`. Removendo esse arquivo da
cópia de medição, o aplicativo fica em **138,8 MiB**. Portanto, a integração acrescenta cerca de
29,4 MiB ao `.app`; ela não reintroduz o executável Claude de aproximadamente 190 MiB que foi
removido na auditoria anterior. O auxiliar `dcc-claude-sidecar` continua sendo a ponte do SDK, com
cerca de 62,7 MiB, separado do CLI Claude instalado pelo usuário. O `.dmg` gerado nesta máquina ficou
em **78,2 MiB** comprimidos; o tamanho do download deve ser acompanhado por plataforma em cada
release.

Na release v2.2.2, `memory_query` pode retornar `hits: []` e ainda assim preencher `raw_hits` com
observações recém-capturadas. O adaptador do DCC trata `raw_hits` como fallback quando não há páginas
consolidadas, preservando o teste de ingestão sem exigir uma etapa de síntese LLM.

### Resultado manual observado em 17/09/2026

Uma execução real anterior do DCC retornou três `raw_hits` no escopo `dcc-pilot/dev-command-center`:
dois prompts do piloto e uma notificação de uma sessão posterior. Essa notificação citou a página,
decisão, sessão de origem e worktree atual, o que sugeria recuperação entre sessões, mas não isolava
qual camada forneceu o contexto. O resultado independente do `curl` continua mostrando `hits: []`
porque não há página consolidada; isso é esperado na v2.2.2.

Há duas observações de prompt com IDs diferentes e conteúdo idêntico. Se apenas um prompt foi
enviado, investigar uma exportação duplicada; se houve dois turnos ou uma sincronização manual e
outra automática, o resultado é compatível com o fluxo atual. A prova de deduplicação precisa
comparar os `event_id` do DCC com as chaves de ingestão, não apenas o texto exibido.

Em seguida, um novo chat criado pelo botão `+` na mesma tarefa recebeu apenas uma consulta pelo
identificador do piloto e respondeu com a decisão Worktree, sua justificativa, a sessão de origem,
a página de memória e o worktree atual. Isso valida a recuperação entre chats no fluxo real do
DCC. Para atribuir a recuperação exclusivamente ao ai-memory, o próximo cenário deve usar um
segredo de teste novo, criado somente na sessão exportada, e verificar que ele não aparece no
handoff ou histórico local do DCC.

Ao executar esse cenário, a consulta precisa usar exatamente o novo identificador
`MEMORY_ISOLATION_UNIQUE_20260917`. Um `raw_hit` contendo apenas o próprio prompt
“procure na memória...” é uma observação da consulta atual, não prova de recuperação; a evidência
válida é o registro do prompt-fonte, criado antes, em uma sessão diferente.

No teste mais recente desse identificador, o `curl` mostrou o prompt-fonte entre dez `raw_hits`, mas o modelo
respondeu que não o encontrava. Isso confirma ingestão e indexação, porém também revela a diferença
entre **memória armazenada** e **memória usada pelo turno**: antes do modo automático descrito acima,
o DCC não chamava `memory_query` para montar o contexto do novo chat. Repita o cenário depois de
reiniciar o DCC com `DCC_AI_MEMORY_URL` definido.

Na repetição com `DCC_MEMORY_RECOVERY_20260917`, em 17/09/2026 às 20:06, o Claude declarou que o
bloco `historical evidence` estava injetado no prompt atual. Isso confirma o caminho completo
**fechamento → exportação → `memory_query` → contexto do provider**. A recusa de usar o token como
uma decisão veio do guardrail do modelo: o histórico continha instruções para gravar “segredos” e
não revelá-los, um padrão de prompt injection. Para medir a qualidade da memória, use fatos de
projeto transparentes e permita que sejam mostrados em respostas futuras.

Na execução seguinte, às 20:33, o mesmo fluxo recuperou o bloco histórico, mas o Claude rejeitou o
identificador artificial `DCC-20260917` e respondeu com os fatos genuínos já existentes sobre
worktrees: `.env` por symlink, `npm ci` com cache isolado e reversão de `yarn.lock`. Esse é o
comportamento desejado para a camada de evidência: a ponte está funcionando, enquanto o modelo não
transforma qualquer prompt de teste em uma decisão oficial do projeto.

## Prova integrada ainda necessária

Uma implementação experimental opt-in deve passar pelo DCC real:

1. Claude registra uma decisão, uma tentativa que falhou e uma pendência. Incluir uma decisão
   dita apenas na resposta final, para não validar só repetição do prompt.
2. Encerrar/reiniciar o DCC e abrir outra tarefa com Codex. Recuperar os três itens com origem,
   sem copiar manualmente o resumo nem instalar configuração global.
3. Rodar o mesmo cenário em Local e Worktree. Duas branches têm fatos incompatíveis; a tarefa
   destino recebe apenas o que se aplica, e vê aviso de evidência de outro checkout quando necessário.
4. Duas sessões/projetos simultâneos não trocam fatos. Tentativa de consulta de escopo não autorizado
   é bloqueada pelo backend, incluindo tentativa de ampliar a busca globalmente.
5. Corrigir uma decisão, reabrir a tarefa e confirmar que a versão antiga aparece como substituída.
6. Interromper runtime e serviço de memória em pontos de entrega, reiniciar e verificar recuperação,
   duplicação, perda, comportamento sem memória e descarte de respostas de contexto já obsoletas.
7. Exercitar exclusões de captura, texto malicioso, remoção de conhecimento e diagnóstico.

Critérios propostos, ainda não medidos: zero erro de escopo; todos os fatos críticos dos cenários
curados recuperados com origem; nenhuma hipótese promovida a validação; contexto limitado; startup
e turno continuam utilizáveis com motor ausente. Medir p50/p95, tokens de contexto, RAM e disco com
corpus representativo. Como meta inicial de engenharia, avaliar recuperação local em até 500 ms no
p95 e deadline de 1 s; ajustar com dados, sem tratar esses números como capacidade comprovada.

Comparar três condições em prompts equivalentes: DCC atual, ai-memory sem LLM e ai-memory com
consolidação opcional. Avaliar português, sinônimos, decisões contraditórias, histórico longo e
informação antiga que ainda é válida. Medir retrabalho e correção, além da presença de palavras-chave.

## Escolha entre integrar, adaptar e construir

| Opção | Decisão atual |
|---|---|
| Somente adicionar o MCP | Útil para exploração manual; insuficiente para continuidade automática confiável |
| Serviço separado com adaptador DCC | Caminho recomendado para a prova; preserva substituição e isolamento de falhas |
| Incorporar crates Rust | Tecnicamente plausível, não compilado nesta análise; exige auditar API, runtime e dependências; upstream exige Rust 1.95 |
| Fork completo | Prematuro; considerar só diante de limitação reproduzível que não caiba no adaptador nem em contribuição upstream |
| Motor próprio completo agora | Custo desnecessário antes de medir o reaproveitamento |

Escolher ai-memory para o produto se o adaptador conseguir garantir escopo, evidência, lifecycle e
recuperação, com qualidade e operação aceitáveis. Uma falha em hook de provider não descarta o
motor: o DCC pode exportar seus próprios eventos. Uma falha de isolamento incontornável ou custo
operacional elevado pode justificar outro backend.

Se o motor não passar, preservar os contratos e implementar uma alternativa pequena sobre o
SQLite que o DCC já tem: registros tipados de decisão/pendência/resultado, origem nos eventos,
índice FTS5 a validar no build distribuído, checkpoint de tarefa, substituição de fatos e exportação
Markdown. Começar sem vetores, grafo sofisticado, sincronização de equipe ou extração LLM obrigatória.
Adicionar essas capacidades quando a avaliação mostrar uma lacuna real. Não seria começar o DCC
do zero: a base de identidade, histórico e execução já existe.

## Diferencial de produto a perseguir

O valor não é um botão “Memory”. É abrir uma tarefa com outro modelo e receber decisões
pertinentes, motivos, tentativas descartadas, trabalho validado e próximos passos, com origem
consultável e conhecimento do checkout atual.

Uma superfície pequena pode mostrar “Contexto recuperado”, fontes, data e ações para corrigir,
desconsiderar ou fixar conhecimento. O DCC pode ligar uma memória ao resultado de teste e ao commit
que a sustenta; deve também explicar quando essa evidência não cobre a branch atual.

Isso é uma hipótese de diferenciação que precisa ser demonstrada em fluxos reais. Esta análise
não fez auditoria atual das funcionalidades de memória de Synara/T3 Code e não sustenta alegação
de exclusividade ou superioridade geral sobre esses produtos.

## Complemento: validação para escolher integração, PR ou fork

A documentação de [companions](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/docs/companion-crates.md)
recomenda produtos externos sobre as APIs públicas, com filas, UI e políticas próprias. Novas
superfícies no núcleo devem ter utilidade independente do produto integrador. Isso favorece o
adaptador DCC e PRs pequenas, sem pedir ao upstream que incorpore o modelo de tarefas do DCC.

Uma inspeção adicional identificou `/hook/batch`, utilizado pelo importer e pelo spool upstream,
com acknowledgements de processamento, inclusive aceitação parcial. Essa rota **não foi exercitada
pelo smoke de 22 checks**, que usou `/hook`. Avaliar ordem, retries, índices aceitos, descarte por
política e queda do servidor nesse endpoint antes de propor uma API nova de confirmação.
Fonte: [router e testes de batch](https://github.com/akitaonrails/ai-memory/blob/v2.3.0/crates/ai-memory-hooks/src/router.rs).
Importação histórica também tem a identidade `external-import`; o adaptador deve preservar a
distinção entre importar eventos antigos e observar uma execução ao vivo.

PRs candidatas, condicionadas a casos reproduzíveis na prova integrada:

- **Atualização condicional por versão/hash:** impedir que duas sessões sobrescrevam correções
  umas das outras. A documentação de companions reconhece a ausência de compare-and-write atômico.
  Mudança genérica, com conflito explícito e compatibilidade com clientes existentes.
- **Filtros estruturados de recuperação:** selecionar metadados/tags antes de ranking e limite,
  se o mapeamento de escopos não atender o DCC. Filtrar apenas depois do top-k pode perder resultados
  válidos. Isso resolve seleção, não substitui autorização por projeto.
- **Proveniência extensível:** transportar e devolver IDs externos de evento/documento por uma
  superfície pública, se manter esse vínculo apenas no DCC causar perda ou ambiguidade demonstrável.

Não abrir três frentes antecipadamente. Primeiro reproduzir uma limitação, implementar a menor
correção genérica, testar contra o DCC e incluir testes/documentação no upstream. A aceitação da
PR é decisão dos mantenedores, não condição para validar localmente a hipótese.

Um fork para desenvolver uma contribuição é diferente de manter uma distribuição divergente.
Um fork temporário com poucos patches, versão fixada e testes pode permitir o piloto enquanto a
PR é avaliada. Assumir um fork permanente apenas se o DCC precisar de mudanças coerentes e
sustentáveis que o upstream não queira manter. Considerar backend próprio quando a adaptação
exigir modificar continuamente armazenamento, recuperação e lifecycle, com custo maior que um
motor menor sobre os eventos e SQLite existentes no DCC.

Saída esperada da prova: matriz de cenários com resultado e origem, falhas reproduzíveis,
medições de contexto/latência/recursos e lista exata dos patches necessários. Essa evidência
permite decidir sem construir dois motores completos ou depender de uma PR ser aceita.
