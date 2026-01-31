/**
 * Provider Service - Encrypt/decrypt e migration de API keys
 * Camada entre IPC/repo e credential-storage.
 */

import type { Provider, CreateProviderDTO, UpdateProviderDTO } from "../../lib/database/types";
import { getDatabase } from "../../lib/database/connection";
import {
  isEncryptionAvailable,
  encryptApiKey,
  decryptApiKey,
} from "./credential-storage";
import db from "../../lib/database";

/** Retorna provider com apiKey preenchida (decriptada quando necessário). */
function hydrateProvider(provider: Provider): Provider {
  if (provider.apiKey) return provider;
  if (provider.apiKeyEncrypted) {
    const decrypted = decryptApiKey(provider.apiKeyEncrypted);
    if (decrypted) {
      const { apiKeyEncrypted: _, ...rest } = provider;
      return { ...rest, apiKey: decrypted };
    }
  }
  return provider;
}

/** Remove apiKey antes de enviar ao renderer; mantém hasApiKey. */
export function sanitizeForRenderer(provider: Provider): Provider {
  const { apiKey, apiKeyEncrypted, ...rest } = provider;
  return { ...rest, hasApiKey: !!(apiKey || apiKeyEncrypted) };
}

/** Prepara dados para create: encripta apiKey quando possível. */
function prepareCreateData(data: CreateProviderDTO): CreateProviderDTO & { apiKeyEncrypted?: Buffer } {
  const apiKey = data.apiKey?.trim();
  if (!apiKey) return data;

  if (isEncryptionAvailable()) {
    const encrypted = encryptApiKey(apiKey);
    if (encrypted) {
      return { ...data, apiKey: undefined, apiKeyEncrypted: encrypted };
    }
  }
  return data;
}

/** Prepara dados para update: encripta apiKey quando presente. */
function prepareUpdateData(data: UpdateProviderDTO): UpdateProviderDTO & { apiKeyEncrypted?: Buffer } {
  if (data.apiKey === undefined) return data;

  const apiKey = data.apiKey?.trim();
  if (!apiKey) {
    return { ...data, apiKey: null, apiKeyEncrypted: undefined };
  }

  if (isEncryptionAvailable()) {
    const encrypted = encryptApiKey(apiKey);
    if (encrypted) {
      return { ...data, apiKey: undefined, apiKeyEncrypted: encrypted };
    }
  }
  return data;
}

export const providerService = {
  isEncryptionAvailable,

  findAll(): Provider[] {
    return db.providers.findAll().map(hydrateProvider);
  },

  findById(id: string): Provider | null {
    const p = db.providers.findById(id);
    return p ? hydrateProvider(p) : null;
  },

  findByType(type: Provider["type"]): Provider[] {
    return db.providers.findByType(type).map(hydrateProvider);
  },

  findActive(): Provider[] {
    return db.providers.findActive().map(hydrateProvider);
  },

  create(data: CreateProviderDTO): Provider {
    const prepared = prepareCreateData(data);
    return hydrateProvider(db.providers.create(prepared));
  },

  update(id: string, data: UpdateProviderDTO): Provider | null {
    const prepared = prepareUpdateData(data);
    const p = db.providers.update(id, prepared);
    return p ? hydrateProvider(p) : null;
  },

  delete: db.providers.delete.bind(db.providers),
  count: db.providers.count.bind(db.providers),
  existsByName: db.providers.existsByName.bind(db.providers),

  setActive(id: string, isActive: boolean): Provider | null {
    const p = db.providers.update(id, { isActive });
    return p ? hydrateProvider(p) : null;
  },

  /** Migra api_key (texto plano) para api_key_encrypted. Executar no startup. */
  migrateLegacyApiKeys(): number {
    const database = getDatabase();
    if (!isEncryptionAvailable()) return 0;

    const rows = database
      .prepare(
        "SELECT id, api_key FROM providers WHERE api_key IS NOT NULL AND api_key != '' AND (api_key_encrypted IS NULL OR length(api_key_encrypted) = 0)"
      )
      .all() as Array<{ id: string; api_key: string }>;

    let migrated = 0;
    for (const row of rows) {
      const encrypted = encryptApiKey(row.api_key);
      if (encrypted) {
        database
          .prepare("UPDATE providers SET api_key_encrypted = ?, api_key = NULL WHERE id = ?")
          .run(encrypted, row.id);
        migrated++;
      }
    }
    if (migrated > 0) {
      console.log(`[ProviderService] Migrated ${migrated} legacy API key(s) to encrypted storage.`);
    }
    return migrated;
  },

  async testConnection(id: string) {
    const provider = this.findById(id); // hydrated
    if (!provider) return { success: false, error: "Provider not found" };
    if (provider.type === "openai" || provider.type === "anthropic") {
      if (!provider.apiKey) return { success: false, error: "API key not configured" };
      return { success: true };
    }
    if (provider.type === "cursor" || provider.type === "claude-code" || provider.type === "codex") {
      if (!provider.cliPath) return { success: false, error: "CLI path not configured" };
      return { success: true };
    }
    return { success: true };
  },
};
