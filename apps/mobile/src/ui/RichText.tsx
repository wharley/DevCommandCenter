/**
 * Just enough markdown for agent answers on a phone: fenced code blocks
 * (language, copy, light highlighting), inline code, headings, bullets and
 * bold. Full markdown (tables, diff) is a follow-up — the plan is to try
 * Expo DOM components with a web renderer.
 */
import { highlightCode, type CodeTokenKind } from "@dcc/mobile-core";
import Feather from "@expo/vector-icons/Feather";
import * as Clipboard from "expo-clipboard";
import { Fragment, useEffect, useMemo, useState } from "react";
import { Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import { font, makeStyles, radius, useTheme, type Scheme } from "../theme";
import { haptic } from "./haptics";

type Block = { kind: "code"; text: string; lang: string } | { kind: "text"; text: string };

export function splitBlocks(source: string): Block[] {
	const blocks: Block[] = [];
	// The capture keeps each fence's info string: text, open info, code, close info, text…
	const parts = source.split(/^```([^\n]*)\n?/m);
	for (let i = 0; i < parts.length; i += 4) {
		const text = parts[i];
		if (text) blocks.push({ kind: "text", text });
		const code = parts[i + 2];
		if (code !== undefined && code !== "") {
			blocks.push({ kind: "code", text: code.replace(/\n$/, ""), lang: (parts[i + 1] ?? "").trim().split(/\s+/)[0] ?? "" });
		}
	}
	return blocks;
}

const TOKEN_COLORS: Record<Scheme, Record<Exclude<CodeTokenKind, "plain">, string>> = {
	dark: { keyword: "#c792ea", string: "#c3e88d", number: "#f78c6c", comment: "#6b7685" },
	light: { keyword: "#8a3fb8", string: "#2f7d32", number: "#b5530c", comment: "#8a93a0" },
};

/** A fenced block: language and copy on top, the code scrolling sideways, lightly colored. */
function CodeBlock({ text, lang }: { text: string; lang: string }) {
	const { color, scheme } = useTheme();
	const styles = useStyles();
	const [copied, setCopied] = useState(false);
	const tokens = useMemo(() => highlightCode(text, lang), [text, lang]);
	useEffect(() => {
		if (!copied) return;
		const timer = setTimeout(() => setCopied(false), 1500);
		return () => clearTimeout(timer);
	}, [copied]);
	const copy = () => {
		void Clipboard.setStringAsync(text).then(() => {
			haptic("success");
			setCopied(true);
		});
	};
	return (
		<View style={styles.codeBox}>
			<View style={styles.codeHead}>
				<Text style={styles.codeLang}>{lang || "código"}</Text>
				<Pressable onPress={copy} hitSlop={8} accessibilityRole="button" accessibilityLabel={copied ? "Copiado" : "Copiar código"} style={styles.copy}>
					<Feather name={copied ? "check" : "copy"} size={14} color={copied ? color.accent : color.mute} />
				</Pressable>
			</View>
			<ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.codeScroll}>
				<Text style={styles.code} selectable>
					{tokens.map((token, i) =>
						token.kind === "plain" ? (
							<Fragment key={i}>{token.text}</Fragment>
						) : (
							<Text key={i} style={{ color: TOKEN_COLORS[scheme][token.kind], fontStyle: token.kind === "comment" ? "italic" : "normal" }}>
								{token.text}
							</Text>
						),
					)}
				</Text>
			</ScrollView>
		</View>
	);
}

function Inline({ text }: { text: string }) {
	const styles = useStyles();
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
	const styles = useStyles();
	return (
		<View style={{ gap: 8 }}>
			{splitBlocks(text).map((block, i) =>
				block.kind === "code" ? (
					<CodeBlock key={i} text={block.text} lang={block.lang} />
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

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		text: { color: color.fg, fontSize: 15, lineHeight: 22 },
		heading: { fontWeight: "700" },
		bold: { fontWeight: "700" },
		inlineCode: { fontFamily: font.mono, fontSize: 13, color: color.fg, backgroundColor: color.chip },
		codeBox: {
			backgroundColor: color.panel,
			borderRadius: radius.lg,
			borderWidth: StyleSheet.hairlineWidth,
			borderColor: color.border,
			overflow: "hidden",
		},
		codeHead: {
			flexDirection: "row",
			alignItems: "center",
			justifyContent: "space-between",
			paddingLeft: 14,
			paddingRight: 6,
			height: 38,
			borderBottomWidth: StyleSheet.hairlineWidth,
			borderBottomColor: color.border,
		},
		codeLang: { fontFamily: font.mono, fontSize: 12, color: color.mute },
		copy: { width: 32, height: 32, borderRadius: 8, alignItems: "center", justifyContent: "center" },
		codeScroll: { paddingHorizontal: 14, paddingVertical: 12 },
		code: { fontFamily: font.mono, fontSize: 12.5, lineHeight: 20, color: color.fg },
		bulletRow: { flexDirection: "row", gap: 8 },
	}),
);
