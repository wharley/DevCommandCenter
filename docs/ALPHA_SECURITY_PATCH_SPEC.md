# Alpha Security Patch Spec

> Migração de API keys em texto plano para criptografia via `@electron/safeStorage`. Mini-doc para usar como descrição de issue/PR.

---

## Objetivo

Criptografar `providers.api_key` antes de persistir no SQLite. Requisito de Alpha (BYOK exige segurança mínima).

---

## Mudanças no schema

```sql
-- providers: adicionar coluna para blob criptografado
ALTER TABLE providers ADD COLUMN api_key_encrypted BLOB;

-- api_key (TEXT) mantida temporariamente para migração; depois pode ser removida
-- Fluxo: ler api_key (se existe e api_key_encrypted vazio) → encriptar → gravar api_key_encrypted → limpar api_key
```

**Decisão:** Manter `api_key` por compatibilidade durante migração. Após migração completa, `api_key` pode ser deprecated ou removida em versão futura.

---

## Fluxo de migração

1. **Ao iniciar app:** Para cada provider com `api_key` preenchido e `api_key_encrypted` vazio:
   - Se `safeStorage.isEncryptionAvailable()`: encriptar, gravar em `api_key_encrypted`, limpar `api_key`
   - Senão: manter `api_key` em texto plano e exibir aviso ao usuário
2. **Ao criar/atualizar provider:** Sempre tentar encriptar; gravar em `api_key_encrypted`. Nunca gravar em `api_key` (exceto em fallback quando safeStorage indisponível).
3. **Ao ler provider:** Prioridade: `api_key_encrypted` → decriptar; fallback: `api_key` (legado).

---

## Compatibilidade com dados antigos

| Cenário | Ação |
|---------|------|
| DB novo | Criar providers já com `api_key_encrypted` |
| DB existente, `api_key` preenchido | Migrar em background: encriptar → `api_key_encrypted` → limpar `api_key` |
| DB existente, `api_key` vazio | Nada a fazer |

---

## Quando safeStorage não está disponível

`safeStorage.isEncryptionAvailable()` pode retornar `false` em alguns ambientes (ex.: Linux sem keyring, sessão remota).

**Opções:**

| Opção | Prós | Contras |
|-------|------|---------|
| A) Não salvar key, avisar usuário | Seguro | UX ruim; provider inutilizável |
| B) Fallback para texto plano + aviso | Funciona | Key fica em texto plano |
| C) Exigir safeStorage (bloquear) | Consistente | Pode bloquear em Linux |

**Recomendação:** **B** para Alpha — fallback em texto plano com aviso claro na UI ("Criptografia indisponível neste ambiente; API key será armazenada em texto plano. Recomenda-se não usar em máquinas compartilhadas."). Em Beta, avaliar keytar como fallback cross-platform.

---

## Testes mínimos

1. **Roundtrip:** Encriptar string → decriptar → igual à original
2. **Migração:** DB com `api_key` legado → após migração, provider funciona e `api_key` está vazio
3. **safeStorage indisponível:** Mock `isEncryptionAvailable() = false` → fallback funciona, aviso exibido
4. **Provider novo:** Criar provider com key → só `api_key_encrypted` preenchido
5. **Leitura:** Provider com `api_key_encrypted` → adapters recebem key decriptada corretamente

---

## Critérios de aceite

- [ ] Coluna `api_key_encrypted` adicionada ao schema
- [ ] Migração automática de `api_key` → `api_key_encrypted` no startup (quando safeStorage disponível)
- [ ] Repositories (create/update) usam encriptação
- [ ] Repositories (read) decriptam ao retornar provider
- [ ] Aviso na UI quando safeStorage indisponível (fallback em texto plano)
- [ ] Testes unitários cobrindo roundtrip e migração
