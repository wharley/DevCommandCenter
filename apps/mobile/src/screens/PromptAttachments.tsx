/**
 * Attachments of a sent message, as the desktop's `UserMessage` shows them:
 * images as themselves (a tap opens them full size), other files as a chip
 * with the name. The image comes from the computer
 * (`GET /api/v1/mobile/sessions/:id/attachment`), which only serves files this
 * session's prompts attached; a file the OS already cleaned falls back to the chip.
 */
import { isImagePath, pathBasename, t } from "@dcc/mobile-core";
import Feather from "@expo/vector-icons/Feather";
import { useEffect, useState } from "react";
import { Image, Modal, Pressable, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import type { Backend } from "../backend/types";
import { makeStyles, radius, useTheme } from "../theme";

/** Thumbnails already fetched (or known to be gone) in this run of the app. */
const previews = new Map<string, Promise<string | null>>();

function loadPreview(backend: Backend, sessionId: string, path: string): Promise<string | null> {
	const key = `${backend.mode}|${sessionId}|${path}`;
	let pending = previews.get(key);
	if (!pending) {
		pending = backend
			.request<{ dataUrl?: string }>(
				"GET",
				`/api/v1/mobile/sessions/${encodeURIComponent(sessionId)}/attachment?path=${encodeURIComponent(path)}`,
			)
			.then((body) => body?.dataUrl ?? null)
			.catch(() => {
				// Offline is not "gone": ask again next time.
				previews.delete(key);
				return null;
			});
		previews.set(key, pending);
	}
	return pending;
}

export function PromptAttachments({ paths, backend, sessionId }: { paths: string[]; backend: Backend; sessionId: string }) {
	const styles = useStyles();
	return (
		<View style={styles.row}>
			{paths.map((path, index) =>
				isImagePath(path) ? (
					<ImageAttachment key={`${path}-${index}`} path={path} backend={backend} sessionId={sessionId} />
				) : (
					<AttachmentChip key={`${path}-${index}`} path={path} />
				),
			)}
		</View>
	);
}

function AttachmentChip({ path }: { path: string }) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<View style={styles.chip}>
			<Feather name={isImagePath(path) ? "image" : "file"} size={12} color={color.mute} />
			<Text style={styles.chipText} numberOfLines={1}>
				{pathBasename(path)}
			</Text>
		</View>
	);
}

function ImageAttachment({ path, backend, sessionId }: { path: string; backend: Backend; sessionId: string }) {
	const styles = useStyles();
	const [state, setState] = useState<{ loading: boolean; uri: string | null }>({ loading: true, uri: null });
	const [open, setOpen] = useState(false);
	const insets = useSafeAreaInsets();
	useEffect(() => {
		let cancelled = false;
		void loadPreview(backend, sessionId, path).then((uri) => {
			if (!cancelled) setState({ loading: false, uri });
		});
		return () => {
			cancelled = true;
		};
	}, [backend, sessionId, path]);
	if (state.loading) return <View style={[styles.image, styles.placeholder]} />;
	if (!state.uri) return <AttachmentChip path={path} />;
	const name = pathBasename(path);
	return (
		<>
			<Pressable onPress={() => setOpen(true)} accessibilityRole="imagebutton" accessibilityLabel={t("conversation.attachment.openImage", { name })}>
				<Image source={{ uri: state.uri }} style={styles.image} resizeMode="cover" />
			</Pressable>
			<Modal visible={open} transparent animationType="fade" onRequestClose={() => setOpen(false)}>
				<Pressable style={[styles.viewer, { paddingTop: insets.top, paddingBottom: insets.bottom }]} onPress={() => setOpen(false)} accessibilityLabel="Fechar">
					<Image source={{ uri: state.uri }} style={styles.full} resizeMode="contain" accessibilityLabel={name} />
				</Pressable>
			</Modal>
		</>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		row: { flexDirection: "row", flexWrap: "wrap", justifyContent: "flex-end", gap: 6 },
		chip: {
			flexDirection: "row",
			alignItems: "center",
			gap: 6,
			maxWidth: 220,
			paddingHorizontal: 8,
			paddingVertical: 5,
			borderRadius: radius.sm,
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.bg,
		},
		chipText: { color: color.mute, fontSize: 11, flexShrink: 1 },
		image: { width: 132, height: 100, borderRadius: radius.sm, borderWidth: 1, borderColor: color.border },
		placeholder: { backgroundColor: color.elevated },
		viewer: { flex: 1, backgroundColor: "rgba(0,0,0,0.92)", alignItems: "center", justifyContent: "center" },
		full: { width: "100%", height: "100%" },
	}),
);
