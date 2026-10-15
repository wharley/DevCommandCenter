/**
 * What sits around a composer's text field: the paperclip (photo, camera,
 * file), the thumbnails of what is attached, and the microphone for dictation.
 * Used by the thread and by "Nova tarefa".
 */
import { formatBytes, MAX_ATTACHMENT_BYTES, t } from "@dcc/mobile-core";
import Feather from "@expo/vector-icons/Feather";
import { useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Image, Linking, Modal, Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { pickAttachments, type PickSource } from "../attachments/pick";
import { AttachmentQueue, type PendingAttachment } from "../attachments/queue";
import type { Backend } from "../backend/types";
import { errorMessage } from "../backend/types";
import type { Dictation } from "../dictation/useDictation";
import { alpha, makeStyles, radius, useTheme } from "../theme";
import { Mono } from "./primitives";

export function useAttachments(backend: Backend) {
	const [items, setItems] = useState<PendingAttachment[]>([]);
	const [notice, setNotice] = useState<string | null>(null);
	const queue = useMemo(() => new AttachmentQueue(backend, setItems), [backend]);
	useEffect(() => () => queue.clear(), [queue]);

	const pick = async (source: PickSource) => {
		setNotice(null);
		try {
			const outcome = await pickAttachments(source, queue.room());
			if (outcome.kind === "denied") setNotice(outcome.message);
			if (outcome.kind !== "picked") return;
			const refused = [...outcome.refused];
			for (const file of outcome.files) {
				const refusal = queue.add(file);
				if (refusal) refused.push(`${file.name}: ${refusal}`);
			}
			if (refused.length > 0) setNotice(refused.join("\n"));
		} catch (error) {
			setNotice(errorMessage(error, "Não foi possível anexar o arquivo selecionado."));
		}
	};

	return { items, queue, pick, notice, clearNotice: () => setNotice(null) };
}

export type Attachments = ReturnType<typeof useAttachments>;

const SOURCES: Array<{ source: PickSource; icon: keyof typeof Feather.glyphMap; label: string }> = [
	{ source: "library", icon: "image", label: "Fotos" },
	{ source: "camera", icon: "camera", label: "Tirar foto" },
	{ source: "document", icon: "file", label: "Arquivo" },
];

export function AttachButton({ attachments, disabled }: { attachments: Attachments; disabled?: boolean }) {
	const { color } = useTheme();
	const styles = useStyles();
	const [open, setOpen] = useState(false);
	const insets = useSafeAreaInsets();
	const full = attachments.queue.room() === 0;
	const choose = (source: PickSource) => {
		setOpen(false);
		// iOS won't present the picker while this sheet is still closing.
		setTimeout(() => void attachments.pick(source), 350);
	};
	return (
		<>
			<Pressable
				onPress={() => setOpen(true)}
				disabled={disabled || full}
				hitSlop={6}
				accessibilityRole="button"
				accessibilityLabel={t("composer.attachments.add")}
				style={[styles.tool, (disabled || full) && { opacity: 0.4 }]}
			>
				<Feather name="paperclip" size={17} color={color.mute} />
			</Pressable>
			<Modal visible={open} transparent animationType="fade" onRequestClose={() => setOpen(false)}>
				<Pressable style={styles.backdrop} onPress={() => setOpen(false)} accessibilityLabel="Cancelar" />
				<View style={[styles.sheet, { paddingBottom: insets.bottom + 12 }]}>
					<Mono style={styles.sheetTitle}>{t("composer.attachments.add")}</Mono>
					{SOURCES.map((option) => (
						<Pressable
							key={option.source}
							onPress={() => choose(option.source)}
							accessibilityRole="button"
							style={({ pressed }) => [styles.option, pressed && { backgroundColor: color.elevated }]}
						>
							<Feather name={option.icon} size={17} color={color.fg} />
							<Text style={styles.optionLabel}>{option.label}</Text>
						</Pressable>
					))}
					<Text style={styles.sheetHint}>
						Até {formatBytes(MAX_ATTACHMENT_BYTES)} por arquivo. Fotos são reduzidas antes de enviar.
					</Text>
				</View>
			</Modal>
		</>
	);
}

/** Thumbnails of what will go with the message; each can be removed before sending. */
export function AttachmentTray({ attachments }: { attachments: Attachments }) {
	const { color } = useTheme();
	const styles = useStyles();
	const { items, notice } = attachments;
	if (items.length === 0 && !notice) return null;
	return (
		<View style={{ gap: 6 }}>
			{items.length > 0 ? (
				<ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.tray} keyboardShouldPersistTaps="handled">
					{items.map((item) => (
						<View key={item.id} style={[styles.thumb, item.status === "failed" && { borderColor: color.danger }]}>
							{item.kind === "image" && item.previewUri ? (
								<Image source={{ uri: item.previewUri }} style={styles.thumbImage} accessibilityLabel={item.name} />
							) : (
								<View style={styles.fileThumb}>
									<Feather name="file-text" size={18} color={color.mute} />
									<Text style={styles.fileName} numberOfLines={2}>
										{item.name}
									</Text>
								</View>
							)}
							{item.status === "uploading" ? (
								<View style={styles.thumbOverlay}>
									<ActivityIndicator size="small" color={color.fg} />
								</View>
							) : null}
							{item.status === "failed" ? (
								<View style={[styles.thumbOverlay, { backgroundColor: alpha(color.danger, 0.35) }]}>
									<Feather name="alert-triangle" size={16} color={color.fg} />
								</View>
							) : null}
							<Pressable
								onPress={() => attachments.queue.remove(item.id)}
								hitSlop={8}
								accessibilityRole="button"
								accessibilityLabel={`Remover ${item.name}`}
								style={styles.remove}
							>
								<Feather name="x" size={11} color={color.fg} />
							</Pressable>
						</View>
					))}
				</ScrollView>
			) : null}
			{items
				.filter((item) => item.status === "failed")
				.map((item) => (
					<Text key={item.id} style={styles.error}>
						{item.name}: {item.error}
					</Text>
				))}
			{notice ? (
				<Pressable onPress={attachments.clearNotice}>
					<Text style={styles.error}>{notice}</Text>
				</Pressable>
			) : null}
		</View>
	);
}

/** Tap to dictate, tap again to stop. Never sends. */
export function MicButton({ dictation, disabled }: { dictation: Dictation; disabled?: boolean }) {
	const { color } = useTheme();
	const styles = useStyles();
	const { phase } = dictation.state;
	const live = phase === "listening";
	return (
		<Pressable
			onPress={dictation.toggle}
			disabled={disabled && !dictation.listening}
			hitSlop={6}
			accessibilityRole="button"
			accessibilityLabel={dictation.listening ? "Parar o ditado" : "Ditar mensagem"}
			accessibilityState={{ selected: dictation.listening }}
			style={[
				styles.tool,
				live && { backgroundColor: alpha(color.danger, 0.15), borderColor: alpha(color.danger, 0.6) },
				disabled && !dictation.listening && { opacity: 0.4 },
			]}
		>
			{phase === "preparing" || phase === "stopping" ? (
				<ActivityIndicator size="small" color={color.mute} />
			) : (
				<Feather name={live ? "mic-off" : "mic"} size={17} color={live ? color.danger : color.mute} />
			)}
		</Pressable>
	);
}

/** "Ouvindo…" while dictating; the error (and a way to the settings) after. */
export function DictationStatus({ dictation }: { dictation: Dictation }) {
	const styles = useStyles();
	const { phase, error, needsSettings, onDevice } = dictation.state;
	if (phase === "listening") {
		return (
			<View style={styles.status}>
				<View style={styles.recDot} />
				<Mono style={styles.statusText}>
					Ouvindo{onDevice ? " · no aparelho" : ""} — toque no microfone para parar
				</Mono>
			</View>
		);
	}
	if (!error) return null;
	return (
		<Pressable onPress={dictation.clearError} style={styles.status}>
			<Text style={styles.error}>
				{error}
				{needsSettings ? (
					<Text style={styles.link} onPress={() => void Linking.openSettings()}>
						{"  "}Abrir Ajustes
					</Text>
				) : null}
			</Text>
		</Pressable>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		tool: {
			width: 40,
			height: 40,
			borderRadius: 20,
			borderWidth: 1,
			borderColor: color.border,
			alignItems: "center",
			justifyContent: "center",
			backgroundColor: color.bg,
		},
		tray: { gap: 8, paddingTop: 2, paddingRight: 8 },
		thumb: {
			width: 64,
			height: 64,
			borderRadius: radius.sm,
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.bg,
			overflow: "hidden",
		},
		thumbImage: { width: "100%", height: "100%" },
		fileThumb: { flex: 1, alignItems: "center", justifyContent: "center", gap: 3, padding: 4 },
		fileName: { color: color.mute, fontSize: 9, textAlign: "center" },
		thumbOverlay: {
			...StyleSheet.absoluteFillObject,
			alignItems: "center",
			justifyContent: "center",
			backgroundColor: "rgba(0,0,0,0.45)",
		},
		remove: {
			position: "absolute",
			top: 3,
			right: 3,
			width: 18,
			height: 18,
			borderRadius: 9,
			alignItems: "center",
			justifyContent: "center",
			backgroundColor: "rgba(0,0,0,0.7)",
		},
		error: { color: color.danger, fontSize: 12, lineHeight: 17 },
		link: { color: color.info, fontWeight: "600" },
		status: { flexDirection: "row", alignItems: "center", gap: 8 },
		statusText: { fontSize: 11, color: color.mute, flexShrink: 1 },
		recDot: { width: 8, height: 8, borderRadius: 4, backgroundColor: color.danger },
		backdrop: { flex: 1, backgroundColor: "rgba(0,0,0,0.5)" },
		sheet: {
			backgroundColor: color.panel,
			borderTopLeftRadius: radius.lg,
			borderTopRightRadius: radius.lg,
			borderTopWidth: 1,
			borderColor: color.border,
			paddingHorizontal: 12,
			paddingTop: 16,
			gap: 4,
		},
		sheetTitle: { fontSize: 10, letterSpacing: 1.4, textTransform: "uppercase", color: color.mute, paddingHorizontal: 8, marginBottom: 6 },
		sheetHint: { color: color.faint, fontSize: 12, paddingHorizontal: 8, paddingTop: 6 },
		option: { flexDirection: "row", alignItems: "center", gap: 12, padding: 12, borderRadius: radius.md },
		optionLabel: { color: color.fg, fontSize: 15, fontWeight: "500" },
	}),
);
