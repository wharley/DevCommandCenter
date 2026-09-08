# Auditoria de tamanho do pacote DCC — 2026-09-08

## Decisão posterior à auditoria

O responsável pelo produto definiu **CLI instalado e autenticado pelo usuário**,
seguindo T3 Code e Synara. DCC mantém o Agent SDK, descobre a instalação existente
e orienta quando estiver ausente, quebrada ou sem autenticação. Não haverá download,
instalação, atualização ou fallback de Claude gerenciado pelo DCC. Essa decisão
substitui a recomendação inicial de runtime sob demanda abaixo. As medições e a
análise original permanecem como registro da base anterior à implementação.

## Parecer

É viável reduzir o pacote mantendo o Anthropic Agent SDK. Há duas frentes distintas: otimização do que já distribuímos e mudança na distribuição do executável Claude. A primeira oferece um ganho moderado mensurável. A segunda oferece o maior ganho no instalador, mas exige trabalho de produto e engenharia para preservar a experiência de instalação, compatibilidade e recuperação.

Recomendação: começar pelos símbolos dos executáveis próprios, preservando os artefatos de diagnóstico. Em uma etapa separada, avaliar um runtime Claude com versão controlada, instalado sob demanda e reutilizado entre atualizações. Não substituir automaticamente a versão empacotada por qualquer `claude` encontrado no PATH.

Esta auditoria não alterou código, dependências, configurações de build, aplicativo instalado ou release. Os experimentos usaram cópias temporárias. Os resultados não constituem aprovação de compatibilidade funcional de um novo pacote.

## Base e método

- Inventário e experimento de símbolos: aplicativo instalado **0.1.65, macOS ARM64**, aproximadamente 330 MiB. Esse é o aplicativo com a falha de OpenSSL relatada; não é uma medição do artefato final 0.1.66.
- Inspeção do empacotamento e experimentos do auxiliar: código **b45f4a4ba863b9261f42e054f3873e49bea658f6**, versão **0.1.66**, Bun **1.3.13**. A remoção de Web Push já pertence a esse código e não foi refeita nesta auditoria.
- Unidade: MiB = 1.048.576 bytes. Tamanho dos arquivos, tamanho comprimido do download e consumo de RAM são métricas diferentes.
- Arquivos e medições brutas desta sessão: `/private/tmp/dcc-size-audit`. São temporários; os resultados relevantes estão registrados abaixo.

| Componente instalado | MiB |
| --- | ---: |
| Claude Code, `Resources/vendor/claude-code/claude` | 189,81 |
| Auxiliar próprio, `dcc-claude-sidecar` | 61,58 |
| Executável principal | 52,04 |
| `dcc` | 2,48 |
| `dccd` | 6,72 |
| `dccd-http` | 16,21 |
| Demais recursos | Cerca de 1 |

## SDK e executável Claude são componentes diferentes

O DCC fixa `@anthropic-ai/claude-agent-sdk` em **0.2.126**, `@anthropic-ai/claude-code` em **2.1.258** e `mcp-remote` em **0.1.38**, em [package.json](../package.json). O SDK é usado pelo auxiliar JavaScript; o executável Claude é copiado separadamente por [stage-vendor.mjs da versão auditada](https://github.com/wharley/DevCommandCenter/blob/b45f4a4ba863b9261f42e054f3873e49bea658f6/sidecar/scripts/stage-vendor.mjs).

O arquivo de entrada JavaScript `sdk.mjs` do SDK tem aproximadamente 672 KiB nesta instalação de desenvolvimento. Isso não representa o tamanho de todas as suas dependências. O auxiliar compila esse código, nossas integrações e um runtime Bun em um executável independente.

Existe também um pacote nativo opcional do SDK no `node_modules` de desenvolvimento, com outro executável de aproximadamente 206 MiB. Não encontrei essa segunda cópia no `.app`: o empacotamento examinado copia o executável explícito de `@anthropic-ai/claude-code`. Excluir dependências opcionais do ambiente de desenvolvimento, portanto, não economizaria automaticamente outros 206 MiB na distribuição atual.

O DCC já passa `pathToClaudeCodeExecutable` ao SDK em [index.mjs](../sidecar/src/index.mjs). Assim, pode continuar usando o SDK com um executável obtido de outra forma. Entretanto, a descoberta atual em [claude_sdk_sidecar.rs](../crates/dcc-providers/src/claude_sdk_sidecar.rs) espera os caminhos empacotados/de desenvolvimento; simplesmente remover o recurso de 190 MiB quebraria essa expectativa.

## O que T3 Code e Synara fazem

Foram examinadas revisões específicas dos repositórios, não somente descrições comerciais:

- **T3 Code**, revisão `061543e9e5b54ec0048725c37d52fef2962df173`: usa o Agent SDK e fornece um executável Claude externo via `pathToClaudeCodeExecutable`. O empacotamento exclui explicitamente os pacotes nativos opcionais `@anthropic-ai/claude-agent-sdk-*`, preservando o JavaScript do SDK. O próprio comentário explica que usa o Claude instalado pelo usuário. Fontes: [adaptador](https://github.com/pingdotgg/t3code/blob/061543e9e5b54ec0048725c37d52fef2962df173/apps/server/src/provider/Layers/ClaudeAdapter.ts#L4673), [exclusão no pacote](https://github.com/pingdotgg/t3code/blob/061543e9e5b54ec0048725c37d52fef2962df173/scripts/build-desktop-artifact.ts#L955), [resolução de executáveis, incluindo Windows](https://github.com/pingdotgg/t3code/blob/061543e9e5b54ec0048725c37d52fef2962df173/apps/server/src/provider/Drivers/ClaudeExecutable.ts).
- **Synara**, revisão `d3f6b1b67a946b5414cec02e6b0a616b73834d83`: também carrega o Agent SDK e fornece um caminho configurado ou `claude` como executável. Portanto, a integração permite o CLI externo mantendo o SDK. Não foi auditado o conteúdo de um instalador publicado do Synara; não há base nesta análise para afirmar que exclui todos os pacotes nativos opcionais ou que seu aplicativo completo ocupa menos espaço. Fontes: [carregamento do SDK](https://github.com/Emanuele-web04/synara/blob/d3f6b1b67a946b5414cec02e6b0a616b73834d83/apps/server/src/provider/claudeAgentSdk.ts), [adaptador](https://github.com/Emanuele-web04/synara/blob/d3f6b1b67a946b5414cec02e6b0a616b73834d83/apps/server/src/provider/Layers/ClaudeAdapter.ts#L5319).

Ambos usam Electron. Essa arquitetura já oferece runtime JavaScript; não é uma comparação direta com um auxiliar Bun separado no Tauri. Migrar o DCC para Electron somente para eliminar esse auxiliar não tem economia demonstrada e envolve uma mudança muito maior.

## Experimentos de redução

### Símbolos dos executáveis Rust

Aplicado `/usr/bin/strip -S -x -o COPIA ORIGINAL` somente a cópias dos quatro executáveis próprios da versão instalada:

| Executável | Original MiB | Cópia MiB | Economia MiB |
| --- | ---: | ---: | ---: |
| Principal | 52,04 | 40,53 | 11,50 |
| `dcc` | 2,48 | 2,29 | 0,19 |
| `dccd` | 6,72 | 5,66 | 1,06 |
| `dccd-http` | 16,21 | 12,73 | 3,49 |
| **Total** | **77,45** | **61,21** | **16,24** |

O ganho corresponde a aproximadamente **4,9% do aplicativo de 330 MiB**. Comprimir individualmente os originais e as cópias com gzip nível 9 produziu uma economia agregada de **2,02 MiB**. Isso é uma aproximação do impacto no download; não é uma medição do arquivo real de atualização do Tauri.

Remover somente informações de debug com `strip -S` não trouxe economia útil. Logo, adicionar apenas `strip = "debuginfo"` não tem ganho demonstrado nesta base. A configuração de Cargo e seu resultado devem ser medidos, sem presumir equivalência exata entre opções do compilador e o comando experimental. Referência: [perfis do Cargo](https://doc.rust-lang.org/cargo/reference/profiles.html).

As cópias modificadas não foram tratadas como aplicativos assinados utilizáveis, nem executadas para validar funcionalidade. A implementação deve ocorrer **antes da assinatura**, preservar executáveis originais e símbolos/dSYM identificados por versão e UUID, e provar que os relatórios de falha continuam simbolicáveis. Depois são necessários assinatura, notarização, inspeção de dependências e teste do pacote final.

### Minificação do auxiliar Bun

Builds pareados do mesmo código e runtime:

| Variante | Bytes | MiB aproximados | Bytes gzip |
| --- | ---: | ---: | ---: |
| Programa vazio compilado com Bun | 63.072.928 | 60,15 | 23.060.107 |
| Auxiliar atual, sem minificação | 65.778.082 | 62,73 | 23.694.395 |
| `--minify --keep-names` | 64.919.458 | 61,91 | 23.610.323 |
| Mesmas opções e `--sourcemap` | 66.124.834 | 63,06 | 24.481.275 |

O ganho da minificação foi **0,82 MiB em disco e cerca de 82 KiB comprimidos**. O runtime domina o tamanho: o executável vazio já tem aproximadamente 60 MiB. Com source map embutido, o resultado ficou maior que o baseline, embora melhore o diagnóstico. Referência sobre o empacotamento: [executáveis Bun](https://bun.sh/docs/bundler/executables).

As três variantes passaram por `--version` e configuração MCP vazia, com um caminho Claude inexistente deliberado, sem invocação do provedor. Esses testes verificam apenas inicialização e protocolo básico; não validam autenticação, conversas, permissões ou MCP real. A diferença entre o auxiliar instalado de 61,58 MiB e o experimental de 62,73 MiB não deve ser atribuída à minificação: somente os builds pareados permitem medir esse efeito.

Reprodução a partir da raiz, preferencialmente em checkout temporário para conter arquivos auxiliares do Bun:

```sh
bun build --compile sidecar/src/index.mjs --outfile /private/tmp/dcc-size-audit/sidecar-baseline
bun build --compile --minify --keep-names sidecar/src/index.mjs --outfile /private/tmp/dcc-size-audit/sidecar-minified
bun build --compile --minify --keep-names --sourcemap sidecar/src/index.mjs --outfile /private/tmp/dcc-size-audit/sidecar-minified-sourcemap
```

## A mudança com maior potencial

Retirar o Claude embutido reduziria aritmeticamente o `.app` de aproximadamente **330 para 140 MiB**, usando o inventário da 0.1.65. Isso **não elimina os 190 MiB do disco** de quem precisa instalar Claude separadamente. Os benefícios seriam evitar essa transferência em cada atualização do DCC, reutilizar uma instalação compatível existente e não instalar Claude para quem usa somente outros provedores.

| Estratégia | Benefício | Principal custo |
| --- | --- | --- |
| Manter Claude no pacote | Versão previsível, executável disponível sem download inicial adicional | Pacote maior em cada release |
| Usar qualquer CLI do PATH | Grande redução imediata do pacote | Versão variável, ausência do CLI e atualizações externas podem quebrar a integração |
| Runtime controlado, instalado sob demanda | Pacote menor e versão previsível; reutilização entre releases | Implementar instalador/cache e tratar primeira utilização sem internet |
| Reutilizar CLI compatível, com runtime controlado como recuperação | Evita duplicação quando possível, preservando recuperação | Maior matriz de resolução e compatibilidade |

Minha preferência é validar primeiro o runtime controlado sob demanda. Reutilização do CLI do usuário pode ser uma opção adicional, condicionada a compatibilidade comprovada. O SDK, nosso auxiliar e as integrações permanecem.

Condições para essa mudança ser confiável:

1. Manter uma matriz explícita de versões SDK/Claude homologadas. Hoje o catálogo anuncia uma combinação fixa em [claude_mcp.rs](../crates/dcc-providers/src/claude_mcp.rs); futuramente precisa informar a versão realmente utilizada. O healthcheck atual verifica a versão do auxiliar, além do estado de autenticação, e não substitui essa verificação do Claude.
2. Resolver um caminho absoluto, considerando aplicativo aberto pelo Finder, instalações nativas/npm, Windows e arquitetura. Fixar o executável de cada sessão; uma atualização não pode trocar seu runtime durante a conversa.
3. Baixar uma versão determinada de origem validada, verificar integridade/autenticidade e instalar atomicamente em diretório versionado. Tratar processos concorrentes, interrupção, ausência de rede, tentativa posterior e retorno à versão anterior.
4. Preservar configuração, credenciais e sessões. Explicar o download na primeira utilização do Claude; se funcionamento inicial offline for requisito, manter uma distribuição completa ou provisionamento prévio do runtime.
5. Validar os caminhos oficiais de distribuição e instalação da Anthropic antes de implementar o downloader. Esta auditoria comprova a separação técnica, não entrega um instalador externo homologado.

## Sequência de engenharia recomendada

**Primeira etapa:** símbolos dos binários próprios e medição dos artefatos finais por plataforma. Registrar tamanho instalado, download, tamanhos por componente e diferença para a release anterior. Preservar e testar diagnóstico de crashes. A minificação pode ser avaliada junto, mas seu ganho é pequeno e não justifica perder diagnósticos.

**Segunda etapa:** prova de conceito isolada do runtime Claude controlado. Validar instalação nova, migração, CLI ausente, versões incompatíveis, download interrompido, modo offline e recuperação. Só então decidir a mudança de distribuição.

**Validação funcional obrigatória antes de substituir o pacote atual:** autenticação, conversa com streaming, retomada de sessão, cancelamento, permissões, AskUserQuestion, modo de planejamento, subagentes, MCP local, MCP remoto/OAuth, atualização e reinício. Usar o pacote assinado extraído do artefato real. No macOS, testar fora do checkout e sem Node/Bun de desenvolvimento: a descoberta atual pode preferir o script do repositório e mascarar problemas do auxiliar distribuído.

LTO, `codegen-units` e otimizações de tamanho merecem um experimento posterior com medições de build, inicialização e desempenho. Não há economia medida para essas opções nesta auditoria. Não adotar `panic = "abort"` apenas para reduzir tamanho, pois altera o comportamento de falha. Também não recomendo fundir os processos próprios ou modificar o binário de terceiros sem necessidade demonstrada: essas mudanças envolvem arquitetura, manutenção e diagnóstico, além do tamanho.

**Decisão proposta:** perseguir primeiro o ganho medido de aproximadamente 16 MiB, com validação de distribuição e diagnóstico; tratar os aproximadamente 190 MiB como um projeto separado de distribuição do runtime, mantendo o Agent SDK e o comportamento do provedor.
