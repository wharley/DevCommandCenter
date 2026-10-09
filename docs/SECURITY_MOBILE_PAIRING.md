# Mobile Pairing — Threat Model & Security Architecture

Status: implementado · 2026-10-09
Escopo: pareamento e acesso do app nativo (`apps/mobile`) ao desktop DCC
pelo relay, com Noise de ponta a ponta. Código: `crates/dcc-remote` (Rust),
`packages/remote-protocol` (TypeScript), `src-tauri/src/remote_connector.rs`
(conector do desktop), `src-tauri/src/pairing.rs` (aparelhos e auditoria) e
`apps/relay` (o relay).

## 1. Objetivo

Permitir que o celular do mesmo usuário acompanhe e responda sessões do
desktop de qualquer rede, sem abrir porta no computador, sem VPN e sem conta,
e sem que o relay no meio consiga ler ou alterar o conteúdo.

Não objetivos:

- Compartilhamento de acesso entre usuários distintos (cada aparelho pareia
  com um desktop específico).
- Esconder do relay os metadados de tráfego (ver §5).
- Autenticação entre desktops ou de scripts locais — isso é a API HTTP local
  (§4.7).

## 2. Ativos a proteger

| Ativo | Por que importa |
|-------|-----------------|
| Chave estática Noise do celular | Única prova de que um aparelho foi aprovado |
| Chave estática Noise do desktop | Identifica o desktop no QR e em toda reconexão |
| Segredo do QR (`otp`) | Durante 5 minutos, permite pedir um pareamento |
| Banco de sessões/workspaces | Contém prompts, código e saídas dos agentes |
| Capacidade de agir nas sessões | Mensagens, interrupções e respostas a permissões |

## 3. Modelo de adversário

Considera-se um adversário que **pode**:

- Operar ou comprometer o relay, ou observar a rede entre as pontas.
- Gravar, reordenar, descartar ou injetar frames no relay.
- Conectar-se à sala do desktop como se fosse um celular.
- Fotografar o QR exibido no desktop.
- Roubar a API key ou o bearer de um servidor `dccd-http`.

Não considera:

- Atacante com root no celular ou no desktop.
- Atacante com acesso físico ao desktop desbloqueado.
- Quebra de X25519, ChaCha20-Poly1305 ou SHA-256.

## 4. Defesas

### 4.1 Nada escuta na rede

O desktop abre uma conexão de **saída** (WebSocket) para o relay, padrão
`wss://relay.devcommandcenter.com`, e só enquanto houver celular pareado ou
um QR aberto. O celular entra na mesma sala. A API HTTP do app desktop escuta
só em `127.0.0.1`; as requisições do celular são decifradas pelo conector e
rodam no mesmo roteador dentro do processo, nunca por socket.

O relay pode ser outro: o código está em `apps/relay` e o endereço é
configurável em Configurações → Conexões ou por `DCC_RELAY_URL`. Só `wss://`
é aceito (`ws://` apenas para localhost).

### 4.2 QR de uso único

```
dcc://pair#v=1&relay=<wss url>&room=<sala>&pk=<chave do desktop>&otp=<segredo>
```

- Tudo fica no fragmento (`#`), que nenhum servidor recebe.
- `room` é o id da sala (128 bits aleatórios); `pk` é a chave estática Noise
  do desktop; `otp` são 128 bits aleatórios.
- O `otp` vale **5 minutos** e **uma vez**, e nunca atravessa o relay: o
  celular prova que o conhece pela PSK derivada dele (HKDF-SHA256,
  `dcc-remote/v1`). O handshake identifica a oferta por um `pair_id` também
  derivado, que não revela o segredo.
- As ofertas vivem só na memória do processo do desktop.

### 4.3 Pareamento: `Noise_IKpsk2` + aprovação humana

`Noise_IKpsk2_25519_ChaChaPoly_SHA256`, com prólogo `dcc-remote/v1`. O
celular é sempre o iniciador (conhece a chave do desktop pelo QR):

1. O celular envia a mensagem 1, com sua chave estática e o `pair_id`.
2. O desktop só responde se a oferta existir, não estiver usada nem expirada,
   e mistura a PSK na mensagem 2: só quem tem o mesmo `otp` lê a resposta.
3. O primeiro frame cifrado do celular prova a posse do segredo; só então a
   oferta é consumida.
4. O desktop mostra um pedido de aprovação com o nome do aparelho e o
   **fingerprint** da chave do celular (6 bytes do SHA-256, ex.
   `3F9A-0C12-B7E4`). O celular mostra o mesmo código; a pessoa aprova só se
   os dois baterem. Sem resposta em 2 minutos, o pedido cai.
5. Aprovado, a chave pública Noise do celular é fixada em
   `paired_devices.noise_public_key`.

Quem fotografa o QR ainda precisa ser aprovado no desktop, com um
fingerprint que não é o do celular da pessoa.

### 4.4 Reconexão: `Noise_IK` com chave fixada

`Noise_IK_25519_ChaChaPoly_SHA256`: o desktop lê a chave estática do celular
na mensagem 1 e só responde se ela pertencer a um aparelho aprovado e não
revogado. Qualquer outra chave é recusada (close `4401`) e registrada como
`relay_refused`. Não há senha, token ou sessão para vazar: a credencial é a
chave privada, que fica no Keychain (iOS) / Keystore (Android) do celular. O
build web do app é só para demonstração e testes; nele a chave fica em
`localStorage`.

Depois do handshake, tudo segue cifrado de ponta a ponta (ChaCha20-Poly1305),
com fragmentação para mensagens grandes.

### 4.5 O que um aparelho pareado alcança

O conector só repassa caminhos em `/api/v1/` e recusa `/api/v1/auth/`,
`/api/v1/remote/` e `/api/v1/terminals/`, além de `/rpc` e `/auth/`. Um
celular não pode aprovar outros celulares, gerar QR, rotacionar credenciais
nem abrir terminais. Cabeçalhos de credencial (`Authorization`, `X-API-Key`)
vindos do celular são descartados; a identidade do aparelho entra só como
extensão interna da requisição, que a rede não consegue forjar.

### 4.6 Revogação e limite de aparelhos

- **Revogar** (Configurações → Conexões → Dispositivos pareados, ou
  `DELETE /auth/devices/:id` num `dccd-http`) marca `revoked_at` e derruba na
  hora a conexão aberta daquele aparelho (close `4499`). Conexões abertas
  também revalidam o aparelho a cada 15 s, o que cobre uma revogação feita
  por outro processo no mesmo banco. A chave revogada não reconecta; voltar
  exige um novo pareamento, com QR e aprovação.
- No máximo **10** aparelhos ativos por desktop; acima disso o pareamento é
  recusado (close `4429`) até que um seja revogado.
- Aparelhos do pareamento antigo pela rede local (sem chave Noise) são
  revogados automaticamente na atualização (`legacy_device_retired`) e não
  ocupam vagas.

### 4.7 API HTTP local

- O app desktop serve a API só em `127.0.0.1`, protegida por `X-API-Key`.
- A API key é aleatória por instalação (256 bits), gravada em
  `~/.dcc/http-config.json` com permissão `0600`. O servidor se recusa a
  subir com a antiga chave padrão pública.
- O `dccd-http` (servidor sem interface) também escuta em loopback por
  padrão. Só um `DCC_HTTP_HOST` explícito fora de loopback o expõe na rede;
  aí a API key deixa de bastar e é exigido o bearer do operador
  (`DCC_HTTP_BEARER_TOKEN`, com expiração e rotação em
  `POST /api/v1/auth/bearer/rotate`). O celular não usa esse caminho.

### 4.8 Relay

- O desktop prova ao relay que é dono da sala com um segredo próprio
  (`x-dcc-room-secret`); o relay guarda só o SHA-256 dele. O celular não tem
  credencial no relay: quem o autentica é o desktop, via Noise.
- O relay aceita só frames binários de até 65 537 bytes, limita cada celular
  a 100 frames/s (rajada de 300) e a 8 celulares por sala, e não registra
  nada sobre o conteúdo.

## 5. O que o relay vê

| Vê | Não vê |
|----|--------|
| Id da sala | Requisições, respostas e eventos |
| Endereços IP do desktop e do celular | Prompts, código, saídas dos agentes |
| Horários e tamanhos dos frames | O segredo do QR e as chaves privadas |

Um relay malicioso pode derrubar ou atrasar a conexão (negação de serviço),
mas não ler, alterar sem ser detectado nem se passar por uma das pontas.

## 6. Limites conhecidos

1. **Metadados visíveis ao relay** (§5). Quem precisa escondê-los pode
   hospedar o próprio relay (`apps/relay`).
2. **Chave do desktop em disco**: a chave privada estática do desktop e o
   segredo da sala ficam no banco local (tabela `remote_relay`). Quem lê esse
   arquivo pode se passar pelo desktop.
3. **Sem expiração automática**: um aparelho fica pareado até ser revogado.
4. **Aprovação depende da pessoa**: aprovar sem comparar o fingerprint anula
   a proteção contra QR fotografado.

## 7. Auditoria

Trilha persistente em `pair_audit_log`, visível em Configurações →
Conexões → log de auditoria:

```
event                  | quando dispara
-----------------------|-----------------------------------------------
relay_offer            | um QR de pareamento foi gerado
relay_offer_invalid    | pedido com QR desconhecido, usado ou expirado
relay_pair_denied      | a pessoa recusou o pareamento
pair                   | aparelho aprovado e fixado
relay_refused          | chave não aprovada (ou revogada) tentou reconectar
limit_reached          | pareamento recusado pelo limite de aparelhos
revoke                 | aparelho revogado
legacy_device_retired  | aparelho do pareamento antigo pela rede local revogado
```

A trilha é local — não é enviada para nenhum serviço externo.

## 8. Resumo rápido

```
┌────────────────────────────────────────────────────────────────┐
│ 1. Transporte                                                  │
│    conexão de saída ao relay · API local só em 127.0.0.1       │
├────────────────────────────────────────────────────────────────┤
│ 2. Pareamento                                                  │
│    QR com otp de 128 bits · 5 min · uso único ·                │
│    Noise_IKpsk2 · aprovação no desktop com fingerprint         │
├────────────────────────────────────────────────────────────────┤
│ 3. Reconexão                                                   │
│    Noise_IK · só chaves fixadas e não revogadas                │
├────────────────────────────────────────────────────────────────┤
│ 4. Cifragem                                                    │
│    ChaCha20-Poly1305 de ponta a ponta · relay só vê bytes      │
├────────────────────────────────────────────────────────────────┤
│ 5. Revogação                                                   │
│    imediata · derruba a conexão aberta · até 10 aparelhos      │
├────────────────────────────────────────────────────────────────┤
│ 6. Auditoria                                                   │
│    pair_audit_log local                                        │
└────────────────────────────────────────────────────────────────┘
```
