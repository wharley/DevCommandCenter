/**
 * Extrai tokens úteis para navegação cruzada entre repos (rotas API, paths, URLs).
 * Heurística leve — sem AST; foco em produtividade humana na Review.
 */
const MAX_TOKENS = 12;

export function extractContextTokens(diff: string): string[] {
  if (!diff?.trim()) return [];
  const found = new Set<string>();

  const patterns: RegExp[] = [
    /\/api[\w\-/.]*(?:\?[\w=&%-]*)?/gi,
    /['"`](\/[\w\-/.]+)['"`]/g,
    /fetch\s*\(\s*['"`]([^'"`]+)['"`]/gi,
    /axios\.(?:get|post|put|patch|delete)\s*\(\s*['"`]([^'"`]+)['"`]/gi,
    /https?:\/\/[^\s'"`<>]+/gi,
  ];

  for (const re of patterns) {
    let m: RegExpExecArray | null;
    const r = new RegExp(re.source, re.flags);
    while ((m = r.exec(diff)) !== null) {
      const raw = (m[1] ?? m[0]).trim();
      if (raw.length < 4 || raw.length > 200) continue;
      if (raw.startsWith("http")) {
        try {
          const u = new URL(raw);
          found.add(u.pathname + (u.search || ""));
        } catch {
          found.add(raw);
        }
      } else {
        found.add(raw);
      }
      if (found.size >= MAX_TOKENS) break;
    }
    if (found.size >= MAX_TOKENS) break;
  }

  return Array.from(found).slice(0, MAX_TOKENS);
}
