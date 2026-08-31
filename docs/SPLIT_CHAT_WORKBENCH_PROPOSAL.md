# Proposta técnica — Split Chat Workbench

> **Status:** proposta para implementação incremental, ainda não iniciada  
> **Decisão recomendada:** viável, desde que tratada como evolução do modelo de estado do
> workbench — não como uma camada visual envolvendo várias cópias do chat.  
> **Referência de produto:** split views do
> [Synara](https://github.com/Emanuele-web04/synara), analisadas no commit
> [`8c7d87f`](https://github.com/Emanuele-web04/synara/commit/8c7d87fe721e9da1e1c306dc5e1e9970c746dd37). O
> [Dray](https://github.com/monorepo-labs/dray) também foi avaliado como referência complementar de
> sessões paralelas e painel contextual, no commit
> [`0961455`](https://github.com/monorepo-labs/dray/commit/09614552c9a7d371565ffa77dd3ef5f476d4aee5).

---

## 1. Resumo executivo

O DCC deve permitir visualizar e operar até quatro conversas simultaneamente, cada uma com sua
timeline e seu composer, sem exigir trocas constantes de workspace ou sessão.

A funcionalidade é tecnicamente viável. O backend e o frontend já possuem partes importantes da
fundação:

- eventos chegam globalmente e já são agrupados por sessão;
- snapshots são mantidos por `sessionId`;
- históricos são consultados por sessão;
- o review do último turno já recebe `sessionId + workspaceId`;
- o DCC já possui superfícies redimensionáveis, Inspector, diff e comportamento de overlay.

O bloqueio não é de infraestrutura. O bloqueio é que a composição atual da interface deriva quase
tudo de uma única `selectedSessionId` e de um único workspace visualmente ativo. Portanto, montar
quatro cópias do `SessionWorkbench` criaria uma implementação aparentemente funcional, mas sujeita a
envio para a sessão errada, colisão de drafts, Inspector inconsistente e duplicação de superfícies
pesadas.

O caminho seguro é introduzir o conceito de **painel de conversa independente** e manter um
**painel focado** para comandar Inspector, terminal, atalhos e superfícies globais.

### Decisão de produto recomendada

- Até quatro chats em um layout 2×2.
- Cada painel possui timeline, composer, draft, scroll e estado transitório independentes.
- Um único Inspector compartilhado acompanha o painel focado.
- O estado do Inspector é lembrado por painel, mas somente um Inspector é renderizado.
- Terminal e delivery acompanham o workspace do painel focado.
- A identidade do painel é `{ workspaceId, sessionId }` desde o início.
- A liberação ocorre em fases: dois painéis no mesmo workspace antes de quatro painéis e antes de
  cross-workspace.

---

## 2. Objetivo e motivação

### 2.1 Problema atual

Para acompanhar trabalhos paralelos, o usuário precisa:

1. selecionar um workspace ou uma sessão;
2. ler a timeline;
3. interagir com o composer;
4. trocar para outro contexto;
5. repetir o processo.

Isso é especialmente custoso quando vários agentes estão executando ou aguardando pequenas decisões
do usuário. A navegação esconde estado relevante e transforma acompanhamento paralelo em alternância
serial.

### 2.2 Resultado desejado

O usuário deve conseguir:

- arrastar uma sessão ou workspace para uma borda e criar um novo painel;
- acompanhar até quatro timelines simultaneamente;
- escrever diretamente no composer de qualquer painel;
- identificar imediatamente qual painel está focado;
- abrir o diff ou review de turno daquele painel sem perder o layout;
- maximizar temporariamente um painel e retornar ao grid anterior;
- fechar um painel sem encerrar ou arquivar sua sessão;
- restaurar o layout após reiniciar o app, quando as sessões ainda forem válidas.

### 2.3 Não objetivos

- Não criar quatro instâncias completas de Inspector.
- Não renderizar quatro terminais ou quatro áreas de delivery por padrão.
- Não transformar cada painel numa cópia integral de todo o workbench.
- Não permitir profundidade arbitrária de splits.
- Não mudar a semântica de sessão, workspace, worktree ou provider no backend.
- Não incentivar execuções simultâneas no mesmo worktree sem indicar o risco.

---

## 3. Referência analisada: Synara

O Synara persiste split views como uma árvore recursiva de painéis com profundidade máxima 2, o que
resulta em até quatro folhas num grid 2×2.

Cada folha carrega seu próprio:

- `threadId`;
- estado de painel lateral;
- turno selecionado no diff;
- arquivo selecionado;
- identidade estável de painel.

A árvore guarda separadamente o `focusedPaneId`. Portanto, foco não define quais chats estão
montados; ele serve para rota, atalhos e ações contextuais.

Referências diretas:

- [modelo e persistência do split](https://github.com/Emanuele-web04/synara/blob/8c7d87fe721e9da1e1c306dc5e1e9970c746dd37/apps/web/src/splitViewStore.ts);
- [operações imutáveis e limite de profundidade](https://github.com/Emanuele-web04/synara/blob/8c7d87fe721e9da1e1c306dc5e1e9970c746dd37/apps/web/src/splitView.logic.ts);
- [renderização dos painéis, foco, resize e drop](https://github.com/Emanuele-web04/synara/blob/8c7d87fe721e9da1e1c306dc5e1e9970c746dd37/apps/web/src/components/chat/SplitChatSurface.tsx);
- [drafts independentes por thread](https://github.com/Emanuele-web04/synara/blob/8c7d87fe721e9da1e1c306dc5e1e9970c746dd37/apps/web/src/composerDraftStore.ts).

### O que deve ser aproveitado conceitualmente

- Árvore de layout pequena e persistível.
- Folhas identificadas por `paneId`, em vez de posição como `left` ou `right`.
- Foco separado da existência dos painéis.
- Estado contextual armazenado por folha.
- Colapso automático da árvore ao fechar uma folha.
- Limite explícito de quatro painéis.

### O que não deve ser copiado literalmente

O DCC possui Inspector, review de último turno, Git, delivery, terminal, mission spec e companion
surfaces mais integrados ao conceito de workspace. Colocar uma cópia desses recursos dentro de cada
chat reduziria demais a área útil e aumentaria o custo de renderização. No DCC, essas superfícies
devem seguir o painel focado.

### Referência complementar: Dray

O Dray reforça duas decisões úteis para o DCC, mas **não implementa, na versão 0.9.2 analisada, o
mesmo split de dois a quatro chats do Synara**. Ele mantém várias sessões executando em paralelo e
exibe uma sessão selecionada por vez no corpo principal. Ao lado dela, pode abrir um painel
contextual para diff, pull request, checks ou comentários.

Os padrões aproveitáveis são:

- estado de execução e streaming mantido por sessão, mesmo quando a sessão não está visível;
- draft preservado por `sessionId` durante a troca de sessão;
- uma única superfície contextual à direita, associada à conversa selecionada;
- sessões paralelas apresentadas na navegação sem exigir múltiplas instâncias do painel lateral.

Isso apoia a proposta de um único Inspector compartilhado seguindo `focusedPaneId`. Entretanto, o
Dray não serve como prova de desempenho para quatro timelines e quatro composers montados ao mesmo
tempo, porque sua composição atual continua baseada em um único `selectedSessionId`.

Referências diretas:

- [seleção única e estado paralelo por sessão](https://github.com/monorepo-labs/dray/blob/09614552c9a7d371565ffa77dd3ef5f476d4aee5/apps/desktop/src/hooks/useSessions.ts);
- [draft independente por sessão](https://github.com/monorepo-labs/dray/blob/09614552c9a7d371565ffa77dd3ef5f476d4aee5/apps/desktop/src/hooks/useDraft.ts);
- [shell com uma coluna principal e um painel contextual opcional](https://github.com/monorepo-labs/dray/blob/09614552c9a7d371565ffa77dd3ef5f476d4aee5/apps/desktop/src/components/layout/AppShell.tsx).

---

## 4. Estado atual do DCC

### 4.1 Fundação já compatível

| Área | Estado atual | Consequência para o split |
|---|---|---|
| Eventos | `useSessionEventFeed` recebe o stream global e mantém buckets por sessão | Não é necessário criar uma assinatura nativa por painel |
| Snapshots | `sessionSnapshotsById` já é um mapa por sessão | Pode alimentar vários painéis |
| Histórico | Query `sessionThreads` já é chaveada por `sessionId` | Cada painel pode consultar sua timeline independentemente |
| Conversa | `ActiveThreadViewport` já limita a janela de mensagens renderizadas | Quatro timelines são possíveis com controle de memória |
| Turn review | A superfície recebe `sessionId` e `workspaceId` | Pode seguir o painel focado |
| Layout | Sidebar, Inspector e companion surface já possuem resize/overlay | Há padrões prontos de interação e acessibilidade |
| Store | Zustand já é dependência do desktop | Pode persistir a árvore sem nova dependência |

### 4.2 Acoplamentos que precisam ser removidos

1. `selectedSessionId` é único para toda a aplicação.
2. `sessionEvents` visíveis são projetados somente para a sessão selecionada.
3. Ao trocar de sessão, o histórico anterior é removido do cache de frontend.
4. `pendingPrompt` e `pendingPromptSessionId` representam somente um envio otimista por vez.
5. Draft, effort e approval policy do composer usam `workspaceId` como chave principal.
6. `steer`, `queue`, `resume` e `abort` operam sobre `selectedSessionSnapshot`.
7. `surfaceSelection`, Inspector e vários fluxos de arquivo/diff são globais.
8. `workspaceSessions` contém apenas as sessões do workspace selecionado.
9. O `SessionWorkbench` inclui chat, terminal, delivery e estado de superfícies; duplicá-lo duplica
   responsabilidades que deveriam continuar únicas.

### 4.3 Por que uma camada visual não é suficiente

Uma implementação que apenas envolvesse várias instâncias do workbench num grid teria estes modos de
falha:

- dois composers do mesmo workspace persistiriam sobre a mesma chave de draft;
- clicar num painel mudaria a sessão global usada por ações assíncronas de outro painel;
- um envio poderia substituir o `pendingPrompt` de outro envio simultâneo;
- alternar foco removeria a query de histórico que outro painel ainda está exibindo;
- Inspector, diff e arquivo selecionado poderiam saltar entre painéis;
- quatro workbenches executariam queries e effects duplicados de Git, terminal e delivery;
- ações iniciadas antes de uma mudança de foco poderiam concluir no contexto visual errado.

Essa abordagem deve ser explicitamente evitada, mesmo para um MVP.

---

## 5. Experiência proposta

### 5.1 Layout principal

```text
┌────────────┬───────────────────────────────┬───────────────┐
│ Workspaces │ Chat A          │ Chat B      │               │
│            │ timeline        │ timeline    │   Inspector   │
│            │ composer        │ composer    │   contextual  │
│            ├─────────────────┼─────────────│               │
│            │ Chat C          │ Chat D      │               │
│            │ timeline        │ timeline    │               │
│            │ composer        │ composer    │               │
└────────────┴───────────────────────────────┴───────────────┘
```

O Inspector é opcional e ocupa a lateral direita. Quando fechado, os chats usam toda a largura.

### 5.2 Criação do split

- Arrastar uma sessão/workspace da sidebar para uma borda do chat mostra zonas de drop.
- Bordas esquerda/direita criam split horizontal.
- Bordas superior/inferior criam split vertical.
- Um botão `Split` no header oferece uma alternativa acessível ao drag-and-drop.
- Ao atingir quatro folhas, novas zonas de subdivisão ficam indisponíveis.
- Uma sessão já visível não deve ser duplicada no mesmo layout; o drop apenas foca o painel existente.

### 5.3 Foco

- Clicar, digitar ou navegar por teclado dentro de um painel o torna focado.
- O painel focado recebe um contorno discreto, sem escurecer excessivamente os demais.
- Atalhos como focar composer, abortar, abrir terminal ou abrir Inspector usam o painel focado.
- O foco deve ser armazenado por `paneId`, nunca inferido somente por `sessionId`.

### 5.4 Fechar, substituir e maximizar

- Fechar painel remove somente a folha do layout; não encerra a sessão.
- A árvore colapsa automaticamente quando resta apenas um filho.
- Com uma única folha restante, a interface volta ao modo normal de chat único.
- `Maximize` apresenta somente o painel focado, preservando a árvore para a ação `Restore layout`.
- Um painel vazio permite escolher uma sessão existente ou iniciar uma nova sessão.

### 5.5 Responsividade

- Quatro painéis devem ser oferecidos somente quando houver área útil adequada.
- Em janelas estreitas, o layout preservado pode ser apresentado como duas folhas ou como uma folha
  maximizada, sem apagar a árvore persistida.
- Inspector e companion surfaces viram overlay antes de comprimir o composer abaixo de sua largura
  mínima.
- Resizes devem ter limites de proporção, por exemplo entre 25% e 75% por nó.

---

## 6. Inspector, diff e review de turno

### 6.1 Princípio de escopo

O Inspector possui conteúdos com escopos diferentes:

| Conteúdo | Escopo correto |
|---|---|
| Último turno, atividade e permissões | Sessão do painel focado |
| Git changes, branch, PR/MR, pipeline e delivery | Workspace do painel focado |
| Arquivo ou diff aberto a partir de uma mensagem | Painel que originou a ação |
| Terminal | Projeto/workspace do painel focado |
| Mission spec e plano | Workspace e, quando aplicável, sessão de origem |

### 6.2 Estado por painel, apresentação compartilhada

Cada folha pode lembrar qual conteúdo contextual abriu:

```ts
type PaneInspectorState = {
	mode: "git" | "code";
	tab: "activity" | "context" | "spec";
	scope: "workspace" | "last-turn";
	surface: PaneSurfaceSelection | null;
};
```

Somente o estado do `focusedPaneId` é projetado no Inspector compartilhado. Ao voltar para outro
painel, sua seleção anterior pode ser restaurada se ainda for válida.

### 6.3 Regras de segurança

- A ação de annotation deve carregar explicitamente o `workspaceId` e `sessionId` de origem.
- Respostas assíncronas devem validar se o painel e a seleção ainda existem antes de atualizar a UI.
- Arquivos, diffs e previews potencialmente obsoletos não devem ser restaurados após reiniciar o app
  sem revalidação.
- Review/undo de turno nunca deve assumir a sessão focada no momento da confirmação; deve usar a
  sessão capturada quando a operação foi aberta.

---

## 7. Modelo de estado proposto

### 7.1 Identidade da folha

```ts
type PaneId = string;

type ConversationPane = {
	kind: "leaf";
	id: PaneId;
	workspaceId: string;
	sessionId: string | null;
	inspector: PaneInspectorState;
};

type SplitNode = {
	kind: "split";
	id: PaneId;
	direction: "horizontal" | "vertical";
	first: PaneNode;
	second: PaneNode;
	ratio: number;
};

type PaneNode = ConversationPane | SplitNode;

type SplitLayout = {
	version: 1;
	root: PaneNode;
	focusedPaneId: PaneId;
	maximizedPaneId: PaneId | null;
	updatedAt: string;
};
```

`workspaceId` deve estar na folha mesmo durante a primeira fase, que poderá restringir splits ao
mesmo workspace. Isso evita uma migração estrutural para liberar cross-workspace posteriormente.

### 7.2 Estado que não deve morar na árvore persistida

- texto completo de mensagens;
- deltas de streaming;
- snapshots de runtime;
- prompts otimistas;
- requests de permissão;
- conteúdo de arquivo ou diff;
- referências a objetos React/DOM.

Esses dados continuam nas stores e queries existentes, chaveados por identidade estável.

### 7.3 Persistência e migração

- Persistir somente estrutura, proporções, identidade dos painéis e preferências leves.
- Versionar o payload desde a primeira entrega.
- Validar se workspace e sessão ainda existem durante hidratação.
- Remover folhas inválidas e colapsar a árvore de maneira determinística.
- Nunca impedir o boot por layout corrompido; fallback para o chat atualmente selecionado.

---

## 8. Arquitetura de frontend

### 8.1 Separar `ConversationPane` de `SessionWorkbench`

Extrair uma superfície focada somente na conversa:

```text
SessionWorkbench / SplitWorkbenchShell
├── ConversationPane(s)
│   ├── chat header
│   ├── ActiveThreadViewport
│   ├── PendingPermissionPanel
│   └── WorkspaceComposer
├── shared terminal drawer
├── shared delivery controls
├── shared companion surface
└── shared Inspector
```

O componente de painel deve receber contexto e ações explícitas, sem ler uma seleção global
implicitamente:

```ts
type ConversationPaneContext = {
	paneId: string;
	workspaceId: string;
	sessionId: string | null;
};
```

### 8.2 Eventos

Manter uma única assinatura global do runtime. O hook/store deve oferecer projeções seletivas:

```ts
eventsForSession(sessionId)
activityEventsForSession(sessionId)
snapshotForSession(sessionId)
```

Atualizar somente os painéis que possuem eventos novos. Eventos de uma sessão não podem provocar a
reconstrução das timelines dos outros três painéis.

### 8.3 Histórico e cache

- Cada painel monta a query `sessionThreads(sessionId)`.
- O cleanup por mudança de foco deve ser removido ou passar a considerar o conjunto de sessões
  atualmente montadas.
- Queries de sessões visíveis não podem ser removidas.
- Ao fechar uma folha, a query volta ao ciclo normal de GC do React Query.
- O cache persistido deve continuar respeitando o limite global existente.

### 8.4 Composer e drafts

O draft precisa deixar de ser apenas workspace-scoped. Uma opção compatível:

```ts
type ComposerScope = {
	workspaceId: string;
	sessionId: string | null;
	paneId?: string;
};
```

Regras:

- sessão existente: draft por `sessionId`;
- painel de nova sessão: draft por `paneId`, até a sessão ser criada;
- depois de `startThread`, promover/rechavear o draft para o novo `sessionId`;
- effort e approval policy podem manter fallback por workspace/provider, mas a seleção ativa deve ser
  independente quando necessário;
- prefill e revisão devem carregar o painel destinatário explicitamente.

### 8.5 Prompts e ações simultâneas

Substituir singletons por mapas:

```ts
pendingPromptBySessionId: Record<string, PendingPrompt | undefined>;
pendingPromptByPaneId: Record<string, PendingPrompt | undefined>;
```

Todas as ações precisam ser target-aware:

```ts
submitPrompt({ paneId, workspaceId, sessionId, turn })
steerPrompt({ sessionId, turn })
queuePrompt({ sessionId, turn })
resumeSession({ sessionId })
abortSession({ sessionId })
```

Nenhuma dessas operações deve resolver seu alvo consultando `selectedSessionId` depois de iniciada.

### 8.6 Workspaces diferentes

Para cross-workspace, a UI precisa conseguir obter os metadados de cada `workspaceId` visível sem
substituir o workspace global. Isso inclui:

- resumo do workspace e caminhos;
- sessões do workspace;
- provider/model da sessão;
- worktree e root path;
- permissões e estado de runtime;
- contexto necessário para Inspector e terminal quando o painel ganhar foco.

A seleção da sidebar pode continuar representando o contexto principal de navegação, mas não deve
determinar quais painéis permanecem montados.

---

## 9. Concorrência e segurança de worktree

O split não cria a possibilidade de duas sessões atuarem no mesmo worktree — isso já pode acontecer
hoje —, mas torna essa operação mais acessível e visível.

Regras recomendadas:

- Exibir indicador quando dois ou mais painéis possuem sessões ativas no mesmo workspace/worktree.
- Explicar que alterações simultâneas podem interferir entre si.
- Não bloquear leitura ou digitação automaticamente.
- Considerar confirmação para iniciar uma segunda execução mutável no mesmo worktree.
- Não apresentar alerta quando os painéis pertencem a worktrees isolados diferentes.
- Git changes permanece uma visão do workspace, não uma atribuição garantida a uma única sessão.
- Last-turn review continua sendo a fonte adequada para alterações atribuídas a um turno específico.

---

## 10. Plano incremental

### Fase 0 — Refatoração sem mudança visual

Objetivo: tornar o modo atual de chat único compatível com destinos explícitos.

- [ ] Criar `ConversationPaneContext`.
- [ ] Tornar submit, steer, queue, abort e resume explicitamente session-targeted.
- [ ] Migrar prompt otimista de singleton para mapa.
- [ ] Introduzir draft por sessão/painel com migração do draft antigo por workspace.
- [ ] Expor seletores de eventos por sessão.
- [ ] Ajustar cleanup de queries para preservar sessões montadas.
- [ ] Manter a UI visualmente idêntica e executar toda a suíte relevante.

**Gate:** nenhum fluxo do chat único pode regredir. Se essa fase não ficar limpa, o split não deve
prosseguir.

### Fase 1 — Dois painéis no mesmo workspace

Objetivo: validar a arquitetura com o menor aumento de complexidade visual.

- [ ] Extrair `ConversationPane`.
- [ ] Criar árvore com no máximo duas folhas.
- [ ] Adicionar ação de split pelo header/picker, antes do drag-and-drop.
- [ ] Permitir timelines, composers e streaming simultâneos.
- [ ] Implementar foco e resize.
- [ ] Fazer Inspector/turn review seguir o painel focado.
- [ ] Preservar sessão normal ao fechar o segundo painel.
- [ ] Colocar a feature atrás de flag.

**Gate:** duas sessões devem conseguir receber turns simultaneamente sem cruzar draft, prompt,
estado de botão, permissão ou destino de ação.

### Fase 2 — Quatro painéis e drag-and-drop

Objetivo: entregar a experiência completa 2×2.

- [ ] Liberar árvore com profundidade máxima 2.
- [ ] Implementar zonas de drop nas quatro bordas.
- [ ] Adicionar colapso, substituição e maximização de folha.
- [ ] Persistir e reidratar layout.
- [ ] Implementar navegação por teclado e alternativa ao drag.
- [ ] Aplicar limites responsivos de largura/altura.
- [ ] Medir memória, commit time e fluidez com quatro streams ativos.

### Fase 3 — Cross-workspace

Objetivo: eliminar a necessidade de alternar workspace para interagir com trabalhos paralelos.

- [ ] Permitir `{ workspaceId, sessionId }` diferentes por folha.
- [ ] Consultar sessões e metadados para todos os workspaces visíveis.
- [ ] Fazer Inspector, terminal e delivery seguirem o workspace focado.
- [ ] Preservar painéis ao navegar pela sidebar.
- [ ] Tratar workspace arquivado, concluído ou removido enquanto visível.
- [ ] Indicar claramente projeto, workspace e branch no header compacto de cada painel.

### Fase 4 — Polimento e liberação padrão

- [ ] Telemetria de criação, remoção, maximização e restauração do split.
- [ ] Testes prolongados com sessões simultâneas.
- [ ] Ajustes de densidade, atalhos e acessibilidade.
- [ ] Documentação de usuário.
- [ ] Remover feature flag somente após estabilidade comprovada.

---

## 11. Estratégia de testes

### 11.1 Testes unitários

- criação e subdivisão da árvore;
- limite de profundidade e de quatro folhas;
- resize com clamp;
- remoção e colapso de folhas;
- foco após remoção;
- hidratação e migração de payload inválido/antigo;
- promoção de draft de `paneId` para `sessionId`;
- roteamento de ações para sessão explícita;
- seleção do Inspector a partir do painel focado.

### 11.2 Testes de componente

- dois composers mantêm textos diferentes;
- envio em A não desabilita ou limpa B;
- streaming em A não reconstrói a timeline de B;
- abort em B nunca atinge A;
- permission request aparece somente no painel correto;
- troca de foco atualiza Inspector sem desmontar chats;
- fechamento de painel preserva a sessão;
- maximizar e restaurar preserva scroll e draft.

### 11.3 Testes de integração

- dois turns simultâneos no mesmo workspace;
- dois turns simultâneos em workspaces diferentes;
- painel focado muda enquanto um envio está em andamento;
- sessão é concluída, abortada ou arquivada em background;
- workspace é removido enquanto há uma folha aberta;
- app reinicia com layout persistido e uma sessão ausente;
- diff/annotation continua apontando para a sessão capturada na abertura;
- limite de memória com quatro históricos extensos.

### 11.4 Acessibilidade

- criar split sem drag-and-drop;
- mover foco entre painéis por teclado;
- resize com setas, Home e End;
- anunciar painel focado e posição no grid;
- ordem de tabulação previsível;
- foco restaurado ao fechar/maximizar painéis e overlays.

---

## 12. Riscos e mitigação

| Risco | Severidade | Mitigação |
|---|---:|---|
| Ação enviada para sessão errada | Crítica | APIs target-aware; capturar IDs no início; testes concorrentes |
| Colisão ou perda de draft | Alta | chave por sessão/painel; migração e promoção atômica |
| Prompt otimista cruzado | Alta | mapa por sessão/painel, nunca singleton |
| Inspector mostra contexto incorreto | Alta | derivar de `focusedPaneId`; validar workspace e sessão capturados |
| Duplicação pesada de queries/effects | Alta | extrair `ConversationPane`; manter serviços globais únicos |
| Quatro streams degradam a WebView | Média/alta | seletores por sessão; janela de mensagens; profiling antes da Fase 3 |
| Agentes alteram o mesmo worktree | Alta | indicador e confirmação contextual; recomendar isolamento |
| Layout inutilizável em tela pequena | Média | breakpoints, overlay e maximize automático sem apagar layout |
| Estado persistido fica inválido | Média | payload versionado, validação e fallback determinístico |
| Escopo cresce para um sistema de docking genérico | Média | limitar a chat 2×2; superfícies globais seguem o foco |

### Sinais de que a implementação está virando gambiarra

A implementação deve ser interrompida/reavaliada se:

- renderizar múltiplos `SessionWorkbench` completos;
- mudar `selectedSessionId` temporariamente para executar ação de outro painel;
- depender do foco DOM para descobrir o destino de um comando assíncrono;
- usar uma única chave de draft para dois composers montados;
- desmontar/remontar timelines a cada troca de foco;
- duplicar o Inspector dentro de cada folha;
- liberar cross-workspace antes das ações serem target-aware;
- corrigir races com delays, `setTimeout` ou sincronização visual.

---

## 13. Rollout e observabilidade

### Feature flag

O split deve nascer atrás de uma flag local de desenvolvimento e depois de uma preferência
experimental. O modo de chat único continua sendo o fallback.

### Métricas úteis

- quantidade de layouts criados;
- distribuição de 2, 3 e 4 painéis;
- uso de maximize/restore;
- sessões simultaneamente ativas;
- tempo de commit React durante streaming;
- memória aproximada por quantidade de painéis;
- falhas ao reidratar layout;
- ações canceladas por contexto de painel inválido;
- alertas de worktree compartilhado apresentados.

Não registrar conteúdo de prompt, resposta, arquivo ou diff.

### Fallback

Se ocorrer erro de layout ou hidratação:

1. preservar sessões e drafts;
2. descartar somente o estado visual do split;
3. abrir a última sessão focada no workbench normal;
4. registrar diagnóstico local sem bloquear o app.

---

## 14. Estimativa de esforço

Estimativa apenas para planejamento, sujeita a revisão após a Fase 0:

| Entrega | Ordem de grandeza |
|---|---|
| Fase 0 — estado e ações target-aware | 3–5 dias de engenharia |
| Fase 1 — dois painéis, mesmo workspace | 3–5 dias |
| Fase 2 — quatro painéis, DnD, persistência e acessibilidade | 4–7 dias |
| Fase 3 — cross-workspace e superfícies contextuais | 5–10 dias |
| Fase 4 — hardening e rollout | 3–5 dias |

Uma entrega completa e polida representa aproximadamente **três a cinco semanas para uma pessoa**.
O primeiro marco útil — dois chats corretos no mesmo workspace — pode chegar antes, sem assumir a
dívida de uma implementação puramente visual.

---

## 15. Critérios de aceite finais

- [ ] Até quatro chats podem permanecer visíveis num grid 2×2.
- [ ] Cada chat mantém timeline, scroll, draft e composer independentes.
- [ ] É possível enviar mensagens para painéis diferentes sem mudar de workspace na sidebar.
- [ ] Dois ou mais streams podem atualizar simultaneamente sem cruzar estado.
- [ ] Todas as ações mutáveis usam `sessionId`/`workspaceId` explícitos.
- [ ] Inspector e review de turno seguem somente o painel focado.
- [ ] Git, terminal e delivery usam o workspace correto do painel focado.
- [ ] Fechar um painel não fecha nem arquiva a sessão.
- [ ] Maximizar/restaurar preserva o layout e os drafts.
- [ ] O layout sobrevive ao restart e degrada com segurança quando uma sessão desaparece.
- [ ] Há alternativa acessível ao drag-and-drop.
- [ ] O modo de chat único continua funcional e disponível como fallback.
- [ ] Quatro painéis ativos não provocam degradação inaceitável de memória ou digitação.
- [ ] O usuário é alertado sobre sessões mutáveis simultâneas no mesmo worktree.

---

## 16. Questões a decidir antes da Fase 2

1. Qual largura e altura mínimas habilitam 2×2?
2. Ao navegar na sidebar, o clique substitui o painel focado ou maximiza/abre fora do split?
3. Um workspace pode aparecer em mais de um painel com sessões diferentes?
4. O segundo agente no mesmo worktree exige apenas aviso ou confirmação explícita?
5. O Inspector restaura sua seleção por painel ou abre sempre em um resumo seguro?
6. O terminal deve permanecer aberto ao trocar foco entre workspaces ou pedir confirmação quando há
   comando/processo ativo?
7. Layouts são globais, por projeto ou nomeáveis pelo usuário?
8. Uma nova sessão criada a partir de um painel vazio herda provider/model do workspace, do painel
   focado ou apresenta picker próprio?

Essas decisões não bloqueiam a Fase 0. Elas devem ser fechadas após testar dois painéis reais, quando
os trade-offs de densidade e navegação estiverem observáveis.
