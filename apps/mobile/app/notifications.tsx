/**
 * Shown once, after pairing: why the app would like to notify, and what an
 * notification carries, before the system prompt (which can only be shown
 * once). "Agora não" is remembered; Settings can turn it on later.
 */
import { useSession } from "@/backend/session";
import { color } from "@/theme";
import { Button, Mono, Panel } from "@/ui/primitives";
import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { useState } from "react";
import { Platform, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

export default function NotificationsOffer() {
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
				{ paddingTop: Platform.OS === "ios" ? 40 : insets.top + 24, paddingBottom: insets.bottom + 24 },
			]}
		>
			<View style={styles.icon}>
				<Feather name="bell" size={28} color={color.wait} />
			</View>
			<Text style={styles.title}>Avisar quando um agente precisar de você?</Text>
			<Text style={styles.body}>
				O computador manda uma notificação quando um agente pede permissão, faz uma pergunta ou termina uma tarefa —
				mesmo com o app fechado. Tocar nela abre a conversa.
			</Text>
			<Panel style={{ gap: 8, marginTop: 20 }}>
				<Mono style={styles.label}>O que aparece na notificação</Mono>
				<Text style={styles.example}>DCC · meu-projeto precisa de você</Text>
				<Text style={styles.hint}>
					Só isso: o nome do projeto e o tipo do aviso. O texto passa pela Expo e pela Apple/Google, então nunca leva a
					conversa, comandos ou arquivos — o resto chega cifrado quando você abre o app.
				</Text>
			</Panel>
			<Button label="Ativar notificações" style={{ marginTop: 24 }} busy={busy} onPress={accept} />
			<Button label="Agora não" variant="ghost" style={{ marginTop: 10 }} disabled={busy} onPress={decline} />
			<Text style={[styles.hint, { marginTop: 14, textAlign: "center" }]}>Dá para mudar depois em Configurações.</Text>
		</ScrollView>
	);
}

const styles = StyleSheet.create({
	root: { flex: 1, backgroundColor: color.bg },
	content: { paddingHorizontal: 24 },
	icon: {
		width: 56,
		height: 56,
		borderRadius: 28,
		alignItems: "center",
		justifyContent: "center",
		backgroundColor: color.panel,
		borderWidth: 1,
		borderColor: color.border,
		marginBottom: 20,
	},
	title: { color: color.fg, fontSize: 22, fontWeight: "700", lineHeight: 28 },
	body: { color: color.fg, fontSize: 15, lineHeight: 22, marginTop: 12 },
	label: { fontSize: 10, letterSpacing: 1.5, textTransform: "uppercase", color: color.mute },
	example: { color: color.fg, fontSize: 14, fontWeight: "600" },
	hint: { color: color.mute, fontSize: 12, lineHeight: 18 },
});
