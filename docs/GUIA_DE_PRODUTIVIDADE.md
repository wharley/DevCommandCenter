# 🧠 Guia de Produtividade: Dev Command Center (DCC)

Este guia detalha como utilizar o DCC para maximizar sua eficiência como desenvolvedor, explorando os conceitos de isolamento de contexto, execução paralela e automação assistida.

---

## 1. O Conceito de Worktree-per-Task

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

## 2. Execução Multimodal com Panes

Diferente de um terminal comum, o DCC permite que múltiplos processos (humanos e agentes) colaborem no mesmo diretório.

### Quando usar vários Panes no mesmo Workspace?

| Tipo de Pane | Função | Cenário de Uso |
| :--- | :--- | :--- |
| **Agent (Claude/Gemini)** | Execução Proativa | "Refatore este componente para usar Tailwind." |
| **Terminal (Workspace)** | Validação e Logs | Rodar `npm test --watch` enquanto o agente trabalha. |
| **Terminal (Base)** | Referência Estática | Consultar arquivos na branch `main` sem sair do contexto atual. |
| **Segundo Agent** | Especialização | Usar um segundo agente para gerar documentação enquanto o primeiro escreve código. |

### Fluxo de Trabalho Sugerido:
1.  **Pane 1 (Agent):** Delegue a tarefa principal (ex: criar uma API).
2.  **Pane 2 (Terminal):** Deixe um processo de lint ou testes rodando.
3.  **Pane 3 (Base Terminal):** Use para dar `cat` ou `grep` em padrões de projeto que você quer que o agente siga.

---

## 3. Sistema de Atenção (Attention Heuristic)

O DCC monitora a saída dos terminais e agentes para você. 

- **Notificações:** Quando um agente para de escrever ou emite um padrão de "sucesso/erro", o DCC marca o Workspace com um badge azul e envia uma notificação.
- **Multitarefa Real:** Você pode estar em outro aplicativo ou em outro projeto. O DCC te chama quando sua decisão humana é necessária.

---

## 4. Melhores Práticas para Devs

### 💡 Dicas de Especialista:

1.  **Nomes de Workspaces:** Use nomes descritivos como `feat-auth-stripe` ou `fix-header-mobile`. Isso ajuda na organização do sistema de arquivos `.dcc/worktrees/`.
2.  **BYOK (Bring Your Own Key):** Configure seus provedores (Claude, Gemini, OpenAI) na aba **Settings**. O DCC é agnóstico e permite que você use o melhor modelo para cada tipo de tarefa.
3.  **Limpeza Periódica:** Worktrees ocupam espaço em disco. Use a política de cleanup do DCC para remover workspaces de tarefas já finalizadas e mergeadas.
4.  **Uso do Terminal Base:** Sempre que precisar comparar "como era antes" sem usar o Git Diff, use o botão **Base Terminal**. Ele abre um terminal no diretório original do projeto (não no worktree), permitindo consultas rápidas.

---

## 5. Resumo do Fluxo de Alta Performance

1.  **Identifique a Tarefa:** Crie um Comb dedicado.
2.  **Inicie o Agente:** Dê o contexto inicial e a missão.
3.  **Monitore via Logs:** Abra um terminal paralelo no mesmo workspace para ver o impacto em tempo real (ex: logs do servidor).
4.  **Aja sob Demanda:** Responda às notificações de atenção do DCC.
5.  **Revise e Commite:** Valide o diff final e finalize a missão.

---

*O Dev Command Center transforma o terminal de uma ferramenta passiva em um orquestrador ativo da sua engenharia.*
