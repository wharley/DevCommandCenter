import type { CSSProperties } from "react";

export type CodeToken = { content: string; style?: CSSProperties };
export type HighlightedLines = CodeToken[][];

/** Same themes as the diff surfaces, so code reads the same in chat and in review. */
const THEMES = { light: "pierre-light", dark: "pierre-dark" } as const;
/** Above this the block stays plain: tokenizing and thousands of spans cost more than they give. */
export const HIGHLIGHT_MAX_CHARS = 60_000;
const CACHE_LIMIT = 64;

/** Insertion-ordered, so the first key is the least recently used. */
const cache = new Map<string, HighlightedLines>();
const unsupportedLanguages = new Set<string>();

const cacheKey = (code: string, language: string) => `${language}\u0000${code}`;

/** Synchronous hit, so a block scrolled back into view paints highlighted on its first frame. */
export function cachedHighlight(code: string, language: string): HighlightedLines | null {
	const key = cacheKey(code, language);
	const hit = cache.get(key);
	if (!hit) return null;
	cache.delete(key);
	cache.set(key, hit);
	return hit;
}

/**
 * Tokens for both themes, or null when the block should stay plain (unknown
 * language, too large). Reuses the diff view's highlighter: one shiki
 * instance, grammars loaded on demand.
 */
export async function highlightCode(
	code: string,
	language: string,
): Promise<HighlightedLines | null> {
	if (code.length > HIGHLIGHT_MAX_CHARS || unsupportedLanguages.has(language)) return null;
	const cached = cachedHighlight(code, language);
	if (cached) return cached;
	try {
		const { getSharedHighlighter } = await import("@pierre/diffs");
		const highlighter = await getSharedHighlighter({
			themes: [THEMES.light, THEMES.dark],
			langs: [language],
		});
		const { tokens } = highlighter.codeToTokens(code, {
			lang: language,
			themes: THEMES,
			defaultColor: false,
		});
		const lines = tokens.map((line) =>
			line.map((token) => ({
				content: token.content,
				style: token.htmlStyle as CSSProperties | undefined,
			})),
		);
		cache.set(cacheKey(code, language), lines);
		if (cache.size > CACHE_LIMIT) {
			cache.delete(cache.keys().next().value as string);
		}
		return lines;
	} catch {
		// No grammar for this fence label; do not retry on every render.
		unsupportedLanguages.add(language);
		return null;
	}
}
