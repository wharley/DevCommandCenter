/**
 * Escrita robusta para PTY: fragmenta por bytes UTF-8 (paste/colagens grandes)
 * e evita cortar code points no meio.
 */
export const PTY_INPUT_CHUNK_BYTES = 8192;

export function splitUtf8StringIntoChunks(s: string, maxBytes: number): string[] {
  if (!s) return [];
  const bytes = new TextEncoder().encode(s);
  if (bytes.length <= maxBytes) return [s];

  const out: string[] = [];
  let offset = 0;
  const decoder = new TextDecoder();

  while (offset < bytes.length) {
    let end = Math.min(offset + maxBytes, bytes.length);
    while (end > offset && (bytes[end] & 0xc0) === 0x80) {
      end -= 1;
    }
    if (end === offset) {
      end = offset + 1;
    }
    out.push(decoder.decode(bytes.subarray(offset, end)));
    offset = end;
  }

  return out;
}

export type PtyWriteFn = (ptyId: string, data: string) => Promise<{ ok: boolean }>;

/**
 * Envia `data` ao PTY em chunks sequenciais; falha se `write` rejeitar ou retornar ok: false.
 */
export async function writePtyInputInChunks(
  write: PtyWriteFn,
  ptyId: string,
  data: string,
  maxChunkBytes: number = PTY_INPUT_CHUNK_BYTES,
): Promise<void> {
  const chunks = splitUtf8StringIntoChunks(data, maxChunkBytes);
  for (const chunk of chunks) {
    const r = await write(ptyId, chunk);
    if (!r?.ok) {
      throw new Error(
        "Sessão PTY não encontrada ou já encerrada; não foi possível enviar entrada.",
      );
    }
  }
}
