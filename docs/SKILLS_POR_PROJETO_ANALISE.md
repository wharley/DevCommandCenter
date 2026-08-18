# Skills por repositório/projeto no DCC — Análise de viabilidade

## Pergunta de origem

> Faz sentido o DCC suportar **skills por repositório/projeto**? É um ganho real e
> um diferencial, dada a arquitetura atual? Onde armazenar — no repo ou no `.dcc`?
> Como isso se comporta entre Claude, Codex, Gemini e demais agentes?

Este documento responde com base no código atual do DCC, não em hipóteses.

---

## Conclusão executiva

**Sim, vale a pena — mas não como você provavelmente imagina.**

Reimplementar "skills" como feature própria do DCC tem ganho **próximo de zero**: para
Claude e Codex, skills de projeto **já funcionam nativamente** (ver §3). O diferencial real não
é *ter* skills — é o DCC ser o **compilador de contexto único entre múltiplos agentes**:
o usuário escreve a skill **uma vez**, e o DCC a projeta para o formato nativo de cada
provider no momento em que o worktree é montado.

Resumo de uma linha: **não construa um "motor de skills"; construa um "context compiler".**

| Caminho | Veredito |
|---|---|
| DCC reimplementar skills do zero | ❌ Não vale — duplica o que os runtimes já fazem |
| DCC ter UI para gerenciar skills nativas do repo | ✅ Vale — baixo custo, alto polimento |
| DCC compilar uma fonte única → formato nativo de cada provider | ✅✅ É o diferencial |
| Guardar a fonte de verdade no `.devcommandcenter/` | ✅ Correto — é a pasta que o DCC possui |
| Esperar o Claude ler skills de dentro do `.dcc` | ❌ Impossível — ver §4 |

---

## 1. O que é uma "Skill" hoje (e o que não é)

Uma **Skill** (no sentido Claude Code/Agent SDK) é uma pasta com um `SKILL.md`:

```
.claude/skills/
  revisar-pr/
    SKILL.md          # frontmatter (name, description) + corpo
    scripts/          # opcional: código auxiliar
```

A característica que a diferencia de um simples arquivo de instruções é o
**progressive disclosure**: o agente carrega no contexto apenas o `name` + `description`
(frontmatter) de todas as skills; o **corpo só entra no contexto quando a skill é
acionada**. Isso é o que mantém o custo de tokens baixo mesmo com dezenas de skills.

Isso **não** é a mesma coisa que:

- **`CLAUDE.md`** — instruções *sempre* no contexto, sem disclosure. Lido automaticamente.
- **`AGENTS.md`** — equivalente cross-tool do `CLAUDE.md`; também sempre no contexto.
- **Slash commands / subagents** — outros artefatos de `.claude/`, fora do escopo aqui.

Reter este ponto é crítico: **progressive disclosure é um recurso do runtime do Claude.**
Nenhum outro CLI hoje o replica fielmente (ver §3). Qualquer "skill cross-provider" que
o DCC gere para Codex/Gemini será, na prática, **instrução estática** — funcional, mas
sem o carregamento sob demanda. É uma projeção *lossy*, e isso precisa ser assumido.

---

## 2. Como o DCC monta o contexto de um agente hoje

Fatos verificados no código:

1. **Worktree-first.** Cada Comb/Mission = um git worktree isolado. Arquivos *versionados*
   (incluindo `.claude/`, `CLAUDE.md`, `AGENTS.md`) aparecem automaticamente no worktree —
   é a mesma árvore do Git. Arquivos *ignorados* não.

2. **Hook de setup.** `.devcommandcenter/config.json` declara
   `setup: ["./.devcommandcenter/setup-worktree.sh"]`, executado a cada worktree criado.
   Hoje ele só resolve `.env` e `node_modules`. **É o ponto de injeção natural** para
   qualquer passo de "compilar contexto".

3. **`homePath` / `shadowHomePath` por provider.** `provider-runtime-settings.ts` expõe,
   para `claude_code`, `codex` e `gemini`, a capacidade de **sobrescrever o HOME** do
   processo do agente. Ou seja, o DCC controla *de onde* cada agente lê sua config
   **global** (`~/.claude/`, `~/.codex/`, `~/.gemini/`) — sem tocar no repositório.

4. **`settingSources` já inclui `project`.** No sidecar (`sidecar/src/index.mjs:343`), a
   chamada ao Agent SDK passa `settingSources: ["user", "project", "local"]`. Isso é
   determinante — ver §3.1.

Esses quatro pontos já existem. **O DCC não precisa de nova infraestrutura** para o que
esta análise propõe; precisa de uma camada de compilação **em cima** deles.

---

## 3. Como cada provider lê contexto de projeto

### 3.1. Claude Code / Agent SDK — **skills de projeto JÁ funcionam**

Como o provider `claude_code` roda via Agent SDK com `settingSources` incluindo
`"project"`, o SDK descobre e carrega automaticamente `.claude/skills/` do diretório de
trabalho — que, no DCC, é a raiz do worktree. **Conclusão direta: hoje, sem nenhuma
mudança, se você comitar `.claude/skills/foo/SKILL.md`, o Claude do DCC já enxerga a
skill.** Idem `CLAUDE.md`.

Implicações:
- Para o Claude, **não há feature a construir** — há UX a polir (listar, criar, editar,
  ligar/desligar skills sem sair do DCC).
- O único risco real é regressão: se algum dia o `settingSources` mudar para `["user"]`,
  as skills de projeto somem silenciosamente. Vale um teste de fumaça que proteja isso.

### 3.2. Codex — `.agents/skills/`, com progressive disclosure

O Codex atual descobre skills de repositório em `.agents/skills/` desde o diretório de
trabalho até a raiz do repo. Ele carrega inicialmente nome, descrição e caminho; o corpo
de `SKILL.md` entra no contexto quando a skill é escolhida explícita ou implicitamente.
`AGENTS.md` continua válido para instruções sempre ativas e para compatibilidade com
outros agentes, mas já não é o alvo correto para uma skill nativa do Codex.

### 3.3. Gemini CLI — `GEMINI.md`

`GEMINI.md` no repo + `~/.gemini/`. Instrução estática, sem
disclosure. O headless adapter do DCC já invoca o Gemini com `--prompt`, então uma skill
"para Gemini" vira texto de instrução, não artefato estruturado.

### 3.4. Cursor — `.cursor/rules/*.mdc`

Cursor usa regras em `.cursor/rules/`, com frontmatter que permite escopo por glob e
ativação automática/manual. É o formato de terceiro que **mais se aproxima** de skills.
Observação: `.cursor` está hoje no `.gitignore` do DCC — qualquer geração para Cursor
precisaria reconsiderar isso ou gravar fora do ignore.

### 3.5. Droid (Factory) e demais

Convergem para `AGENTS.md` como padrão emergente cross-tool. Tratar como alvo legado
separado do Codex nativo.

### Tabela-resumo

| Provider | Arquivo de projeto | Progressive disclosure? | Skills nativas? |
|---|---|---|---|
| Claude Code | `.claude/skills/`, `CLAUDE.md` | ✅ Sim | ✅ Sim (já ativo no DCC) |
| Codex | `.agents/skills/`, `AGENTS.md` | ✅ Sim | ✅ Sim |
| Gemini | `GEMINI.md` | ❌ Não | ❌ Não |
| Cursor | `.cursor/rules/*.mdc` | ⚠️ Parcial (por glob) | ⚠️ Parcial |
| Droid/outros | `AGENTS.md` | ❌ Não | ❌ Não |

---

## 4. A confusão do `.dcc` — por que não dá, e qual é a saída

A intuição "guardar tudo no `.dcc` e o agente lê de lá" **não funciona**, e a razão é
simples: cada CLI tem **caminhos de descoberta fixos** (`.claude/`, `AGENTS.md`,
`GEMINI.md`...). Eles não conhecem — e não vão conhecer — uma pasta `.devcommandcenter/`.
O agente nunca vai *ler* de dentro do `.dcc`.

Mas a saída é justamente essa distinção:

- **`.devcommandcenter/skills/` = FONTE DE VERDADE.** É onde *o humano e o DCC* editam.
  Versionado, compartilhado com o time, formato único do DCC.
- **`.claude/skills/`, `.agents/skills/`, `AGENTS.md`, `GEMINI.md` = ARTEFATOS COMPILADOS.** Gerados pelo DCC
  a partir da fonte. É *daqui* que os agentes leem.

O `.dcc` é o **código-fonte**; os arquivos nativos são o **build**. O agente nunca lê o
código-fonte — lê o build. Essa é exatamente a relação que resolve a sua dúvida.

---

## 5. Onde está o ganho real (e onde NÃO está)

**Não está** em: dar skills ao Claude (já tem) ou em criar mais um formato de skill.

**Está** em três coisas que **nenhum CLI isolado faz**:

1. **Fonte única, múltiplos alvos.** Escrever a skill `revisar-pr` uma vez e tê-la
   disponível, no formato certo, para Claude *e* Codex *e* Gemini. Hoje, um time que usa
   3 agentes mantém 3 cópias divergentes de instrução. O DCC elimina isso.

2. **Skill como objeto de primeira classe na UI.** Listar, criar, versionar, ligar/
   desligar skills por Comb — em vez de editar arquivos de config soltos. Isso transforma
   "contexto do agente" em algo *gerenciável*, coerente com a tese do DCC de ser um
   "gerenciador de estado de engenharia".

3. **Skills globais por provider sem sujar o repo.** Via `shadowHomePath`, o DCC pode
   injetar skills em `~/.claude/skills/` (ou `~/.agents/skills/`) de um HOME-sombra controlado —
   skills pessoais/da org que **não** vão para o `.git` do projeto.

O diferencial do DCC, portanto, não é "ter skills". É ser a **camada de normalização de
contexto entre agentes heterogêneos** — coerente com o que o README já promete
("abstração completa para CLI Agents").

---

## 6. Arquitetura proposta — o "Context Compiler" do DCC

```
  FONTE DE VERDADE                  COMPILADOR                ARTEFATOS NATIVOS
  (versionado, formato DCC)         (DCC)                     (lidos pelos agentes)

  .devcommandcenter/skills/
    revisar-pr/
      SKILL.md          ──────►   dcc context compile  ──┬──► .claude/skills/revisar-pr/
      scripts/                    (no hook de setup        │      SKILL.md  (cópia fiel)
  .devcommandcenter/                do worktree, ou ao     │
    context.json        ──────►    iniciar a sessão)       ├──► AGENTS.md
    (quais skills, escopo,                                 │      (## Skill: revisar-pr ...
     quais providers)                                      │       corpo achatado)
                                                           ├──► .agents/skills/revisar-pr/
                                                           │      SKILL.md  (Codex nativo)
                                                           │
                                                           └──► GEMINI.md  (idem)
```

### Componentes

1. **Fonte:** `.devcommandcenter/skills/<nome>/SKILL.md` — formato compatível com o do
   Claude (frontmatter `name`/`description` + corpo). Escolher o formato do Claude como
   fonte é estratégico: a compilação para Claude e Codex vira **cópia identidade**
   (custo zero, sem perda), e só os alvos sempre ativos exigem transformação.

2. **Manifesto:** `.devcommandcenter/context.json` — declara quais skills estão ativas,
   para quais providers cada uma se aplica, e escopo (glob de arquivos, por Comb).

3. **Compilador:** uma etapa nova invocada pelo `setup-worktree.sh` (ponto de injeção
   que **já existe**). Para cada provider configurado no Comb:
   - **Claude:** copia/symlinka `.devcommandcenter/skills/*` → `.claude/skills/*`.
   - **Codex:** copia a skill para `.agents/skills/*`, preservando progressive disclosure.
   - **Droid/legado e Gemini:** concatena os corpos das skills aplicáveis numa seção
     gerada e delimitada de `AGENTS.md` / `GEMINI.md` (entre marcadores tipo
     `<!-- dcc:skills:start -->` … `<!-- dcc:skills:end -->`, para reescrita idempotente).
   - **Cursor:** gera `.cursor/rules/<nome>.mdc`.

4. **Decisão de versionamento:** os artefatos compilados podem ser **gitignorados**
   (build descartável, regenerado pelo hook) ou **comitados** (o time vê o resultado no
   PR). Recomendação: gitignorar por padrão, com opção de comitar — análogo a `dist/`.

5. **Skills globais:** as que não pertencem a um projeto vão para o HOME-sombra do
   provider (`shadowHomePath`), sem nunca tocar o repo.

---

## 7. Limitações honestas e riscos

Nenhuma decisão é boa sem o lado ruim explícito:

1. **Compilação lossy nos alvos sempre ativos.** Gemini e consumidores legados de
   `AGENTS.md` não têm progressive disclosure. Skills projetadas para eles incham o
   contexto: 20 skills viram 20 blocos de instrução sempre presentes. Mitigação: o
   manifesto deve permitir ativar poucas skills por Comb, não todas.

2. **Custo de manutenção do transpiler.** São 4–5 formatos de CLI, todos em evolução
   rápida. Cada mudança upstream pode quebrar a compilação. É manutenção contínua real.

3. **Risco de convergência upstream.** `AGENTS.md` já é padrão cross-tool de *instruções*.
   Se a indústria padronizar também um formato de *skill* portável, parte do compilador
   do DCC vira redundante. Mitigação: manter a camada **fina** — não investir pesado em
   abstração antes de demanda comprovada.

4. **Para o Claude isolado, o ganho de compilação é zero.** Se o usuário só usa Claude,
   ele não precisa de nada disto — `.claude/skills/` comitado já basta. O valor só
   aparece em times/fluxos **multi-agente**. Se o uso real do DCC for majoritariamente
   single-agent-Claude, o ROI desta feature cai bastante.

5. **Conflito de escrita em arquivos compartilhados.** Gerar dentro de `AGENTS.md` exige
   marcadores idempotentes e cuidado para não sobrescrever conteúdo escrito à mão.

---

## 8. Roadmap faseado (do barato ao caro)

**Fase 0 — Proteger o que já funciona (custo: mínimo).**
Teste de fumaça garantindo que o provider Claude mantém `settingSources` com `project`,
para que skills de repo nunca quebrem silenciosamente.

**Fase 1 — UI de skills do Claude (custo: baixo, valor: imediato).**
Tela no DCC para listar/criar/editar/ligar/desligar skills em `.claude/skills/` do
worktree. Sem compilador ainda — só gestão visual do que o Claude já consome.

**Fase 2 — Fonte única + compilador para Claude (custo: baixo).**
Introduzir `.devcommandcenter/skills/` como fonte; compilação para `.claude/skills/`
(cópia identidade) no hook `setup-worktree.sh`. Valida o pipeline com perda zero.

**Fase 3 — Compilação cross-provider (custo: médio, é o diferencial).**
Estender o compilador para `AGENTS.md` (Codex/Droid) e `GEMINI.md`. Manifesto
`context.json` controlando escopo e providers-alvo.

**Fase 4 — Skills globais via `shadowHomePath` (custo: médio).**
Skills pessoais/da org injetadas no HOME-sombra de cada provider, fora do repo.

Cada fase entrega valor sozinha — dá para parar em qualquer ponto se o ROI não aparecer.

---

## 9. Veredito final

**É viável e vale a pena**, com a ressalva central: o produto não é "skills" — é o
**compilador de contexto cross-provider**. Skills para o Claude já são nativas; o DCC não
deve competir com isso, deve *embrulhar*. O diferencial defensável é "escreva o contexto
do agente uma vez, rode em qualquer agente", apoiado em três peças que o DCC **já tem**:
o hook `setup-worktree.sh`, o isolamento por worktree e o `shadowHomePath` por provider.

Recomendação prática: começar pelas Fases 0–2 (baixo custo, valida a tese) e só investir
nas Fases 3–4 se o uso multi-agente do DCC se confirmar. Não construir o transpiler
completo de antemão — o risco de convergência upstream (§7.3) pede uma camada fina.
