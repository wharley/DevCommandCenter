# Mobile Pairing — Threat Model & Security Architecture

Status: implementado · 2026-05-16
Escopo: pareamento celular ↔ desktop DCC via QR code + PIN com chaves
ECDSA P-256 não-extraíveis.

## 1. Objetivo

Permitir que um celular do mesmo usuário autentique requisições HTTP ao
backend `dccd-http` que roda no desktop, com segurança equivalente a SSH
mas sem exigir uma conexão SSH no celular.

Não objetivos:

- Compartilhamento de acesso entre usuários distintos (cada device parea
  com um desktop específico).
- Autenticação entre desktops (já coberto por bearer token / API key /
  túnel SSH).
- Confidencialidade automática do conteúdo das requisições — isso é
  responsabilidade da camada de transporte (LAN HTTPS via Tailscale, etc).

## 2. Ativos a proteger

| Ativo | Por que importa |
|-------|-----------------|
| Chave privada do device | Único material que prova posse do device |
| Banco de sessions/workspaces | Contém prompts, código, chaves de API de terceiros |
| Capacidade de executar comandos | Endpoints como `POST /api/v1/terminals/spawn` abrem PTYs reais |
| PIN exibido no pareamento | Permite ao atacante completar um pareamento se vazado durante a janela |

## 3. Modelo de adversário

Considera-se um adversário que **pode**:

- Sniffar tráfego HTTP na mesma rede WiFi (café, hotel, escola).
- Realizar ARP spoofing / MITM em redes locais.
- Fotografar/screenshot o QR exibido no desktop antes do pareamento ser
  concluído.
- Tentar acessar `/auth/pair` direto da rede (sem o QR).
- Postar variations bruteforce de PIN em volume razoável.
- Roubar uma única bearer-token ou X-API-Key vazada em logs.
- Ler o conteúdo de cookies / localStorage do browser do celular.

Não considera:

- Atacante com root no dispositivo do usuário (impossível defender via
  software).
- Atacante com acesso físico ao desktop pareado (idem).
- Quebra criptográfica de ECDSA P-256 ou SHA-256.
- Compromisso da máquina do usuário rodando o `dccd` daemon.

## 4. Defesas em profundidade

### 4.1 Pareamento efêmero

- O QR code carrega um `nonce` de 256 bits (URL-safe base64) e o backend
  exibe um PIN de 6 dígitos numéricos.
- O par `(nonce, pin)` vive **60 segundos** e é **single-use** (linha
  `consumed_at` na tabela `pairing_nonces`).
- O hash do PIN é `SHA-256("dcc-pair-pin\0" || nonce || "\0" || pin)` —
  domain-separated com o nonce, então rainbow tables pré-computadas não
  ajudam de uma sessão para outra.
- Tentativas de PIN incorreto são contadas em `failed_attempts`. Após
  **5 erros**, o nonce é travado (`locked_at`) e nem o PIN correto
  consegue resgatá-lo.

Resultado: mesmo que o atacante fotografe o QR, o pareamento só funciona
dentro da janela com o PIN correto. A probabilidade teórica de adivinhar
o PIN é no máximo `5 / 10⁶ ≈ 0,0005 %`.

### 4.2 Chaves device-bound não-extraíveis

- O cliente mobile gera a keypair via `crypto.subtle.generateKey` com
  `extractable: false` — nem mesmo o JavaScript que rodou a geração
  consegue ler os bytes da chave privada.
- A chave é persistida no IndexedDB via structured-clone, mantendo o
  handle opaco entre reloads.
- Um atacante que comprometa a página/JS pode usar a chave para assinar
  requisições enquanto a sessão estiver aberta, mas não pode exfiltrar
  a chave para reutilização posterior.

### 4.3 Assinatura por request (replay-proof)

Cada chamada autenticada inclui três headers:

```
X-Device-Id: <uuid>
X-Timestamp: <RFC 3339>
X-Signature: <base64(ECDSA-DER)>
```

A assinatura cobre o **payload canônico** abaixo, em UTF-8:

```
METHOD\n
PATH\n
TIMESTAMP\n
SHA256_HEX(body)
```

- Mudar o método, o path, o timestamp ou um único byte do body invalida
  a assinatura.
- Timestamp fora de uma janela de **60 segundos** é rejeitado (`AuthError::SignedRequestStaleTimestamp`).
- Bodies maiores que **32 MiB** são rejeitados antes da verificação para
  evitar OOM (`AuthError::SignedRequestBodyTooLarge`).

### 4.4 Revogação granular

- A coluna `revoked_at` é checada em toda verificação. Revogar um device
  tem efeito imediato — a próxima request retorna `403 Forbidden`.
- O usuário vê a lista completa em Settings → Connections → Dispositivos
  pareados, com botão "Revogar" por linha.

### 4.5 Fail-closed no middleware

Quando os três headers `X-Device-Id / X-Timestamp / X-Signature` estão
presentes, o middleware se compromete a verificar a assinatura. Se a
verificação falha, retorna 4xx sem cair para `X-API-Key` ou bearer
token. Isso bloqueia o ataque onde alguém envia signed-headers inválidos
junto com uma API key roubada esperando que o middleware "tente o
próximo método".

### 4.6 Rate limit no `/auth/pair`

In-memory sliding window por IP: **20 tentativas / 60 s**. Combinado com
o lockout de 5 PIN errados por nonce, um atacante consegue no máximo 100
tentativas/min por IP — abaixo do limite onde brute force de PIN se
torna viável dentro da janela de 60 s.

### 4.7 Garbage collection

`create_pairing_nonce` deleta linhas com `created_at` mais antigo que
**10 min**. Mantém a tabela enxuta sem precisar de um worker em
background.

### 4.8 Audit log

Cada `pair`, `revoke`, `pin_failure` e `pin_locked` é registrado em
`pair_audit_log` com `device_id`, IP, user-agent e detalhes. O painel
de Settings exibe os 50 mais recentes com ícones distintos.

## 5. Camada de transporte

A arquitetura de aplicação acima fornece **integridade** (anti-replay,
anti-tamper) e **autenticação** (per-device key) mas **não cifragem do
conteúdo**. Para confidencialidade o usuário escolhe o transporte:

| Opção | Confidencialidade | Setup |
|-------|-------------------|-------|
| LAN HTTP direto | ❌ — qualquer um no mesmo WiFi vê os payloads | nenhum |
| LAN HTTPS self-signed | ✅ mas com prompt no browser | manual |
| Tailscale Serve (free) | ✅ via WireGuard E2E | instalar tailscale |
| Cloudflare Tunnel | ✅ via TLS público | requer domínio |
| SSH reverse tunnel | ✅ | precisa de servidor SSH alcançável |

A solução **default** assume LAN HTTP (cenário típico de uso doméstico).
Para cenários onde o conteúdo precisa ser confidencial em redes
hostis, Tailscale Serve é a recomendação por ser o de menor atrito.

A página da landing `/m/pair` detecta o caso "HTTPS landing → HTTP
backend" (mixed content) e exibe o link LAN direto que o usuário deve
abrir manualmente, em vez de tentar uma chamada que o browser vai
bloquear silenciosamente.

## 6. Limites conhecidos

1. **PIN brute force dentro da janela**: 5 tentativas em 60s significa
   probabilidade ≤ 5/10⁶. Aceitável para uso doméstico mas não para
   ambientes com adversários muito determinados. Mitigação possível:
   PIN de 8 dígitos ou alfanumérico.
2. **Sem confidencialidade default**: se a rede não é confiável, o
   usuário precisa escolher um transporte cifrado. O app não força.
3. **Sem cap de devices por instalação**: implementado em fase
   posterior (ver `Phase 5+`).
4. **Sem expiração automática**: devices ficam pareados indefinidamente
   até serem revogados. Implementado em fase posterior.
5. **Rate limit é in-memory**: reset ao reiniciar o `dccd-http`. Em
   produção real deveria ser persistido.

## 7. Auditoria

Trilha persistente em `pair_audit_log`:

```
event           | quando dispara
----------------|---------------------------------
pair            | device novo registrado com sucesso
revoke          | usuário revogou um device
pin_failure     | PIN incorreto submetido para um nonce
pin_locked      | nonce travado após N PINs incorretos
```

A trilha é local — não é exfiltrada para nenhum serviço externo.

## 8. Resumo rápido

```
┌────────────────────────────────────────────────────────────────┐
│ 1. Transporte (escolha do usuário)                             │
│    LAN HTTP / HTTPS / Tailscale / Cloudflare / SSH tunnel      │
├────────────────────────────────────────────────────────────────┤
│ 2. Pareamento efêmero                                          │
│    nonce 256-bit + PIN 6-dígitos · 60s TTL · single-use ·      │
│    lock após 5 PIN errados · rate-limit 20/min por IP          │
├────────────────────────────────────────────────────────────────┤
│ 3. Autenticação                                                │
│    ECDSA P-256 · chave privada non-extractable no WebCrypto ·  │
│    sig cobre METHOD/PATH/TIMESTAMP/SHA256(body)                │
├────────────────────────────────────────────────────────────────┤
│ 4. Confirmação                                                 │
│    PIN exibido no desktop (out-of-band do QR)                  │
├────────────────────────────────────────────────────────────────┤
│ 5. Revogação                                                   │
│    Soft-delete imediata · per-device · UI em Settings          │
├────────────────────────────────────────────────────────────────┤
│ 6. Audit                                                       │
│    pair_audit_log persistente com IP, UA, detalhes             │
└────────────────────────────────────────────────────────────────┘
```
