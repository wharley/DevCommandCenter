# DCC pelo celular

O companion em `/m/` permite iniciar tarefas, acompanhar conversas, responder a decisões do agente e revisar alterações. O computador continua executando os agentes e precisa estar acordado, com `dccd-http` disponível.

## Funcionalidades

- **Nova tarefa:** descrição, título opcional, agente/modelo do catálogo configurado e opção de planejar antes de implementar. Escolha um workspace pronto ou crie uma branch/worktree a partir de um repositório cadastrado. Alterações não commitadas ficam no checkout original.
- **Criação recuperável:** a solicitação fica identificada no celular e no SQLite. Se a resposta se perder, “Verificar criação” recupera o mesmo pedido. Um reinício do serviço registra falha e preserva os IDs já criados para inspeção; não repete automaticamente a criação.
- **Reconexão:** heartbeat, tentativas com intervalo crescente, recuperação ao voltar ao aplicativo/trocar de rede e atualização pelo histórico persistido. Eventos do processo desktop também aparecem por consultas periódicas.
- **Rascunhos e leitura offline:** mensagens em edição, respostas às perguntas e snapshots de conversas visitadas ficam no armazenamento local, separados por computador/dispositivo. Snapshots têm limite de tamanho e dependem do espaço disponível no navegador. Desconectar remove os dados locais do companion.
- **Revisão:** toque em um arquivo para abrir o patch, incluindo arquivos novos e removidos. A resposta tem limite de 512 KB e indica truncamento.
- **PWA:** ícone, modo independente, atalhos e shell offline. Atualizações são aplicadas pelo botão nas configurações, preservando rascunhos. O service worker armazena apenas assets públicos; APIs e respostas de autenticação não entram no Cache Storage.

## Instalar com HTTPS e Tailscale

O acesso HTTP existente continua disponível no navegador. Instalação com suporte offline requer contexto seguro. Para usar o endereço privado HTTPS do Tailscale, configure **Tailscale Serve** no computador para encaminhar a origem inteira ao DCC (incluindo `/m`, `/api` e `/auth`). Exemplo para a porta padrão, após conferir a configuração Serve existente:

```sh
tailscale serve status
tailscale serve --bg http://127.0.0.1:9876
```

Use a URL HTTPS exibida pelo comando. Serve disponibiliza o serviço dentro da tailnet e pode solicitar habilitação de HTTPS. Veja a [documentação oficial de Serve](https://tailscale.com/docs/reference/tailscale-cli/serve). Esta mudança no código não executa nem altera a configuração do Tailscale.

1. No DCC desktop, abra **Settings → Conexões → Parear novo dispositivo**.
2. Preencha **Endereço HTTPS para instalar no celular** com a origem configurada. O QR e o link passam a usar essa origem. Deixe vazio para usar LAN/Tailscale HTTP como antes.
3. Abra o QR no celular, conectado à mesma tailnet, e confirme o PIN.
4. No iPhone, use **Compartilhar → Adicionar à Tela de Início**. No Android, use a opção de instalação do navegador ou o botão nas configurações do DCC.
5. Se o aplicativo instalado abrir sem o pareamento do navegador, gere outro link HTTPS e cole-o na tela inicial do aplicativo. Confirme o novo PIN.

Mudar entre HTTP e HTTPS muda a origem do armazenamento: será necessário parear nessa origem. Não reutilize links expirados. O pareamento mantém as regras existentes de validade/revogação do DCC.

## Limites atuais

- Web Push foi removido na 0.1.66. Não há avisos com o companion fechado; acompanhe tarefas e decisões com a página aberta. Notificações nativas do desktop continuam disponíveis.

- PWA e reconexão não mantêm o Mac acordado nem executam os agentes sem o computador. O cache permite consultar snapshots e escrever rascunhos; enviar ações exige acesso ao host.
- A arquitetura mantém runtimes de agentes separados por processo. Perguntas/aprovações de tarefas iniciadas pelo companion são encaminhadas ao runtime HTTP. Para um agente em execução no processo desktop, ainda não há encaminhamento dessas respostas entre processos. O histórico pode ser consultado, mas o controle completo dessa execução requer uma ponte de comandos ou um runtime único.
- A criação tem idempotência persistida. Envio de mensagens e respostas usam os endpoints existentes: após resultado de rede ambíguo, confira o histórico antes de repetir. Não há fila automática de ações offline.

## Desenvolvimento e validação

Use Node 22 compatível com o Vite do workspace:

```sh
yarn workspace @dcc/mobile-web typecheck
yarn workspace @dcc/mobile-web test
yarn workspace @dcc/mobile-web build
cargo check -p dev-command-center-tauri --lib --bin dccd-http
cargo test -p dev-command-center-tauri --lib
```

Em checkout sem os sidecars empacotados, as verificações Rust podem usar `TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'` apenas no ambiente do comando. Um build distribuível ainda precisa do fluxo normal de empacotamento do projeto.

`apps/mobile-web/scripts/smoke.mjs` testa o build em Chromium com viewport de celular e servidor simulado em `127.0.0.1:5199`, sem acessar dados reais ou iniciar agentes. Requer Playwright e Chromium instalados; aceita `DCC_PLAYWRIGHT_MODULE` (caminho do módulo), `DCC_CHROMIUM_EXECUTABLE` e `DCC_SMOKE_ARTIFACTS`. Execute com `node apps/mobile-web/scripts/smoke.mjs`. Cobre pareamento, rascunho após reload, perda da resposta de criação, recuperação sem duplicação, shell/histórico offline, mensagens durante desconexão, perguntas do agente, diff e ausência de APIs no cache público.

Os testes Rust cobrem o journal de criação, recuperação após reinício, diff com Git real, isolamento de caminhos/symlinks e autenticação/pareamento. Os testes do mobile cobrem reconexão, stream silencioso, revogação e repetição da mesma solicitação.

## Endpoints adicionados

Todos usam a autenticação HTTP existente.

| Método e caminho | Uso |
| --- | --- |
| `GET /api/v1/mobile/catalog` | Repositórios, workspaces e catálogo real de agentes/modelos |
| `POST /api/v1/mobile/tasks` | Reserva/cria tarefa com `requestId` UUID e corpo imutável nas repetições |
| `GET /api/v1/mobile/tasks/:request_id` | Resultado `creating`, `started` ou `failed`, com IDs parciais disponíveis |
| `GET /api/v1/mobile/workspaces/:workspace_id/patch?path=…` | Patch limitado a arquivo dentro do workspace |
