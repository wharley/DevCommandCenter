# Conceitos e Guia de Uso — Dev Command Center

Este documento explica **por que** o fluxo do Dev Command Center é assim e **como** usá-lo melhor: custo de tokens, revisão humana, missões pequenas e uma missão por projeto por vez.

---

## Conceitos

### Por que o plano é uma etapa separada?

O fluxo é **missão → plano → revisão → código → revisão → apply**. O plano não é “só um resumo”: é uma fase com três funções:

1. **Decidir antes de gastar**  
   O plano usa uma chamada barata (lista de arquivos + descrição; saída é um JSON). A geração de código é a chamada cara. Com o plano em etapa, você vê *o que será feito* e só gasta tokens pesados se aprovar.

2. **Revisão humana no meio**  
   Você vê o plano na tela (passos, arquivos). Pode aprovar e seguir para “Gerar código”, regenerar com feedback (“não incluir testes”, “só frontend”) ou cancelar. Assim você interrompe cedo se o rumo estiver errado, em vez de descobrir depois do código caro.

3. **Escopo claro para o código**  
   A geração de código recebe “implemente *este* plano”. O modelo não fica livre com contexto gigante; é guiado pelo plano. Menos risco de escopo creep e menos tokens desperdiçados.

**Resumo:** O plano existe para reduzir custo (tokens), permitir revisão antes de executar e fixar o escopo antes da parte cara. É “decidir antes de rodar”.

---

### Por que criar várias missões pequenas?

Quebrar o trabalho em **várias missões com escopo pequeno** é o uso ideal:

- **Janela de tokens baixa** — Cada missão = um plano (entrada pequena) e uma geração de código (escopo limitado). Várias missões pequenas = várias execuções com pico menor cada uma. Uma missão gigante = pico alto de tokens.

- **Não ficar preso** — Igual Kanban: tarefa gigante prende uma única corrida de IA por muito tempo. Missões pequenas: você termina uma, revisa, aplica e parte para a próxima. Se uma falhar, o resto não fica bloqueado na mesma corrida.

- **Revisão e controle** — Plano pequeno é mais fácil de revisar e de aprovar ou regenerar antes de gastar tokens em código.

**Boas práticas:** Descreva alterações focadas por missão (ex.: “adicionar validação no formulário X”, “ajustar estilo do botão Y”). Execute uma após a outra no mesmo projeto.

---

### Por que só uma missão por projeto por vez?

O app **não** permite duas missões em andamento no **mesmo** projeto ao mesmo tempo. O motivo é técnico e evita conflitos:

- As mudanças são aplicadas com **`git apply`** (patch unificado) no working tree do repositório.
- Cada patch foi gerado com base em um **estado do repositório** na hora do plano/código.
- Se duas missões rodassem em paralelo no mesmo repo, uma aplicaria primeiro; a outra aplicaria em cima de um estado já alterado. O diff da segunda foi calculado para o estado antigo → **contexto do patch não bate** → conflito ou apply errado.

**Conclusão:** “Uma missão por projeto por vez” existe para garantir base estável para o `git apply` e evitar conflitos. Não é só organização de fila.

---

### Como isso se conecta a custo e limites (Cursor, cloud)?

Plataformas cloud (ex.: Cursor Pro+, Codex) têm pool de crédito; chamadas com contexto enorme (milhões de tokens) e agents “soltos” queimam o crédito rápido e o banner de upgrade aparece cedo.

O Dev Command Center ataca isso **por arquitetura**, não por “otimização de prompt”:

- **Menos tokens por execução** — Plano usa só lista de arquivos (até 50 paths), não conteúdo. Código é chamada separada, escopada pelo plano. Resposta de código pede **diff** para modify (não arquivo inteiro quando não precisa).
- **Revisão antes de rodar** — Você vê o plano e só então gera código; pode descartar ou ajustar antes de gastar.
- **Multi-provider** — Você escolhe provider (e modelo) por missão; pode usar provedor/modelo mais barato quando fizer sentido.
- **Visibilidade** — Tokens e duração por missão na aba Logs; aviso de consumo nos diálogos de “Regenerar plano” e “Regenerar código”.

Frase que resume: **Plataformas cloud otimizam para rodar mais. O Dev Command Center otimiza para decidir antes de rodar.** — Menos tokens desperdiçados, mais decisões conscientes.

---

## Como usar (boas práticas)

1. **Quebre em missões pequenas** — Várias missões com alterações focadas; execute uma após a outra no mesmo projeto.
2. **Sempre revise o plano** — Antes de clicar em “Gerar código”, confira passos e arquivos; use “Regenerar plano” com feedback se precisar ajustar.
3. **Uma missão por vez no mesmo repo** — Não tente rodar duas missões em paralelo no mesmo projeto; o sistema impede e o `git apply` exige base única.
4. **Use a aba Logs** — Veja tokens e duração por missão para ter noção de custo.
5. **Escolha o provider por missão** — Em “Gerar código com:” você pode trocar o provedor; use isso para arbitragem de custo quando fizer sentido.

---

## Referências no código

- Fluxo plano → código: `electron/services/ai-orchestrator.ts` (exige `mission.plan` em `generateCode`).
- Uma missão por projeto: checagem `findInProgress` em `generatePlan` e `generateCode`.
- Apply via git: `electron/services/git-service.ts` (`applyPatch`, `applyChanges`).
- Contexto do plano (só lista de arquivos): `electron/services/adapters/base.ts` (`buildPlanPrompt` usa `projectContext.files`).
- Instrução diff-first para código: `buildCodePrompt` no mesmo arquivo.
