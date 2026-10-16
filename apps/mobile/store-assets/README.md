# Materiais de loja

Tudo aqui é PNG sem canal alpha (conferido com `sips -g hasAlpha`). Os prints
vêm do modo demonstração ("Explorar demonstração"), no iPhone 17 Pro Max
(iOS 26.5), tema escuro, com a barra de status fixada em
`xcrun simctl status_bar … override` (09:41, bateria e sinal cheios).

## Google Play (`google-play/`)

| Arquivo | Tamanho | Onde vai no Play Console |
| --- | --- | --- |
| `icon-512.png` | 512×512, 80 KB | Presença na loja → Detalhes principais → Ícone do app |
| `feature-graphic-1024x500.png` | 1024×500 | Detalhes principais → Recurso gráfico |
| `phone/01…05-*.png` | 1080×1920 (9:16) | Detalhes principais → Capturas de tela do smartphone (na ordem dos números) |

Os prints do Play cortam a barra de status do iOS, para não parecerem de iPhone.

## App Store (`app-store/`)

| Arquivo | Tamanho | Onde vai no App Store Connect |
| --- | --- | --- |
| `iphone-6.9/01…05-*.png` | 1320×2868 | App → versão → Pré-visualizações e capturas de tela → iPhone 6,9" |
| `iphone-6.3/01…05-*.png` | 1206×2622 | App → versão → capturas → "iPhone with Dynamic Island (medium display)" (o slot que o App Store Connect pede hoje) |
| `raw/01…05-*.png` | 1320×2868 | Prints crus sem legenda (também aceitos no iPhone 6,9"); fonte das versões com legenda |

O ícone do App Store não sobe à parte: vem do build. `ios.icon` no `app.json`
aponta para `../assets/icon-ios.png` (1024×1024, sem alpha, o mesmo desenho
sobre o fundo `#05091c` do ícone); o Android segue com `assets/icon.png`.

## Telas e legendas

| # | Tela | Legenda |
| --- | --- | --- |
| 01 | Home "Tarefas" ("Precisa de você" / "Rodando") | Veja quem **precisa de você** |
| 02 | Conversa com pedido de permissão | Aprove com **um toque** |
| 03 | Nova tarefa (bicho do projeto, chips de agente, acesso, worktree) | Delegue de **qualquer lugar** |
| 04 | Folha de projetos com os bichos e o estado de cada um | Seus projetos, **à mão** |
| 05 | Mudanças com o diff de um arquivo aberto | Revise o **diff** no bolso |

Recurso gráfico: wordmark DCC, "Seus agentes de IA, no seu bolso."

## Gerar de novo

`source/build.mjs` monta cada imagem em HTML (robôs e bichos de
`packages/mascots`, na paleta do app) e `source/render.swift` a fotografa com
o WebKit do macOS num PNG opaco do tamanho exato. Para trocar legendas, edite
`SHOTS` no script; para trocar telas, substitua os arquivos em `app-store/raw/`
(mesmos nomes, 1320×2868).

```bash
PATH="$HOME/.nvm/versions/node/v22.21.1/bin:$PATH" node apps/mobile/store-assets/source/build.mjs
```

Os prints crus saem do simulador com alpha; passe-os para RGB antes de salvar
em `raw/` (por exemplo `Image.open(p).convert("RGB").save(p)` com Pillow).
