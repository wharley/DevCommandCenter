/**
 * Just enough markdown for agent answers on a phone: fenced code blocks,
 * inline code, headings, bullets and bold. Full markdown (tables, diff) is a
 * follow-up — the plan is to try Expo DOM components with a web renderer.
 */
import { Fragment } from "react";
import { ScrollView, StyleSheet, Text, View } from "react-native";
import { color, font, radius } from "../theme";

type Block = { kind: "code"; text: string } | { kind: "text"; text: string };

export function splitBlocks(source: string): Block[] {
	const blocks: Block[] = [];
	const parts = source.split(/^```[^\n]*\n?/m);
	parts.forEach((part, index) => {
		if (!part) return;
		blocks.push(index % 2 === 1 ? { kind: "code", text: part.replace(/\n$/, "") } : { kind: "text", text: part });
	});
	return blocks;
}

function Inline({ text }: { text: string }) {
	const pieces = text.split(/(`[^`\n]+`|\*\*[^*\n]+\*\*)/g);
	return (
		<>
			{pieces.map((piece, i) => {
				if (piece.startsWith("`") && piece.endsWith("`") && piece.length > 2) {
					return (
						<Text key={i} style={styles.inlineCode}>
							{piece.slice(1, -1)}
						</Text>
					);
				}
				if (piece.startsWith("**") && piece.endsWith("**") && piece.length > 4) {
					return (
						<Text key={i} style={styles.bold}>
							{piece.slice(2, -2)}
						</Text>
					);
				}
				return <Fragment key={i}>{piece}</Fragment>;
			})}
		</>
	);
}

export function RichText({ text }: { text: string }) {
	return (
		<View style={{ gap: 8 }}>
			{splitBlocks(text).map((block, i) =>
				block.kind === "code" ? (
					<ScrollView key={i} horizontal style={styles.codeBox} contentContainerStyle={{ padding: 10 }}>
						<Text style={styles.code} selectable>
							{block.text}
						</Text>
					</ScrollView>
				) : (
					<View key={i} style={{ gap: 4 }}>
						{block.text
							.trim()
							.split("\n")
							.map((line, j) => {
								const heading = /^#{1,6}\s+(.*)$/.exec(line);
								const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
								if (heading) {
									return (
										<Text key={j} style={[styles.text, styles.heading]} selectable>
											<Inline text={heading[1]!} />
										</Text>
									);
								}
								if (bullet) {
									return (
										<View key={j} style={styles.bulletRow}>
											<Text style={styles.text}>•</Text>
											<Text style={[styles.text, { flex: 1 }]} selectable>
												<Inline text={bullet[1]!} />
											</Text>
										</View>
									);
								}
								return line.trim() ? (
									<Text key={j} style={styles.text} selectable>
										<Inline text={line} />
									</Text>
								) : (
									<View key={j} style={{ height: 4 }} />
								);
							})}
					</View>
				),
			)}
		</View>
	);
}

const styles = StyleSheet.create({
	text: { color: color.fg, fontSize: 15, lineHeight: 22 },
	heading: { fontWeight: "700" },
	bold: { fontWeight: "700" },
	inlineCode: { fontFamily: font.mono, fontSize: 13, color: color.accent, backgroundColor: color.elevated },
	codeBox: { backgroundColor: color.panel, borderRadius: radius.sm, borderWidth: 1, borderColor: color.border },
	code: { fontFamily: font.mono, fontSize: 12, lineHeight: 18, color: color.fg },
	bulletRow: { flexDirection: "row", gap: 8 },
});
