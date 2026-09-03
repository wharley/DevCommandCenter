import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en/common.json";
import ptBR from "@/i18n/locales/pt-BR/common.json";
import {
	HELP_TOPIC_ICONS,
	HELP_TOPIC_IDS,
	isHelpTopicId,
	matchesHelpTopic,
} from "./help-topics";

type LocaleBundle = typeof ptBR;
type HelpTopicCopy = LocaleBundle["help"]["topics"][keyof LocaleBundle["help"]["topics"]];

const LOCALES: Array<[string, LocaleBundle]> = [
	["pt-BR", ptBR],
	["en", en as unknown as LocaleBundle],
];

/**
 * UI labels quoted in help copy that are not i18n values: slash commands,
 * hard-coded strings, and templated labels whose interpolated form is not
 * a literal in the bundle.
 */
const LABEL_ALLOWLIST: Record<string, string[]> = {
	"pt-BR": ["/spec", "/clear", "Criar PR", "Parear novo dispositivo"],
	en: ["/spec", "/clear", "Create PR", "Pair new device"],
};

function collectStringValues(node: unknown, out: Set<string>): Set<string> {
	if (typeof node === "string") {
		out.add(node);
	} else if (Array.isArray(node)) {
		node.forEach((item) => collectStringValues(item, out));
	} else if (node && typeof node === "object") {
		Object.values(node).forEach((value) => collectStringValues(value, out));
	}
	return out;
}

function collectQuotedLabels(topic: HelpTopicCopy): string[] {
	const texts = [
		topic.whatIs,
		topic.whenToUse,
		...topic.steps,
		"tip" in topic ? topic.tip : "",
	];
	return texts.flatMap((text) =>
		Array.from(text.matchAll(/\[([^\]]+)\]/g), (match) => match[1]!),
	);
}

describe("help topics", () => {
	it("has an icon for every topic id", () => {
		for (const id of HELP_TOPIC_IDS) {
			expect(HELP_TOPIC_ICONS[id]).toBeDefined();
		}
	});

	it("recognises topic ids and rejects strangers", () => {
		expect(isHelpTopicId("delegate")).toBe(true);
		expect(isHelpTopicId("settings")).toBe(false);
	});

	it("matches search ignoring case and accents", () => {
		expect(matchesHelpTopic("Delegar para outro agente", "DELEGAR")).toBe(true);
		expect(matchesHelpTopic("Revisão do último turno", "revisao ultimo")).toBe(true);
		expect(matchesHelpTopic("Terminal", "browser")).toBe(false);
		expect(matchesHelpTopic("anything", "   ")).toBe(true);
	});

	describe.each(LOCALES)("%s copy", (locale, bundle) => {
		it("covers every topic with the fields the dialog renders", () => {
			for (const id of HELP_TOPIC_IDS) {
				const topic = bundle.help.topics[id];
				expect(topic, `missing help.topics.${id}`).toBeDefined();
				expect(topic.label.length).toBeGreaterThan(0);
				expect(topic.summary.length).toBeGreaterThan(0);
				expect(topic.whatIs.length).toBeGreaterThan(0);
				expect(topic.whenToUse.length).toBeGreaterThan(0);
				expect(topic.steps.length).toBeGreaterThanOrEqual(3);
			}
		});

		it("only quotes [labels] that exist in the app's own strings", () => {
			const known = collectStringValues(bundle, new Set<string>());
			// Values the bundle itself owns but the help copy quotes without
			// the interpolation suffix (e.g. "Criar {{requestLabel}}").
			const allowlist = new Set(LABEL_ALLOWLIST[locale] ?? []);
			const unknown: string[] = [];
			for (const id of HELP_TOPIC_IDS) {
				for (const label of collectQuotedLabels(bundle.help.topics[id])) {
					if (!known.has(label) && !allowlist.has(label)) {
						unknown.push(`${id}: [${label}]`);
					}
				}
			}
			expect(unknown).toEqual([]);
		});
	});
});
