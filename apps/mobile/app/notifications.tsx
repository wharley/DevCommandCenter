/**
 * Shown once, after pairing: why the app would like to notify, and what an
 * notification carries, before the system prompt (which can only be shown
 * once). "Agora não" is remembered; Settings can turn it on later.
 */
import { useSession } from "@/backend/session";
import { makeStyles, radius, useTheme } from "@/theme";
import { Button, SectionLabel } from "@/ui/primitives";
import { AgentMascot } from "@/ui/sprites";
import { DitherPatch } from "@/ui/surfaces";
import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { useState } from "react";
import { Platform, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

export default function NotificationsOffer() {
	const { color } = useTheme();
	const styles = useStyles();
	const insets = useSafeAreaInsets();
	const { session } = useSession();
	const [busy, setBusy] = useState(false);
	if (session.status !== "remote") return null;
	const push = session.push;

	const close = () => (router.canGoBack() ? router.back() : router.replace("/"));
	const accept = () => {
		setBusy(true);
		void push.enable().finally(() => {
			setBusy(false);
			close();
		});
	};
	const decline = () => {
		void push.disable().finally(close);
	};

	return (
		<ScrollView
			style={styles.root}
			// An iOS sheet already starts below the status bar; Android modals are full screen.
			contentContainerStyle={[
				styles.content,
				{ paddingTop: Platform.OS === "ios" ? 36 : insets.top + 24, paddingBottom: insets.bottom + 20 },
			]}
		>
			<View style={styles.hero}>
				<DitherPatch tint={color.waitFill} size={140}>
					<AgentMascot provider={null} state="waiting" size={64} />
				</DitherPatch>
				<Text style={styles.title} accessibilityRole="header">
					Avisar quando um agente precisar de você?
				</Text>
				<Text style={styles.body}>
					O computador manda uma notificação quando um agente pede permissão, faz uma pergunta ou termina uma tarefa —
					mesmo com o app fechado. Tocar nela abre a conversa.
				</Text>
			</View>

			<View>
				<SectionLabel>O que aparece na notificação</SectionLabel>
				<View style={styles.preview} accessible accessibilityLabel="Exemplo: DCC, meu-projeto precisa de você">
					<View style={styles.appIcon}>
						<Feather name="terminal" size={17} color={color.accentInk} />
					</View>
					<View style={{ flex: 1, minWidth: 0, gap: 2 }}>
						<View style={styles.previewHead}>
							<Text style={styles.previewApp}>DCC</Text>
							<Text style={styles.previewWhen}>agora</Text>
						</View>
						<Text style={styles.previewText}>meu-projeto precisa de você</Text>
					</View>
				</View>
				<Text style={styles.footnote}>
					Só isso: o nome do projeto e o tipo do aviso. O texto passa pela Expo e pela Apple/Google, então nunca leva a
					conversa, comandos ou arquivos — o resto chega cifrado quando você abre o app.
				</Text>
			</View>

			<View style={styles.actions}>
				<Button label="Ativar notificações" style={styles.button} busy={busy} onPress={accept} />
				<Button label="Agora não" variant="soft" style={styles.button} disabled={busy} onPress={decline} />
				<Text style={styles.later}>Dá para mudar depois em Ajustes.</Text>
			</View>
		</ScrollView>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		content: { flexGrow: 1, paddingHorizontal: 16, gap: 26 },
		hero: { alignItems: "center", gap: 10, paddingHorizontal: 8 },
		title: { color: color.fg, fontSize: 26, lineHeight: 31, fontWeight: "700", letterSpacing: -0.6, textAlign: "center" },
		body: { color: color.mute, fontSize: 15, lineHeight: 22, textAlign: "center" },
		preview: {
			flexDirection: "row",
			alignItems: "center",
			gap: 12,
			padding: 14,
			borderRadius: radius.xl,
			backgroundColor: color.panel,
			shadowColor: color.shadow,
			shadowOpacity: 1,
			shadowRadius: 18,
			shadowOffset: { width: 0, height: 6 },
		},
		appIcon: { width: 38, height: 38, borderRadius: 10, alignItems: "center", justifyContent: "center", backgroundColor: color.accent },
		previewHead: { flexDirection: "row", alignItems: "baseline", justifyContent: "space-between", gap: 8 },
		previewApp: { color: color.fg, fontSize: 14, fontWeight: "600" },
		previewWhen: { color: color.mute, fontSize: 12 },
		previewText: { color: color.fg, fontSize: 14 },
		footnote: { color: color.mute, fontSize: 12, lineHeight: 17, marginTop: 10, paddingHorizontal: 4 },
		actions: { marginTop: "auto", gap: 10 },
		button: { minHeight: 52, borderRadius: 999 },
		later: { color: color.mute, fontSize: 12, textAlign: "center", marginTop: 4 },
	}),
);
