# 🧠 Guia de Produtividade: Dev Command Center (DCC)

Este guia detalha como utilizar o DCC para maximizar sua eficiência como desenvolvedor, explorando os conceitos de isolamento de contexto, execução paralela e automação assistida.

Para um **mapa de recursos atuais** (daemon, `.dcc.toml`, processos supervisionados, tasks, paleta ⌘K, CLI/MCP) e **fluxos que cruzam estes benefícios**, veja também **[Guia de recursos e fluxos](GUIA_RECURSOS_E_FLUXOS_DCC.md)**.

---

## 1. Criar um workspace (Comb) — do zero ao worktree

O fluxo na app segue o resumo visual **Projeto → Workspace → Panes → Git/PR → Merge** (dica no diálogo “Novo Workspace”). Na prática:

1. **Escolhe o projeto** (repositório Git já registado no DCC).
2. **(Opcional) Preenche a partir de uma issue** — secção *Issue (GitHub / GitLab)*: cola o URL da issue ou `owner/repo#123` (GitHub), opcionalmente um token PAT para repos privados, e carrega. O DCC sugere **nome** e **descrição** do workspace; revê o texto antes de criar.
3. **Nome do workspace** — ao escrever, aparece a **pré-visualização de branch e pasta** (quando a app desktop expõe `previewWorktreeNaming`): vês o **prefixo de branch** do `.dcc.toml`, o nome sanitizado (caracteres não alfanuméricos viram hífen) e um **exemplo de sufixo** em hex (o sufixo real vem do ID do workspace no momento da criação).
4. **Branch base** — a lista de branches locais é carregada do repo; por defeito tenta alinhar com a branch atual ou `main`. Se não houver lista, podes escrever à mão.
5. **Descrição (opcional)** — notas humanas sobre a missão.
6. **Criar** — regista o Comb na base do DCC. O **worktree físico** pode ser criado sob demanda na primeira vez que abres um terminal/agente que precise do diretório (ação **garantir worktree** / `ensure`).

**Depois de criar:** na primeira utilização prática, corre o instalador de dependências **dentro da pasta do worktree** (não no clone principal), como já descrito na secção sobre `node_modules`.

---

## 2. O Conceito de Worktree-per-Task

O DCC foi projetado sob a filosofia de que **trocas de contexto matam a produtividade**. 

### O problema tradicional:
Você está no meio de uma refatoração complexa. Um bug crítico surge na `main`. Você precisa:
1. `git stash` ou fazer um commit temporário "wip".
2. `git checkout main`.
3. Corrigir o bug.
4. `git checkout feature`.
5. `git stash pop`.
6. Re-compilar o projeto e torcer para que o estado do ambiente não tenha quebrado.

### A solução DCC:
No DCC, você cria um **Comb (Workspace)** para o bug crítico.
- O DCC cria um **Git Worktree** em um diretório separado.
- Você tem dois ambientes físicos no disco rodando simultaneamente.
- O servidor da sua refatoração continua rodando. Você abre o novo Comb, corrige o bug, commita e fecha. **Zero interrupção.**

---

## 3. Execução Multimodal com Panes

Diferente de um terminal comum, o DCC permite que múltiplos processos (humanos e agentes) colaborem no mesmo diretório.

### Agentes (CLI)

- **Providers:** em **Settings / Providers**, regista os executáveis dos agentes (Codex, Claude Code, Cursor CLI, etc.). Só aparecem no diálogo “Novo Agent Pane” os **tipos CLI** ativos.
- **Agente por defeito no repo:** no cartão **Command Center** da sidebar lês o **agente padrão** definido no `.dcc.toml` (`defaultAgentProviderId`). Ao abrir um novo pane de agente, esse provider é pré-selecionado quando existe.
- **Criação do pane:** “Novo Agent Pane” escolhe o provider e cria um pane do tipo agente. Antes disso, o DCC **garante o worktree** — sem pasta de checkout o agente não arranca.
- **Terminal embutido:** o agente corre num PTY como os outros panes; se o CLI não estiver resolvido, a UI avisa para configurar o caminho em Providers.
- **Visão geral:** na sidebar, o bloco **Agentes detectados** (quando há sessões) mostra agentes com estado *working* / *waiting* e o `cwd`. Na lista de workspaces, badges resumem agentes ativos por Comb.

### Quando usar vários Panes no mesmo Workspace?

| Tipo de Pane | Função | Cenário de Uso |
| :--- | :--- | :--- |
| **Agent (CLI)** | Execução assistida | “Refatore este componente para usar Tailwind.” |
| **Terminal (Workspace)** | Validação e Logs | Rodar `npm test --watch` enquanto o agente trabalha. |
| **Terminal (Base)** | Referência Estática | Consultar arquivos na branch `main` sem sair do contexto atual. |
| **Segundo Agent** | Especialização | Um agente a codificar e outro a documentar ou rever. |

### Fluxo de Trabalho Sugerido:
1.  **Pane 1 (Agent):** Delegue a tarefa principal (ex: criar uma API).
2.  **Pane 2 (Terminal):** Deixe um processo de lint ou testes rodando.
3.  **Pane 3 (Base Terminal):** Use para dar `cat` ou `grep` em padrões de projeto que você quer que o agente siga.

---

## 4. Sistema de Atenção (Attention Heuristic)

O DCC monitora a saída dos terminais e agentes para você. 

- **Notificações:** Quando um agente para de escrever ou emite um padrão de "sucesso/erro", o DCC marca o Workspace com um badge azul e envia uma notificação.
- **Multitarefa Real:** Você pode estar em outro aplicativo ou em outro projeto. O DCC te chama quando sua decisão humana é necessária.

---

## 5. Revisão Git no DCC (painel Review)

O painel de revisão junta **diffs frente à branch base**, **classificação por ficheiro**, **Git na worktree e no repo principal** e, quando há ligação a PR/MR, **comentários de review**.

- **Árvore de ficheiros e diffs:** navega pelos ficheiros alterados, marca cada um como **OK**, **rever depois** ou **suspeito**; o estado guarda-se por workspace/target no `localStorage` (persistente entre sessões).
- **Trilha (trail):** registo curto de ações (commits, resets, merges) para contexto humano.
- **Fluxo recomendado na UI:** commit e push no **branch da Missão** (worktree); **repositório principal** sem alterações por commitar no momento do merge; depois **merge** para a branch de destino — a própria UI explica merge vs **patch** (“Aplicar” copia diffs para o checkout principal; uso mais pontual).
- **Multi-repo:** podes **adicionar outros projetos do Hive à mesma revisão** — cada um aparece como *target* com o seu `worktreePath` / checkout. Tokens extraídos dos diffs podem cruzar-se entre repos (**tokens entre repos**).
- **Comentários de PR/MR:** com `forge_link` e token, o painel pode mostrar comentários inline do GitHub/GitLab ao lado do código (dependendo do backend exposto na app).

Para política de paths e remoção de worktrees, continua a aplicar-se [WORKTREE_POLICY.md](WORKTREE_POLICY.md).

---

## 6. Atalhos de teclado (workspace)

Na área principal, **⌘** (macOS) ou **Ctrl** (Windows/Linux) é o modificador principal. **Segura o modificador** na sidebar para revelar dicas de atalho nos botões.

| Atalho | Ação |
|--------|------|
| **⌘K** / **Ctrl+K** | Abre a **paleta de comandos** (procurar workspace, pane, preset, task, etc.). |
| **⌘⇧K** / **Ctrl+Shift+K** | Com a paleta **não** em foco: **limpa o scrollback** do terminal ativo (evento interno `dcc-terminal-action`). |
| **⌘⇧N** / **Ctrl+Shift+N** | Novo workspace. |
| **⌘⇧T** / **Ctrl+Shift+T** | Novo terminal no **diretório do workspace** (worktree). |
| **⌘⇧A** / **Ctrl+Shift+A** | Novo **pane de agente** (requer Comb ativo). |
| **⌘⇧B** / **Ctrl+Shift+B** | Terminal **base** (clone principal). |
| **⌘⇧R** / **Ctrl+Shift+R** | Abre **configuração do repositório** (`.dcc.toml`). |
| **⌘⇧I** / **Ctrl+Shift+I** | **Notificações** (atenção). |
| **⌘⇧P** / **Ctrl+Shift+P** | Abre / alterna **Providers**. |
| **⌘[** / **Ctrl+[** | **Workspace anterior** (histórico de navegação entre Combs). |
| **⌘]** / **Ctrl+]** | **Workspace seguinte**. |
| **⌘⌥T** / **Ctrl+Alt+T** | Alternar tema (claro / escuro / sistema em ciclo). |
| **⌘⇧D** / **Ctrl+Shift+D** | Tema **escuro**. |
| **⌘⇧L** / **Ctrl+Shift+L** | Tema **claro**. |
| **⌘⇧S** / **Ctrl+Shift+S** | Tema **do sistema**. |
| **⌘1**–**⌘9** / **Ctrl+1**–**9** | Foco no **pane** correspondente (ordem dos separadores), com Comb ativo. |
| **⌘⇧[** / **⌘⇧]** (**Ctrl+Shift+[** / **]**) | Com **vários panes**, ciclo entre tabs (**anterior** / **seguinte**). |
| **⌘+** / **⌘-** / **⌘0** (**Ctrl+** / **-** / **0**) | Aumentar, diminuir ou repor **tamanho da fonte** do terminal. |

**⌘K** / **⌘⇧K** são tratados cedo (paleta e limpar scrollback) mesmo com foco noutros sítios. Os restantes atalhos **saltam** quando o alvo do teclado está num `input`, `textarea`, `select`, conteúdo editável ou **diálogo** — exceto no **xterm**, onde foco, zoom e alguns atalhos continuam a aplicar-se.

---

## 7. Melhores Práticas para Devs

### Dependências no worktree e limpeza após o merge

Cada worktree do DCC é um **checkout Git em pasta própria** (em geral `<raiz-do-projeto>/.dcc/worktrees/<branch>`). Arquivos ignorados pelo Git — em especial **`node_modules`** — **não são herdados** do clone principal: cada worktree precisa do seu próprio install naquele diretório.

- **Primeira vez (ou worktree novo):** dentro da pasta do worktree, rode o comando de instalação que o projeto usa — por exemplo `yarn install`, `npm ci` ou `pnpm install`. Siga o lockfile do repositório (`yarn.lock`, `package-lock.json`, `pnpm-lock.yaml`) para não misturar gerenciadores sem querer.
- **Depois disso:** não é necessário instalar de novo só por reabrir o terminal ou o workspace; só quando as dependências mudarem, você apagar `node_modules` ou criar **outro** worktree.

Quando o trabalho já estiver **integrado na branch principal** e você não precisar mais daquele checkout isolado:

- **Remova o workspace na sidebar** (ícone de lixeira na lista). O DCC descarta o worktree no Git e apaga a pasta correspondente no disco — **incluindo `node_modules`** daquele caminho — o que evita acumular cópias grandes de dependências em tarefas já encerradas.

Detalhes de caminho e política de worktree: [`WORKTREE_POLICY.md`](./WORKTREE_POLICY.md).

### 💡 Dicas de Especialista:

1.  **Nomes de Workspaces:** Use nomes descritivos como `feat-auth-stripe` ou `fix-header-mobile`. Isso ajuda na organização do sistema de arquivos `.dcc/worktrees/`.
2.  **BYOK (Bring Your Own Key):** Configure seus provedores (Claude, Gemini, OpenAI) na aba **Settings**. O DCC é agnóstico e permite que você use o melhor modelo para cada tipo de tarefa.
3.  **Limpeza após merge:** Quando a tarefa estiver na principal, remova o item da lista na sidebar (veja a subseção acima). Worktrees antigos ocupam espaço — principalmente por causa de `node_modules` duplicados.
4.  **Uso do Terminal Base:** Sempre que precisar comparar "como era antes" sem usar o Git Diff, use o botão **Base Terminal**. Ele abre um terminal no diretório original do projeto (não no worktree), permitindo consultas rápidas.
5.  **Paleta (⌘K):** além de ações globais, lista **projetos**, **workspaces**, **panes**, **processos** do `.dcc.toml`, **presets**, **templates** em `.dcc/tasks` e **tasks** agendadas — útil quando já não queres usar a sidebar.

---

## 8. Resumo do Fluxo de Alta Performance

1.  **Identifique a Tarefa:** Crie um Comb dedicado (opcionalmente a partir de uma issue).
2.  **Garanta o worktree** ao abrir terminal/agente; instale dependências na pasta do worktree na primeira vez.
3.  **Inicie o Agente ou terminais:** escolha provider por defeito do repo ou manualmente no diálogo.
4.  **Monitore via Logs:** Abra um terminal paralelo no mesmo workspace para ver o impacto em tempo real (ex: logs do servidor).
5.  **Aja sob Demanda:** Responda às notificações de atenção do DCC.
6.  **Revise no painel Review:** classifique ficheiros, confirme estado limpo no principal e na worktree, depois merge ou patch conforme o fluxo da equipa.
7.  **Após merge na principal:** Remova o workspace na sidebar para liberar disco (worktree + `node_modules` daquele checkout).

---

*O Dev Command Center transforma o terminal de uma ferramenta passiva em um orquestrador ativo da sua engenharia.*
