# Spec-Driven Development no DCC — Análise de viabilidade

## Pergunta de origem

> O Spec-Driven Development (SDD) está virando tendência, mas poucas ferramentas o
> implementam (encontrei só uma — o `spec-kit`). Como é uma **metodologia**, talvez não
> seja responsabilidade de ferramentas de agentes/orquestradores. **Faz sentido** o DCC
> implementar SDD? Se fizer, como? Se não, basta dizer que não.

Este documento responde com base no código atual do DCC, não em hipóteses. Referências
cruzadas com [`SKILLS_POR_PROJETO_ANALISE.md`](./SKILLS_POR_PROJETO_ANALISE.md), porque a
resposta correta aqui é uma aplicação direta da mesma tese.

---

## Conclusão executiva

**Sim, faz sentido — mas não como o `spec-kit` faz, e não como "implementar uma metodologia".**

O erro que a pergunta quase comete é tratar SDD como um bloco único. Ele não é. SDD tem
duas metades, e elas têm donos diferentes:

- **A disciplina** — *saber escrever* uma boa spec, decidir critérios de aceitação,
  manter o constitution. Isso é metodologia. **Não é responsabilidade do DCC.** É do
  humano (com ajuda de templates e do próprio agente).
- **O estado** — *onde a spec vive*, garantir que ela é versionada, que sobrevive à
  compactação de contexto, que é re-injetada no agente certo, que as tasks derivadas dela
  são rastreadas. Isso é **gerência de estado de engenharia** — e é *exatamente* o que o
  README do DCC diz que o produto é ("o DCC é um gerenciador de estado de engenharia").

O `spec-kit` conflui as duas metades: entrega a metodologia *empacotada como slash
commands acoplados ao Claude*. O DCC deve fazer o oposto — **possuir só a metade de
estado** e deixar a disciplina como conteúdo editável. E há um fato decisivo: **o DCC já
implementa metade do SDD hoje** (o fluxo `plan → implement`, ver §2). SDD não é uma
feature nova; é completar um eixo que já existe — adicionando o estágio *a montante*
(a spec) e o *a jusante* (validação contra critérios).

Resumo de uma linha: **não construa um "motor de metodologia SDD"; trate a spec como o
artefato de primeira classe que falta numa Mission — o "upstream" do plano que o DCC já
tem.**

| Caminho | Veredito |
|---|---|
| DCC implementar SDD como metodologia rígida (constitution, gates obrigatórios) | ❌ Não — colide com o público sênior/velocidade do README |
| DCC clonar o `spec-kit` (slash commands acoplados a um agente) | ❌ Não — reinventa, e amarra a um provider |
| DCC tratar a **spec como artefato versionado da Mission** (upstream do plano) | ✅✅ É o encaixe — completa um fluxo que já existe |
| DCC compilar a spec → contexto nativo de cada provider | ✅✅ É o diferencial — mesma tese do "context compiler" |
| Ligar passos do plano de volta aos critérios de aceitação da spec | ✅ Vale — fecha o loop de validação |
| Guardar a fonte de verdade da spec no worktree, versionada em Git | ✅ Correto — Mission = worktree = spec |

---

## 1. O que é SDD — e o que não é

SDD substitui o *vibe coding* por um pipeline de quatro estágios, cada um produzindo um
artefato durável:

```
  SPEC            →   PLAN           →   TASKS              →   IMPLEMENT
  o "o quê/porquê"    o "como"           decomposição           execução +
  requisitos,         arquitetura,       atômica, testável      validação contra
  critérios de        decisões                                  os critérios
  aceitação
```

O ponto que a indústria de fato descobriu não é "escrever documento antes de codar" —
isso é velho. É outro: **a janela de contexto do agente é volátil, e a spec é o
ancoradouro que sobrevive a ela.** Quando o agente compacta o contexto (o DCC tem
`/compact` em `default-slash-commands.ts`), o que ele esquece? Os requisitos. A spec é o
artefato que pode ser **re-injetado** para reancorar o agente. Esse é o valor real, e é um
problema de *estado* — não de prosa.

SDD **não é**:
- **`CLAUDE.md` / `AGENTS.md`** — instruções *permanentes e genéricas* do repo. A spec é
  *efêmera e específica de uma Mission*. São camadas distintas.
- **Plan mode do agente** — o plano é o estágio 2. Ele é derivado da spec; não a
  substitui. Hoje o DCC tem o plano sem a spec a montante (ver §2).
- **Uma metodologia que o DCC precise *impor*.** O DCC fornece o trilho; o usuário decide
  se anda nele.

---

## 2. O que o DCC já faz hoje (e por que isso muda a resposta)

Fatos verificados no código — o DCC **já implementa os estágios 2–4 do SDD**:

1. **Plano como artefato estruturado.** `features/panel/plan-content.ts` parseia o plano
   do agente (markdown *ou* JSON), extrai `title`, `summary` e `steps[]` — e cada step
   tem **status** (`completed` / `in_progress` / `pending`). Isso é, na prática, uma
   lista de tasks rastreada.

2. **Plano vira artefato exportável.** `buildPlanMarkdownFilename`,
   `downloadPlanAsTextFile`, `normalizePlanContentForExport` — o plano já pode ser salvo
   como `.md`. `PlanReviewCard.tsx` tem botões de copiar/baixar/salvar.

3. **Loop plano → implementação.** `buildPlanImplementationPrompt` gera
   `"PLEASE IMPLEMENT THIS PLAN: …"`; `derivePlanFollowUpState` (`plan-follow-up.ts`)
   detecta um plano não-executado e `ComposerPlanFollowUpBanner.tsx` oferece
   "implementar em nova thread". O DCC **já reinjeta** um artefato de planejamento como
   prompt de execução.

4. **Mission = worktree.** Cada Comb/Mission é um worktree Git isolado (README + doc de
   skills §2). Arquivos versionados aparecem nele automaticamente.

**Conclusão direta:** SDD no DCC não é construir do zero. Os estágios *Plan → Tasks →
Implement* já existem em forma embrionária. O que falta é:

- **O estágio 1, a Spec** — o input *a montante* do qual o plano deveria ser derivado.
  Hoje o plano nasce direto de um prompt solto no composer.
- **O loop de validação** — ligar os `steps` do plano de volta a critérios de aceitação
  declarados, para "pronto" significar "satisfez a spec", não "o agente parou".

Isso reduz drasticamente o custo e o risco da feature: é *extensão* de um eixo existente,
não invenção de um subsistema.

---

## 2.1. Evidência de campo — o caso que confirma a tese

Esta seção registra um relato real de um desenvolvedor (fora da equipe do DCC) que, sem
conhecer este documento, montou *na mão* um fluxo SDD — e esbarrou exatamente nos limites
que a §2 prevê. É a melhor evidência possível: a dor não é hipótese de roadmap, é prática.

**O setup dele.** Criou agentes conectados ao ClickUp para ler uma task, e fez um agente
(Claude) gerar a spec da task. Como a task era grande, a spec ficou subdividida em fases,
cada fase implementada por vez.

**O que deu errado.** No meio de uma fase, o contexto do Claude esgotou. Ele trocou para
o Codex, reenviou o prompt inicial e mandou o Codex **reler a spec que o Claude havia
criado** para descobrir onde parar. O Codex retomou a fase corrente — mas **não avançou
para as fases seguintes**.

**Tradução para este documento:**

| O que ele fez na mão | Onde o DCC já mapeia |
|---|---|
| Spec em `.md` gerada a partir da task | §6 — fonte de verdade versionada |
| Task grande quebrada em fases executadas uma a uma | §1 (estágio TASKS) + §2 item 1 — steps com status |
| Contexto esgotou no meio → trocou Claude → Codex | §5 item 4 + Fase 5 — portabilidade cross-provider |
| Reenviou prompt e mandou reler a spec para reancorar | Fase 4 — re-injeção pós-compactação (feita à mão) |

Cada workaround dele é uma Fase do roadmap (§8) executada manualmente porque a ferramenta
ainda não a oferece. Isso valida o §3: a spec como **arquivo solto** funciona como
*gerador de prompt*, não como *gerente de estado*.

**O achado mais valioso — o resume cross-fase.** A spec solta fez o novo agente *retomar
a fase corrente*, mas não o fez *avançar para as próximas*. Reancorar ≠ continuar. Um
`.md` passivo não garante que o agente leia o status de todas as fases e prossiga da
próxima pendente. Isso só acontece quando alguém **possui o estado** e o reinjeta de
forma ativa — exatamente o papel do orquestrador, não do agente nem do arquivo. A §6 item
6 e a Fase 4 são estendidas abaixo para cobrir isso explicitamente.

**Uma divergência registrada.** Ele usa o ClickUp como fonte da task. Este documento põe
a fonte de verdade em `.devcommandcenter/specs/<mission>.spec.md` (§6). Não há conflito:
o tracker externo é o *input a montante* (a task original); a spec no worktree é o
*artefato durável compilado*. Integração com tracker externo é ponto de extensão natural,
fora do escopo deste documento.

---

## 3. `spec-kit` e `compozy` — o que fazem, e onde acoplam

- **`github/spec-kit`** — entrega o SDD como **slash commands** (`/specify`, `/plan`,
  `/tasks`, `/implement`) + templates de prompt + um `constitution.md`. É *bom*, mas tem
  duas características que o DCC **não deve copiar**:
  1. **Acoplado ao agente.** Os comandos são instalados na pasta de comandos de *um* CLI
     (Claude Code, Copilot, etc.). Reescrever a spec para 3 agentes = 3 instalações. É
     exatamente o problema multi-provider que o doc de skills (§5) identifica.
  2. **A spec vive como arquivo solto.** Nada garante que o agente a releia após uma
     compactação. O `spec-kit` é um *gerador de prompts*, não um *gerente de estado*.

- **`compozy`** — orquestrador de workflows de agentes (YAML, grafo de execução). Resolve
  *encadeamento de execução*, não *o contrato de requisitos*. É ortogonal ao SDD: você
  poderia rodar SDD dentro de um workflow do `compozy`. Não compete com a tese aqui.

A leitura honesta do mercado: o `spec-kit` provou que a *metodologia* tem demanda, mas a
implementou na camada errada (a do agente). **A camada certa é o orquestrador** — porque
só ele tem o estado durável (worktree, DB SQLite, sessões) e a visão multi-agente. Esse é
o espaço que o DCC pode ocupar, e quase ninguém ocupa.

---

## 4. A pergunta central: é responsabilidade de um orquestrador?

A intuição da pergunta — "metodologia talvez não seja responsabilidade da ferramenta" —
está **meio certa**, e vale dissecar onde:

| Aspecto do SDD | É metodologia ou estado? | Dono |
|---|---|---|
| *Como* escrever uma boa spec, o que é um bom critério | Metodologia | Humano / template / agente |
| O `constitution.md` (princípios do projeto) | Metodologia | Humano — é só um arquivo versionado |
| *Onde* a spec vive, e que está no Git | Estado | **DCC** |
| Garantir que a spec é re-injetada após compactação | Estado | **DCC** |
| Rastrear as tasks derivadas e seus status | Estado | **DCC** (já faz com `plan steps`) |
| Projetar a spec para Claude *e* Codex *e* Gemini | Estado / build | **DCC** (é o "context compiler") |
| Decidir se uma task "passou" nos critérios | Misto | Agente executa, DCC registra o veredito |

O DCC **não deve** ter opinião sobre *como* escrever a spec — deve fornecer um template
editável e sair do caminho. Mas *gerenciar o artefato spec* é literalmente a definição de
produto do DCC. A pergunta "é responsabilidade de um orquestrador?" se responde sozinha
quando você separa as duas metades: **a metodologia, não; o estado, sim — e é o core.**

---

## 5. Por que o DCC é a ferramenta *certa* para isso

Quatro peças que o DCC **já tem** e que tornam SDD um encaixe natural — não um enxerto:

1. **Worktree-first = isolamento natural da Mission.** Mission = worktree = uma spec. A
   spec mora em `.devcommandcenter/specs/<mission>.spec.md` (versionada, compartilhada
   com o time). Não há ambiguidade de "qual spec é desta tarefa" — é a do worktree.

2. **O fluxo de plano já existe (§2).** A spec se pluga *a montante* de `plan-content.ts`
   sem reescrever nada do downstream. O plano passa a ser derivado da spec; os `steps`
   passam a referenciar critérios de aceitação.

3. **`setup-worktree.sh` é o ponto de compilação.** `.devcommandcenter/config.json` já
   declara `setup: [...]` rodado a cada worktree. É onde a spec é "compilada" para o
   formato nativo de cada agente — exatamente o mecanismo do doc de skills §6.

4. **Multi-provider + `shadowHomePath`.** O DCC abstrai Claude/Codex/Gemini
   (`provider-runtime-settings.ts`, `sidecar/src/index.mjs`). Uma spec escrita uma vez,
   compilada para o contexto nativo de cada um — é o diferencial que nem o `spec-kit` nem
   o `compozy` têm. **SDD no DCC é o mesmo produto que o "context compiler" do doc de
   skills**, aplicado a um artefato diferente (spec, em vez de skill).

O composer também já tem slash commands extensíveis (`default-slash-commands.ts`), então
`/spec` pode ser um comando **nativo do DCC** — agnóstico de provider, ao contrário do
`spec-kit`.

---

## 6. Arquitetura proposta — a Spec como artefato de Mission

```
  FONTE DE VERDADE                 COMPILADOR (setup-worktree)      CONSUMO
  (versionada, formato DCC)                                         (agentes)

  .devcommandcenter/
    specs/
      <mission>.spec.md  ──────►   dcc spec compile  ──┬──► injeta no system prompt /
        frontmatter:               (no hook de setup    │    contexto do provider ativo
          status, criterios        do worktree, ou ao   │
        corpo: requisitos          (re)abrir a sessão)  ├──► seção marcada de AGENTS.md
                                                         │    (Codex/Gemini — lossy)
    constitution.md  ───────────►  sempre no contexto    │
    (princípios — opcional)        (como CLAUDE.md)      └──► re-injetável via /spec
                                                              após /compact
            │
            ▼  deriva
    plan-content.ts (JÁ EXISTE)  →  steps[] ligados a  spec.criterios[]  →  validação
```

### Componentes

1. **Fonte:** `.devcommandcenter/specs/<mission>.spec.md` — markdown com frontmatter
   (`status: draft|approved|implemented`, lista de `acceptance_criteria`). Um por
   Mission. Versionado no Git (vai no PR, o time revisa).

2. **Template, não motor.** O DCC fornece um `spec.template.md` e *só*. A "metodologia" é
   o template; o usuário edita à vontade. Zero lógica de processo embutida.

3. **Slash command nativo `/spec`** — abre/cria a spec da Mission no composer. Nativo do
   DCC (não instalado na pasta de comandos de um agente), portanto cross-provider.

4. **Spec → Plan (reuso puro).** O "implementar" do plano passa a ser
   `buildPlanFromSpecPrompt(spec)` em vez de prompt solto. `plan-content.ts` não muda —
   só ganha um input melhor a montante.

5. **Loop de validação.** Cada `ParsedPlanStep` pode referenciar um critério da spec.
   "Mission pronta" = todos os critérios marcados. O `PlanReviewCard` já renderiza status
   de step; estende-se para mostrar cobertura de critérios.

6. **Re-injeção.** Como a spec é um arquivo durável que o DCC possui, após um `/compact`
   o DCC pode reinjetá-la — resolvendo o problema de "o agente esqueceu os requisitos".
   *Isto* é o valor que nenhum arquivo solto entrega. Mais que reancorar: a re-injeção
   carrega o **status de todas as fases**, para o agente não só retomar a fase corrente
   mas *avançar para a próxima pendente* — o **resume cross-fase** que a §2.1 mostrou
   faltar num arquivo solto.

7. **Compilação cross-provider** — idêntica ao doc de skills §6: cópia/contexto direto
   para Claude; seção marcada (`<!-- dcc:spec:start -->`) em `AGENTS.md`/`GEMINI.md` para
   os demais (lossy, mas funcional).

Nada disto exige infraestrutura nova: worktree, hook de setup, `plan-content`, slash
commands e abstração de provider **já existem**.

---

## 7. Limitações honestas e riscos

1. **Cerimônia vs. velocidade.** O README vende velocidade e paralelismo para o sênior.
   SDD obrigatório seria fricção. Mitigação: a spec é **opt-in por Mission** — quem quer
   *vibe* num bugfix de 5 minutos não é forçado a escrever spec. O DCC oferece o trilho,
   não o impõe.

2. **A spec mal-escrita não salva ninguém.** Garbage in, garbage out — o DCC gerencia o
   artefato, não a qualidade dele. Honestamente: parte do ganho prometido pelo SDD
   depende de uma habilidade humana que o DCC não controla.

3. **Compilação lossy para Codex/Gemini.** Sem progressive disclosure, a spec vira
   instrução estática sempre presente (mesmo ponto do doc de skills §7.1). Specs longas
   incham o contexto desses agentes.

4. **Convergência upstream.** O *plan mode* dos próprios agentes vem absorvendo
   planejamento; é plausível que absorvam "spec" também. Mitigação: manter a camada
   **fina** — o DCC possui o *estado e a portabilidade*, não tenta ser mais esperto que o
   agente no *raciocínio*. O estado durável e cross-provider é o que o agente isolado
   nunca terá.

5. **Risco de duplicar `CLAUDE.md`.** Se a fronteira spec (efêmera, por Mission) vs.
   `CLAUDE.md` (permanente, do repo) não ficar clara na UX, o usuário coloca coisa no
   lugar errado. Exige design de UX explícito, não só de dados.

6. **Para o usuário single-agent-Claude, o ganho de *compilação* é baixo** — Claude já lê
   `.claude/` e tem plan mode. O ganho que sobra para ele é o *loop de validação* e a
   *re-injeção pós-compactação*. Real, mas menor. O diferencial pleno aparece em fluxo
   multi-agente — mesma ressalva do doc de skills §7.4.

---

## 8. Roadmap faseado (do barato ao caro)

**Fase 0 — Spec como arquivo (custo: mínimo).**
`spec.template.md` + convenção `.devcommandcenter/specs/<mission>.spec.md`. Sem código —
só documentação e um template. Valida se o time *quer* escrever specs antes de investir.

**Status no DCC atual:** implementado como `.devcommandcenter/spec.template.md` e
diretório versionado `.devcommandcenter/specs/`.

**Fase 1 — `/spec` no composer (custo: baixo).**
Slash command nativo que cria/abre a spec da Mission. Aproveita a infra de
`default-slash-commands.ts`. A spec já é editável dentro do DCC.

**Status no DCC atual:** implementado como comando nativo de cliente. Ele preenche o
composer com um prompt para criar/atualizar
`.devcommandcenter/specs/<branch>.spec.md`, sem depender de slash command do provider.

**Fase 2 — Spec alimenta o plano (custo: baixo).**
`buildPlanImplementationPrompt` → variante que parte da spec. Reuso quase total de
`plan-content.ts`. Aqui o eixo SDD fica completo: Spec → Plan → Tasks → Implement.

**Status no DCC atual:** implementado com `buildPlanFromSpecPrompt(...)`, aba `Spec` no
inspector e ação **Generate plan**, que envia a spec em `planMode: true`.

**Fase 3 — Loop de validação (custo: médio, é onde o SDD "fecha").**
`acceptance_criteria` no frontmatter; `ParsedPlanStep` referencia critérios;
`PlanReviewCard` mostra cobertura. "Mission pronta" = critérios satisfeitos.

**Status no DCC atual:** parcialmente implementado. O DCC extrai critérios `AC-*` da
spec e mostra cobertura no `PlanReviewCard` quando o plano referencia explicitamente os
IDs. A aba `Spec` também oferece **Validate**, que envia ao agente um prompt de auditoria
para inspecionar o worktree e responder `PASS` / `FAIL` / `UNKNOWN` por critério sem
alterar arquivos. A resposta de validação agora pede um bloco JSON
`dccMissionValidation`, que o frontend reconhece e renderiza como card estruturado.
Ainda falta persistir esse veredito como estado durável da Mission.

**Fase 4 — Re-injeção pós-compactação (custo: médio, é o valor único).**
O DCC reinjeta a spec após `/compact`. Resolve o esquecimento de requisitos —
o argumento mais forte e o mais difícil de um arquivo solto replicar. Inclui o
**resume cross-fase**: reinjetar não só os requisitos, mas o status de todas as fases,
para o agente continuar da próxima pendente — não apenas reancorar na corrente (§2.1).

**Fase 5 — Compilação cross-provider (custo: médio, é o diferencial).**
Compila a spec para `AGENTS.md`/`GEMINI.md`. Funde-se ao "context compiler" do doc de
skills — provavelmente o *mesmo* compilador, dois tipos de artefato.

Cada fase entrega valor sozinha. Dá para parar em qualquer ponto se o ROI não aparecer —
e a Fase 0 custa quase nada para descobrir isso.

### Status de implementação nesta branch

| Área | Status | Observação |
|---|---:|---|
| Spec versionada | ✅ | `.devcommandcenter/spec.template.md` + `.devcommandcenter/specs/` |
| `/spec` nativo | ✅ | Ação de cliente no composer, agnóstica de provider |
| Spec no inspector | ✅ | Aba `Spec` lista specs via comando Rust limitado ao diretório DCC |
| Spec → Plan | ✅ | `Generate plan` envia `buildPlanFromSpecPrompt(...)` em plan mode |
| Cobertura `AC-*` no plano | ✅ Parcial | Cobertura estrutural por referência explícita ao ID |
| Validação assistida | ✅ Parcial | `Validate` pede auditoria e card JSON `dccMissionValidation` |
| Persistência do veredito | ❌ | Próximo passo provável se o uso real justificar |
| Re-injeção pós-compactação | ❌ | Fase futura |
| Compilação cross-provider | ❌ | Fase futura |

---

## 9. Veredito final

**Faz sentido — e é mais barato e mais natural do que parece**, com uma ressalva central
de enquadramento:

O DCC **não deve "implementar a metodologia SDD"**. Deve fazer o que já é a sua definição
de produto: **gerenciar um artefato de estado de engenharia** — e a spec é só mais um,
*a montante* de um fluxo de plano que o DCC **já tem**. A pergunta "é responsabilidade de
um orquestrador?" tem resposta limpa quando se separam as duas metades do SDD: a
*disciplina* não é; o *estado* é — e é o core.

O `spec-kit` provou a demanda mas errou a camada (acoplou ao agente). O diferencial
defensável do DCC é o mesmo do doc de skills: **escreva o contrato uma vez, rode em
qualquer agente, e que ele sobreviva à compactação de contexto** — apoiado em quatro
peças que já existem: o worktree por Mission, o hook `setup-worktree.sh`, o pipeline de
plano (`plan-content.ts`) e a abstração multi-provider.

Recomendação prática atual: como as Fases 0–2 já estão implementadas e a Fase 3 começou
com cobertura estrutural de critérios e validação assistida estruturada, o próximo
investimento só deve persistir vereditos (`PASS` / `FAIL` / `UNKNOWN`) se o uso real de
specs confirmar o ROI. Não construir um "motor de SDD" — construir o trilho fino e
deixar o usuário (e o agente) fazerem a metodologia.
