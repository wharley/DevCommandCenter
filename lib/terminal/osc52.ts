/**
 * OSC 52 — Manipulate Selection Data (clipboard remota).
 * Usado por tmux, neovim, ferramentas SSH para sincronizar cópia com o clipboard do sistema.
 *
 * Formato: ESC ] 52 ; Pt ; Pd BEL|ST
 * - Pt: buffer (ex.: `c` clipboard, `p` primary — no browser mapeamos tudo para clipboard).
 * - Pd: `?` pede o conteúdo ao emulador; caso contrário dados em base64 (UTF-8).
 */

const OSC = "\x1b]";
const BEL = "\x07";

function utf8ToBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]!);
  }
  return btoa(binary);
}

function base64ToUtf8(b64: string): string | null {
  const cleaned = b64.replace(/\s+/g, "");
  if (!cleaned) return "";
  try {
    const binary = atob(cleaned);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
  } catch {
    return null;
  }
}

/** Resposta ao pedido de leitura (`Pd` === `?`): injeta no PTY o mesmo formato OSC 52. */
export function buildOsc52ReadResponse(selection: string, utf8Text: string): string {
  const sel = selection || "c";
  const b64 = utf8ToBase64(utf8Text);
  return `${OSC}52;${sel};${b64}${BEL}`;
}

/** Resposta vazia / erro de leitura (alguns clientes aceitam sequência sem payload). */
export function buildOsc52EmptyResponse(selection: string): string {
  return `${OSC}52;${selection || "c"};${BEL}`;
}

export type Osc52Bridge = {
  writeText: (text: string) => Promise<void>;
  sendToPty: (data: string) => void;
};

function readClipboardOsc52(selection: string, bridge: Osc52Bridge): Promise<boolean> {
  return (async () => {
    try {
      const text = await navigator.clipboard.readText();
      bridge.sendToPty(buildOsc52ReadResponse(selection, text));
    } catch {
      bridge.sendToPty(buildOsc52EmptyResponse(selection));
    }
    return true;
  })();
}

/**
 * Processa o payload OSC (após `52;`), já sem o código 52 inicial.
 * Devolve `true` se a sequência foi tratada.
 */
export function handleOsc52Payload(oscData: string, bridge: Osc52Bridge): boolean | Promise<boolean> {
  if (oscData === "?") {
    return readClipboardOsc52("c", bridge);
  }

  const semi = oscData.indexOf(";");
  if (semi === -1) return false;

  const selection = oscData.slice(0, semi) || "c";
  const payload = oscData.slice(semi + 1);

  if (payload === "?") {
    return readClipboardOsc52(selection, bridge);
  }

  if (payload === "") {
    void bridge.writeText("").catch(() => {});
    return true;
  }

  const decoded = base64ToUtf8(payload);
  if (decoded === null) {
    return true;
  }

  return (async () => {
    try {
      await bridge.writeText(decoded);
    } catch {
      /* permissão / API clipboard */
    }
    return true;
  })();
}
