# DCC pelo celular

O app nativo do DCC (`apps/mobile`, Expo) mostra os agentes que estão rodando e o que precisa de você, abre a conversa de cada um ao vivo e deixa responder pelo telefone. O computador continua executando os agentes e precisa estar acordado, com o app desktop aberto (ou o `dccd-http`, sem o app).

## O que dá para fazer

- **Agentes:** triagem de todos os projetos — o que precisa de você, o que está rodando e o que terminou há pouco.
- **Workspaces:** os workspaces agrupados por projeto.
- **Conversa:** histórico e eventos ao vivo, envio de mensagem, interromper o turno, responder permissões e perguntas do agente. Com o app desktop aberto, as sessões são as mesmas do desktop: a primeira resposta vale e uma segunda (do desktop ou do celular) recebe 409 com o desfecho. O `dccd-http` sozinho só controla as sessões que ele mesmo iniciou.
- **Demonstração:** sem desktop, "Explorar demonstração" na tela de pareamento abre projetos fictícios dentro do app.

## Como o celular chega ao desktop

O celular nunca abre conexão com o computador. O desktop faz uma conexão de saída (WebSocket) para um relay e o celular entra na mesma sala; o relay só repassa bytes. Celular e desktop fazem um handshake Noise entre si, então o relay não vê requisições, respostas nem eventos — só o id da sala, endereços IP, horários e tamanhos de frame.

- **Relay padrão:** `wss://relay.devcommandcenter.com`, mantido pelo projeto. Dá para usar outro (o código está em `apps/relay`, ver [README do relay](../apps/relay/README.md)) em **Configurações → Conexões → Endereço do relay**, ou com `DCC_RELAY_URL`. Apagar o endereço e salvar desliga o acesso remoto.
- **Conexão sob demanda:** o desktop só fica conectado ao relay enquanto houver celular pareado ou um QR aberto, e reconecta sozinho com backoff.
- **API local:** a API HTTP do app desktop escuta só em `127.0.0.1`. Requisições do celular são decifradas pelo conector e rodam nesse mesmo roteador dentro do processo, sem passar por socket.

Detalhes de segurança em [SECURITY_MOBILE_PAIRING.md](SECURITY_MOBILE_PAIRING.md).

## Parear

1. No desktop, abra **Configurações → Conexões** e clique em **Parear pelo relay**.
2. Escaneie o QR com o app DCC. O QR vale uma vez, por 5 minutos.
3. O desktop pede aprovação mostrando o código (fingerprint) do celular. Aprove só se for o mesmo que o celular mostra.

Depois disso o celular reconecta sozinho, de qualquer rede. Até 10 aparelhos ativos por desktop; para parear outro, revogue um na lista **Dispositivos pareados** da mesma seção. Revogar derruba a conexão do aparelho na hora.

## Servidor sem o app (`dccd-http`)

O `dccd-http` serve a mesma API sem o app desktop e também se conecta ao relay. Por padrão escuta só em loopback com `X-API-Key` (gerada em `~/.dcc/http-config.json`, permissão `0600`). Sem interface gráfica, o operador pareia pela API, com a API key:

| Método e caminho | Uso |
| --- | --- |
| `GET /api/v1/remote/status` | Relay, conexão e fingerprint deste servidor |
| `POST /api/v1/remote/pairing/offers` | Gera o link `dcc://pair#…` (conteúdo do QR) |
| `GET /api/v1/remote/pairing/approvals` | Pedidos de pareamento aguardando aprovação |
| `POST /api/v1/remote/pairing/approvals/:approval_id` | `{"approve": true}` ou `false` |

Revogar um aparelho é feito no app desktop (Configurações → Conexões → Dispositivos pareados); um `dccd-http` no mesmo banco percebe a revogação na revalidação de 15 s e derruba a conexão.

Um `DCC_HTTP_HOST` fora de loopback expõe a API na rede; nesse caso o modo de autenticação passa a remoto e exige o bearer do operador (`DCC_HTTP_BEARER_TOKEN`, rotacionável em `POST /api/v1/auth/bearer/rotate`). O celular não usa esse caminho: ele entra sempre pelo relay.

## Desenvolvimento e validação

Use Node 22 (o teste ponta a ponta pede 22.18+).

```sh
yarn workspace @dcc/mobile typecheck
yarn workspace @dcc/mobile test
yarn workspace @dcc/mobile-core test
yarn test:remote:e2e
cargo test -p dev-command-center-tauri --lib
```

`yarn test:remote:e2e` sobe o relay com `wrangler dev` (sem conta Cloudflare), serve a API como o desktop num banco temporário e conduz um celular simulado em Node, e confere pareamento, reconexão, streams, rotas proibidas e revogação, além de verificar que o relay só viu bytes cifrados. Para testar o app num simulador contra um desktop descartável, veja `apps/mobile/scripts/desktop-fixture.mjs` no [README do app](../apps/mobile/README.md).
