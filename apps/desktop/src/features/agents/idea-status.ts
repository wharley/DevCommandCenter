/**
 * Reads an idea's IDEIA.md by its fixed headings (see the researcher's role
 * in crates/dcc-core/src/domain/agent.rs). DCC, not the agent, decides which
 * milestones are filled. The parse is tolerant: anything it does not
 * recognise is ignored and leaves the milestone open instead of failing.
 */

export const IDEA_FILE = "IDEIA.md";

export const IDEA_MILESTONES = ["problem", "audience", "references", "hypotheses", "name"] as const;
export type IdeaMilestone = (typeof IDEA_MILESTONES)[number];

export type IdeaVerdict = "seguir" | "pivotar" | "descartar";
export type HypothesisState = "aberta" | "confirmada" | "derrubada";

export type IdeaStatus = {
	/** The idea in one sentence, from `# Ideia: …`; null while it is still empty. */
	title: string | null;
	milestones: Record<IdeaMilestone, boolean>;
	filledCount: number;
	verdict: IdeaVerdict | null;
	problem: string;
	audience: string;
	mvp: string;
	/** Name candidates in order; the favourite (★) first when there is one. */
	names: string[];
	favoriteName: string | null;
	referenceCount: number;
	hypotheses: { state: HypothesisState; text: string }[];
	hasArchitecture: boolean;
	hasRoadmap: boolean;
};

const MIN_REFERENCES = 3;

function normalizeHeading(value: string): string {
	return value
		.normalize("NFD")
		.replace(/[̀-ͯ]/g, "")
		.trim()
		.toLowerCase();
}

/** The body of every level-2 section, keyed by its normalized heading. */
function sections(markdown: string): Map<string, string> {
	const result = new Map<string, string>();
	let current: string | null = null;
	let body: string[] = [];
	for (const line of markdown.split(/\r?\n/)) {
		const heading = /^##\s+(.+?)\s*#*\s*$/.exec(line);
		if (heading && !line.startsWith("###")) {
			if (current !== null) result.set(current, body.join("\n").trim());
			current = normalizeHeading(heading[1] ?? "");
			body = [];
		} else if (current !== null) {
			body.push(line);
		}
	}
	if (current !== null) result.set(current, body.join("\n").trim());
	return result;
}

function listItems(body: string): string[] {
	return body
		.split(/\r?\n/)
		.map((line) => /^\s*[-*+]\s+(.*)$/.exec(line)?.[1]?.trim())
		.filter((item): item is string => Boolean(item));
}

/** Plain text of a section without comments, for the card. */
function plain(body: string): string {
	return body
		.replace(/<!--[\s\S]*?-->/g, "")
		.split(/\r?\n/)
		.map((line) => line.trim())
		.filter(Boolean)
		.join(" ")
		.trim();
}

function stripMarkup(value: string): string {
	return value.replace(/[*_`]/g, "").trim();
}

export function parseIdeaStatus(markdown: string): IdeaStatus {
	const byHeading = sections(markdown);
	const section = (name: string) => byHeading.get(normalizeHeading(name)) ?? "";

	const titleMatch = /^#\s+Ideia\s*:\s*(.+?)\s*$/im.exec(markdown);
	const title = titleMatch?.[1] ? stripMarkup(titleMatch[1]) || null : null;

	const problem = plain(section("Problema"));
	const audience = plain(section("Para quem"));

	const referenceCount = listItems(section("Referências")).filter((item) =>
		/https?:\/\/\S+/.test(item),
	).length;

	const hypotheses = listItems(section("Hipóteses")).flatMap((item) => {
		const match = /^\[\s*(aberta|confirmada|derrubada)\s*\]\s*(.*)$/i.exec(item);
		return match
			? [{ state: (match[1] ?? "").toLowerCase() as HypothesisState, text: (match[2] ?? "").trim() }]
			: [];
	});

	let favoriteName: string | null = null;
	const names: string[] = [];
	for (const item of listItems(section("Nome"))) {
		const favorite = item.startsWith("★");
		const name = stripMarkup(item.replace(/^★\s*/, "").split(/\s+[—–-]\s+/)[0] ?? "");
		if (!name) continue;
		if (favorite && favoriteName === null) {
			favoriteName = name;
			names.unshift(name);
		} else if (!names.includes(name)) {
			names.push(name);
		}
	}

	const verdictLine = stripMarkup(
		section("Veredito")
			.split(/\r?\n/)
			.map((line) => line.trim())
			.find(Boolean) ?? "",
	).toLowerCase();
	const verdict = (["seguir", "pivotar", "descartar"] as const).find((candidate) =>
		verdictLine.startsWith(candidate),
	) ?? null;

	const milestones: Record<IdeaMilestone, boolean> = {
		problem: problem.length > 0,
		audience: audience.length > 0,
		references: referenceCount >= MIN_REFERENCES,
		hypotheses: hypotheses.length > 0 && hypotheses.every((item) => item.state !== "aberta"),
		name: favoriteName !== null,
	};

	return {
		title,
		milestones,
		filledCount: IDEA_MILESTONES.filter((milestone) => milestones[milestone]).length,
		verdict,
		problem,
		audience,
		mvp: plain(section("MVP")),
		names,
		favoriteName,
		referenceCount,
		hypotheses,
		hasArchitecture: plain(section("Arquitetura e stack")).length > 0,
		hasRoadmap: /^###\s+\S/m.test(section("Roadmap")),
	};
}

/** A GitHub repository name, which is also the project's folder name. */
export function isValidRepositoryName(name: string): boolean {
	return /^[A-Za-z0-9._-]{1,100}$/.test(name) && name !== "." && name !== "..";
}

/** A repository name from a name candidate: "Salte Saúde" → "salte-saude". */
export function repositoryNameFromCandidate(candidate: string): string {
	return candidate
		.normalize("NFD")
		.replace(/[̀-ͯ]/g, "")
		.toLowerCase()
		.replace(/[^a-z0-9._-]+/g, "-")
		.replace(/^-+|-+$/g, "")
		.slice(0, 100);
}
