/**
 * The `@` list above the keyboard: the workspace's tracked files, filtered on
 * the desktop as the person types (the desktop's ranking, 50 at most). The
 * phone only ever sends an id and the typed text — never a path to read.
 */
import { fileSearchPath, splitPath, t, type FileSearchResult } from "@dcc/mobile-core";
import Feather from "@expo/vector-icons/Feather";
import { useEffect, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import type { Backend } from "../backend/types";
import { font, makeStyles, radius, useTheme } from "../theme";
import { Mono } from "./primitives";

export type FileScope = { workspaceId: string } | { repositoryId: string };

const DEBOUNCE_MS = 120;

export function FileMentionList({
	backend,
	scope,
	query,
	onPick,
}: {
	backend: Backend;
	scope: FileScope;
	/** What follows the `@`. */
	query: string;
	onPick: (path: string) => void;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	const [files, setFiles] = useState<string[] | null>(null);
	const [error, setError] = useState<string | null>(null);
	const scopeKey = "workspaceId" in scope ? `w:${scope.workspaceId}` : `r:${scope.repositoryId}`;

	useEffect(() => {
		let cancelled = false;
		const timer = setTimeout(() => {
			backend
				.request<FileSearchResult>("GET", fileSearchPath(scope, query))
				.then((result) => {
					if (cancelled) return;
					setFiles(result.files.map((file) => file.path));
					setError(null);
				})
				.catch((reason: unknown) => {
					if (cancelled) return;
					setError(reason instanceof Error ? reason.message : String(reason));
				});
		}, DEBOUNCE_MS);
		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
		// `scope` is an object literal per render; its key is what matters.
	}, [backend, scopeKey, query]);

	return (
		<View style={styles.panel} accessibilityLabel={t("composer.fileMentions")}>
			<View style={styles.head}>
				<Mono style={styles.title}>{t("composer.fileMentions")}</Mono>
				{files === null && !error ? <ActivityIndicator size="small" color={color.faint} style={{ transform: [{ scale: 0.7 }] }} /> : null}
			</View>
			{error ? (
				<Text style={styles.empty}>{error}</Text>
			) : files === null ? (
				<Text style={styles.empty}>{t("quickOpen.loading")}</Text>
			) : files.length === 0 ? (
				<Text style={styles.empty}>{t("quickOpen.empty")}</Text>
			) : (
				<ScrollView style={styles.list} keyboardShouldPersistTaps="always" nestedScrollEnabled>
					{files.map((path) => {
						const { name, dir } = splitPath(path);
						return (
							<Pressable
								key={path}
								onPress={() => onPick(path)}
								accessibilityRole="button"
								accessibilityLabel={`@${path}`}
								style={({ pressed }) => [styles.row, pressed && { backgroundColor: color.elevated }]}
							>
								<Feather name="file-text" size={13} color={color.faint} />
								<Text style={styles.name} numberOfLines={1}>
									{name}
								</Text>
								<Text style={styles.dir} numberOfLines={1} ellipsizeMode="head">
									{dir}
								</Text>
							</Pressable>
						);
					})}
				</ScrollView>
			)}
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		panel: {
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.panel,
			borderRadius: radius.md,
			overflow: "hidden",
		},
		head: { flexDirection: "row", alignItems: "center", gap: 6, paddingHorizontal: 12, paddingTop: 8, paddingBottom: 4 },
		title: { fontSize: 10, letterSpacing: 1.4, textTransform: "uppercase", color: color.mute },
		list: { maxHeight: 200 },
		row: { flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 12, paddingVertical: 9 },
		name: { color: color.fg, fontSize: 14, fontFamily: font.mono, flexShrink: 0, maxWidth: "60%" },
		dir: { color: color.faint, fontSize: 12, fontFamily: font.mono, flex: 1 },
		empty: { color: color.faint, fontSize: 12, paddingHorizontal: 12, paddingBottom: 10 },
	}),
);
