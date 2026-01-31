/**
 * Credential Storage - Criptografia de API keys via safeStorage
 * Usado para persistir providers.api_key de forma segura no SQLite.
 */

import { safeStorage } from "electron";

let _encryptionAvailable: boolean | null = null;

/**
 * Verifica se safeStorage está disponível neste ambiente.
 * Pode retornar false em Linux sem keyring ou sessões remotas.
 */
export function isEncryptionAvailable(): boolean {
  if (_encryptionAvailable === null) {
    try {
      _encryptionAvailable = safeStorage.isEncryptionAvailable();
    } catch {
      _encryptionAvailable = false;
    }
  }
  return _encryptionAvailable;
}

/**
 * Encripta uma string (ex.: API key) para armazenamento seguro.
 * Retorna Buffer com o blob criptografado, ou null se criptografia indisponível.
 */
export function encryptApiKey(plainText: string): Buffer | null {
  if (!plainText || !isEncryptionAvailable()) {
    return null;
  }
  try {
    return safeStorage.encryptString(plainText);
  } catch {
    return null;
  }
}

/**
 * Descriptografa um Buffer retornado por encryptApiKey.
 * Retorna a string original ou null em caso de erro.
 */
export function decryptApiKey(encrypted: Buffer): string | null {
  if (!encrypted || !Buffer.isBuffer(encrypted) || encrypted.length === 0) {
    return null;
  }
  if (!isEncryptionAvailable()) {
    return null;
  }
  try {
    return safeStorage.decryptString(encrypted);
  } catch {
    return null;
  }
}
