/**
 * A small highlighter for code blocks on the phone: comments, strings,
 * numbers and the keywords most languages share. Not a parser — enough
 * color to read a snippet, nothing that can fail on odd input.
 */
export type CodeTokenKind = "plain" | "keyword" | "string" | "comment" | "number";
export type CodeToken = { kind: CodeTokenKind; text: string };

const KEYWORDS = new Set(
	(
		"abstract as async await break case catch class const continue def default defer del do elif else enum " +
		"except export extends false finally fn for from func function go if impl implements import in interface " +
		"is lambda let loop match mod mut new nil none None not null or package pass private protected pub public " +
		"raise readonly return self Self static struct super switch this throw true True False try type typeof use " +
		"var void where while with yield"
	).split(" "),
);

/** Comment styles by fence language; unknown languages get both. */
function lineComment(lang: string): RegExp {
	if (/^(py|python|sh|bash|zsh|shell|yaml|yml|toml|ruby|rb|dockerfile|make|r)$/i.test(lang)) return /#[^\n]*/y;
	if (/^(ts|tsx|js|jsx|javascript|typescript|rust|rs|go|java|kotlin|kt|swift|c|cpp|cs|css|scss|json|jsonc)$/i.test(lang))
		return /\/\/[^\n]*/y;
	return /(?:\/\/|#)[^\n]*/y;
}

export function highlightCode(code: string, lang = ""): CodeToken[] {
	const comment = lineComment(lang);
	const block = /\/\*[\s\S]*?(?:\*\/|$)/y;
	const string = /(["'`])(?:\\[\s\S]|(?!\1)[^\\\n])*\1?/y;
	const number = /\b\d[\d_]*(?:\.\d+)?\b/y;
	const word = /[A-Za-z_$][\w$]*/y;
	const tokens: CodeToken[] = [];
	const push = (kind: CodeTokenKind, text: string) => {
		const last = tokens.at(-1);
		if (last && last.kind === kind) last.text += text;
		else tokens.push({ kind, text });
	};
	let i = 0;
	while (i < code.length) {
		let match: RegExpExecArray | null = null;
		for (const [kind, re] of [
			["comment", block],
			["comment", comment],
			["string", string],
			["number", number],
		] as const) {
			re.lastIndex = i;
			match = re.exec(code);
			if (match && match[0]) {
				push(kind, match[0]);
				i += match[0].length;
				break;
			}
			match = null;
		}
		if (match) continue;
		word.lastIndex = i;
		const w = word.exec(code);
		if (w) {
			push(KEYWORDS.has(w[0]) ? "keyword" : "plain", w[0]);
			i += w[0].length;
			continue;
		}
		push("plain", code[i]!);
		i += 1;
	}
	return tokens;
}
