/**
 * One file of a workspace, full screen: line numbers, light highlighting,
 * the whole width scrolling sideways. Read-only — the phone reads code, the
 * computer edits it (`GET /api/v1/mobile/workspaces/:id/file`).
 */
import { splitPath, type WorkspaceFile } from "@dcc/mobile-core";
import { useBackend } from "@/backend/session";
import { ApiError, errorMessage } from "@/backend/types";
import { font, makeStyles, useTheme } from "@/theme";
import { GlassButton } from "@/ui/glass";
import { haptic } from "@/ui/haptics";
import { Loading, Mono } from "@/ui/primitives";
import { HighlightedCode, languageOf } from "@/ui/RichText";
import { EdgeFade } from "@/ui/surfaces";
import * as Clipboard from "expo-clipboard";
import { router, useLocalSearchParams } from "expo-router";
import { useEffect, useMemo, useState } from "react";
import { FlatList, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

/** Past this, the list still scrolls fine; the rest stays on the computer. */
const MAX_LINES = 5000;

export default function FileScreen() {
	const { workspace, path } = useLocalSearchParams<{ workspace: string; path: string }>();
	const backend = useBackend();
	const insets = useSafeAreaInsets();
	const { color } = useTheme();
	const styles = useStyles();
	const [file, setFile] = useState<WorkspaceFile | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [copied, setCopied] = useState(false);
	const filePath = String(path ?? "");
	const { name, dir } = splitPath(filePath);
	const lang = languageOf(filePath);

	useEffect(() => {
		let cancelled = false;
		backend
			.request<WorkspaceFile>(
				"GET",
				`/api/v1/mobile/workspaces/${encodeURIComponent(String(workspace))}/file?path=${encodeURIComponent(filePath)}`,
			)
			.then((next) => {
				if (!cancelled) setFile(next);
			})
			.catch((err) => {
				if (cancelled) return;
				// A desktop from before the viewer has no such route: a bare 404, without the API's error body.
				const old = err instanceof ApiError && err.status === 404 && !err.body;
				setError(old ? "Atualize o DCC no computador para abrir arquivos pelo celular." : errorMessage(err, "Não deu para abrir o arquivo."));
			});
		return () => {
			cancelled = true;
		};
	}, [backend, workspace, filePath]);

	const lines = useMemo(() => (file && !file.binary ? file.text.split("\n").slice(0, MAX_LINES) : []), [file]);
	const gutter = Math.max(2, String(lines.length).length) * 8 + 14;
	const headerSpace = insets.top + 64;

	const copy = () => {
		if (!file || file.binary) return;
		void Clipboard.setStringAsync(file.text).then(() => {
			haptic("success");
			setCopied(true);
			setTimeout(() => setCopied(false), 1500);
		});
	};

	return (
		<View style={styles.root}>
			{error ? (
				<View style={[styles.center, { paddingTop: headerSpace }]}>
					<Text style={styles.message}>{error}</Text>
				</View>
			) : !file ? (
				<View style={{ paddingTop: headerSpace }}>
					<Loading />
				</View>
			) : file.binary ? (
				<View style={[styles.center, { paddingTop: headerSpace }]}>
					<Text style={styles.message}>Arquivo binário — abra no computador.</Text>
				</View>
			) : (
				<ScrollView horizontal bounces={false} contentContainerStyle={{ minWidth: "100%" }}>
					<FlatList
						data={lines}
						keyExtractor={(_, index) => String(index)}
						initialNumToRender={60}
						windowSize={9}
						contentContainerStyle={{ paddingTop: headerSpace, paddingBottom: insets.bottom + 40, paddingRight: 24 }}
						scrollIndicatorInsets={{ top: headerSpace - insets.top }}
						ListFooterComponent={
							file.truncated || file.text.split("\n").length > MAX_LINES ? (
								<Text style={[styles.message, { paddingLeft: gutter + 12, paddingTop: 16 }]}>
									O arquivo continua no computador (mostrando o começo).
								</Text>
							) : null
						}
						renderItem={({ item, index }) => (
							<View style={styles.line}>
								<Text style={[styles.number, { width: gutter }]}>{index + 1}</Text>
								<Text style={styles.code} selectable>
									<HighlightedCode code={item || " "} lang={lang} />
								</Text>
							</View>
						)}
					/>
				</ScrollView>
			)}

			<EdgeFade edge="top" height={headerSpace + 12} solid={(headerSpace - 6) / (headerSpace + 12)} />
			<View style={[styles.header, { top: insets.top + 6 }]}>
				<GlassButton icon="x" accessibilityLabel="Fechar" onPress={() => router.back()} />
				<View style={styles.titleBlock}>
					<Text style={styles.title} numberOfLines={1} accessibilityRole="header">
						{name}
					</Text>
					<Mono style={styles.subtitle} numberOfLines={1}>
						{[dir.replace(/\/$/, ""), file && !file.binary ? `${lines.length} linhas` : null].filter(Boolean).join(" · ")}
					</Mono>
				</View>
				<GlassButton
					icon={copied ? "check" : "copy"}
					tint={copied ? color.accent : color.fg}
					accessibilityLabel={copied ? "Copiado" : "Copiar o arquivo"}
					onPress={copy}
				/>
			</View>
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		center: { flex: 1, alignItems: "center", paddingHorizontal: 32 },
		message: { color: color.mute, fontSize: 14, lineHeight: 20, textAlign: "center" },
		header: { position: "absolute", left: 16, right: 16, flexDirection: "row", alignItems: "center", gap: 10 },
		titleBlock: { flex: 1, minWidth: 0, alignItems: "center" },
		title: { color: color.fg, fontSize: 16, fontWeight: "600" },
		subtitle: { color: color.mute, fontSize: 11, marginTop: 2 },
		line: { flexDirection: "row", minHeight: 20 },
		number: { color: color.faint, fontFamily: font.mono, fontSize: 11, lineHeight: 20, textAlign: "right", paddingRight: 12 },
		code: { color: color.fg, fontFamily: font.mono, fontSize: 12.5, lineHeight: 20 },
	}),
);
