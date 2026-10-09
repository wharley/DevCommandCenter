import { desktopFingerprint } from "@/backend/identity";
import { useConnectionState, useSession } from "@/backend/session";
import type { Backend } from "@/backend/types";
import { color } from "@/theme";
import { Button, Mono, Panel, StateDot } from "@/ui/primitives";
import { router } from "expo-router";
import { Alert, Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

export default function SettingsScreen() {
	const insets = useSafeAreaInsets();
	const { session, signOut } = useSession();
	if (session.status !== "remote" && session.status !== "demo") return null;

	const leave = () => {
		void signOut().then(() => router.replace("/pair"));
	};

	return (
		<ScrollView style={styles.root} contentContainerStyle={[styles.content, { paddingBottom: insets.bottom + 24 }]}>
			<View style={styles.header}>
				<Text style={styles.title}>Configurações</Text>
				<Pressable onPress={() => router.back()} hitSlop={10}>
					<Text style={styles.close}>Fechar</Text>
				</Pressable>
			</View>

			{session.status === "demo" ? (
				<Panel style={{ gap: 12 }}>
					<Text style={styles.body}>Você está no modo demonstração: projetos e agentes fictícios, sem rede.</Text>
					<Button label="Parear com meu computador" onPress={() => router.push("/pair")} />
					<Button label="Sair da demonstração" variant="ghost" onPress={leave} />
				</Panel>
			) : (
				<>
					<Connection backend={session.backend} />
					<Panel style={{ gap: 14, marginTop: 12 }}>
						<Field label="Computador" value={desktopFingerprint(session.pairing)} mono accent />
						<Field label="Este aparelho" value={session.deviceFingerprint} mono accent />
						<Field label="Id do aparelho" value={session.pairing.deviceId} mono />
						<Field label="Relay" value={session.pairing.relay} mono />
						<Field label="Pareado em" value={new Date(session.pairing.pairedAt).toLocaleString("pt-BR")} />
					</Panel>
					<Text style={styles.hint}>
						Os códigos acima são os mesmos que o computador mostra em Configurações → Dispositivos. Para tirar o
						acesso deste aparelho, revogue-o lá; esquecer aqui só apaga o pareamento deste celular.
					</Text>
					<Button
						label="Esquecer este computador"
						variant="danger"
						style={{ marginTop: 16 }}
						onPress={() =>
							Alert.alert("Esquecer este computador?", "Para voltar a usar, será preciso parear de novo com um QR novo.", [
								{ text: "Cancelar", style: "cancel" },
								{ text: "Esquecer", style: "destructive", onPress: leave },
							])
						}
					/>
				</>
			)}
		</ScrollView>
	);
}

function Connection({ backend }: { backend: Backend }) {
	const state = useConnectionState(backend);
	const label =
		state.kind === "online"
			? "Conectado pelo relay, cifrado de ponta a ponta"
			: state.kind === "connecting"
				? "Conectando…"
				: state.kind === "offline"
					? state.error
					: "O computador não reconhece mais este aparelho";
	return (
		<Panel style={styles.connection}>
			<StateDot state={state.kind === "online" ? "live" : state.kind === "unpaired" ? "fail" : "wait"} />
			<Text style={[styles.body, { flex: 1 }]}>{label}</Text>
			{state.kind === "offline" ? (
				<Pressable onPress={() => backend.reconnectNow()} hitSlop={8}>
					<Text style={styles.action}>Tentar</Text>
				</Pressable>
			) : null}
		</Panel>
	);
}

function Field({ label, value, mono, accent }: { label: string; value: string; mono?: boolean; accent?: boolean }) {
	return (
		<View style={{ gap: 3 }}>
			<Mono style={styles.label}>{label}</Mono>
			{mono ? (
				<Mono style={[styles.value, accent && { color: color.accent, fontSize: 16, letterSpacing: 1.5 }]} selectable>
					{value}
				</Mono>
			) : (
				<Text style={styles.value}>{value}</Text>
			)}
		</View>
	);
}

const styles = StyleSheet.create({
	root: { flex: 1, backgroundColor: color.bg },
	content: { paddingHorizontal: 20, paddingTop: 20 },
	header: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", marginBottom: 20 },
	title: { color: color.fg, fontSize: 20, fontWeight: "600" },
	close: { color: color.mute, fontSize: 14 },
	body: { color: color.fg, fontSize: 14, lineHeight: 20 },
	hint: { color: color.mute, fontSize: 12, lineHeight: 18, marginTop: 12 },
	connection: { flexDirection: "row", alignItems: "center", gap: 10 },
	action: { color: color.wait, fontWeight: "700", fontSize: 13 },
	label: { fontSize: 10, letterSpacing: 1.5, textTransform: "uppercase", color: color.mute },
	value: { color: color.fg, fontSize: 13 },
});
