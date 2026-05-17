import hljs from "highlight.js/lib/core";
import bash from "highlight.js/lib/languages/bash";
import css from "highlight.js/lib/languages/css";
import diff from "highlight.js/lib/languages/diff";
import go from "highlight.js/lib/languages/go";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import shell from "highlight.js/lib/languages/shell";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

// Only register languages we're likely to see in code-related chats.
// Each extra language adds a few KB; skip the long tail (perl, lisp, etc).
const LANGS: Array<[string, unknown]> = [
	["bash", bash],
	["sh", bash],
	["shell", shell],
	["zsh", shell],
	["css", css],
	["diff", diff],
	["patch", diff],
	["go", go],
	["json", json],
	["markdown", markdown],
	["md", markdown],
	["python", python],
	["py", python],
	["rust", rust],
	["rs", rust],
	["sql", sql],
	["typescript", typescript],
	["ts", typescript],
	["tsx", typescript],
	["javascript", typescript],
	["js", typescript],
	["jsx", typescript],
	["xml", xml],
	["html", xml],
	["yaml", yaml],
	["yml", yaml],
];

let registered = false;
function ensureRegistered() {
	if (registered) return;
	registered = true;
	for (const [name, mod] of LANGS) {
		// hljs.registerLanguage tolerates duplicate registrations.
		hljs.registerLanguage(name, mod as Parameters<typeof hljs.registerLanguage>[1]);
	}
}

export function highlightCode(
	source: string,
	language: string | null,
): { html: string; language: string | null } {
	ensureRegistered();
	const lang = (language ?? "").toLowerCase();
	if (lang && hljs.getLanguage(lang)) {
		try {
			const result = hljs.highlight(source, { language: lang, ignoreIllegals: true });
			return { html: result.value, language: lang };
		} catch {
			/* fall through to auto */
		}
	}
	// Auto-detect when language is unknown — keeps inline ```fenced``` blocks
	// without a tag readable.
	try {
		const auto = hljs.highlightAuto(source);
		return { html: auto.value, language: auto.language ?? null };
	} catch {
		return { html: escapeHtml(source), language: null };
	}
}

function escapeHtml(s: string): string {
	return s
		.replace(/&/g, "&amp;")
		.replace(/</g, "&lt;")
		.replace(/>/g, "&gt;")
		.replace(/"/g, "&quot;")
		.replace(/'/g, "&#039;");
}
