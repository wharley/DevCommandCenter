import { desktopFingerprint } from "@/backend/identity";
import { useConnectionState, useSession } from "@/backend/session";
import type { Backend } from "@/backend/types";
import type { PushRegistration } from "@/notifications/registration";
import { pushStatusText, usePushStatus } from "@/notifications/usePush";
import { alpha, makeStyles, useTheme } from "@/theme";
import { appVersionLine } from "@/updates";
import { haptic } from "@/ui/haptics";
import { Mono, SectionLabel, StateDot } from "@/ui/primitives";
import { GroupedList, Row, SheetHeader, SheetScroll } from "@/ui/surfaces";
import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { useState, type ReactNode } from "react";
import Constants from "expo-constants";
import { Alert, Platform, Pressable, StyleSheet, Switch, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

export default function SettingsScreen() {
	const { color } = useTheme();
	const styles = useStyles();
	const insets = useSafeAreaInsets();
	const { session, signOut } = useSession();
	if (session.status !== "remote" && session.status !== "demo") return null;

	const leave = () => {
		void signOut().then(() => router.replace("/pair"));
	};

	return (
		<SheetScroll
			// An iOS sheet already starts below the status bar; Android modals are full screen.
			style={{ marginTop: Platform.OS === "ios" ? 0 : insets.top }}
			contentStyle={[styles.content, { paddingBottom: insets.bottom }]}
			header={<SheetHeader title="Ajustes" onClose={() => router.back()} />}
		>
			{session.status === "demo" ? (
				<View>
					<SectionLabel>Modo demonstração</SectionLabel>
					<GroupedList inset={56}>
						<Row
							leading={<Tile icon="smartphone" tint={color.accent} />}
							title="Parear com meu computador"
							chevron
							onPress={() => router.push("/pair")}
						/>
						<Row
							leading={<Tile icon="log-out" tint={color.danger} />}
							title="Sair da demonstração"
							tone="danger"
							onPress={() => {
								haptic("warning");
								leave();
							}}
						/>
					</GroupedList>
					<Text style={styles.footnote}>Você está no modo demonstração: projetos e agentes fictícios, sem rede.</Text>
				</View>
			) : (
				<>
					<Connection backend={session.backend} />
					<Notifications push={session.push} />
					<View>
						<SectionLabel>Pareamento</SectionLabel>
						<GroupedList>
							<Field label="Computador" value={desktopFingerprint(session.pairing)} mono accent />
							<Field label="Este aparelho" value={session.deviceFingerprint} mono accent />
							<Field label="Id do aparelho" value={session.pairing.deviceId} mono stacked />
							<Field label="Relay" value={session.pairing.relay} mono stacked />
							<Field label="Pareado em" value={new Date(session.pairing.pairedAt).toLocaleString("pt-BR")} />
						</GroupedList>
						<Text style={styles.footnote}>
							Os códigos acima são os mesmos que o computador mostra em Configurações → Dispositivos. Para tirar o
							acesso deste aparelho, revogue-o lá; esquecer aqui só apaga o pareamento deste celular.
						</Text>
					</View>
					<GroupedList inset={56}>
						<Row
							leading={<Tile icon="trash-2" tint={color.danger} />}
							title="Esquecer este computador"
							tone="danger"
							onPress={() => {
								haptic("warning");
								Alert.alert("Esquecer este computador?", "Para voltar a usar, será preciso parear de novo com um QR novo.", [
									{ text: "Cancelar", style: "cancel" },
									{ text: "Esquecer", style: "destructive", onPress: leave },
								]);
							}}
						/>
					</GroupedList>
				</>
			)}
			{__DEV__ ? (
				<GroupedList inset={56}>
					<Row
						leading={<Tile icon="grid" tint={color.mute} />}
						title="Galeria de componentes"
						chevron
						onPress={() => router.push("/ui-gallery")}
					/>
				</GroupedList>
			) : null}
			<Mono style={styles.version}>DCC {appVersionLine(Constants.expoConfig?.version)}</Mono>
		</SheetScroll>
	);
}

/** The leading glyph of an action row: an icon on a faint tile of its tint. */
function Tile({ icon, tint }: { icon: keyof typeof Feather.glyphMap; tint: string }) {
	const styles = useStyles();
	return (
		<View style={[styles.tile, { backgroundColor: alpha(tint, 0.12) }]}>
			<Feather name={icon} size={15} color={tint} />
		</View>
	);
}

function Connection({ backend }: { backend: Backend }) {
	const { color } = useTheme();
	const styles = useStyles();
	const state = useConnectionState(backend);
	const title =
		state.kind === "online"
			? "Conectado"
			: state.kind === "connecting"
				? "Conectando…"
				: state.kind === "offline"
					? "Sem conexão"
					: "Acesso revogado";
	const detail =
		state.kind === "online"
			? "Pelo relay, cifrado de ponta a ponta."
			: state.kind === "connecting"
				? "Abrindo o canal cifrado com o computador."
				: state.kind === "offline"
					? state.error
					: "O computador não reconhece mais este aparelho.";
	return (
		<View>
			<SectionLabel>Conexão</SectionLabel>
			<GroupedList>
				<Row
					leading={
						<View style={styles.dotSlot}>
							<StateDot state={state.kind === "online" ? "live" : state.kind === "unpaired" ? "fail" : "wait"} size={9} />
						</View>
					}
					title={title}
					detail={<Text style={styles.rowDetail}>{detail}</Text>}
					trailing={
						state.kind === "offline" ? (
							<Pressable
								onPress={() => {
									haptic("tap");
									backend.reconnectNow();
								}}
								hitSlop={8}
								accessibilityRole="button"
								style={({ pressed }) => [styles.chipButton, pressed && { opacity: 0.75 }]}
							>
								<Feather name="refresh-cw" size={13} color={color.fg} />
								<Text style={styles.chipButtonText}>Tentar</Text>
							</Pressable>
						) : null
					}
				/>
			</GroupedList>
		</View>
	);
}

function Notifications({ push }: { push: PushRegistration }) {
	const { color } = useTheme();
	const styles = useStyles();
	const status = usePushStatus(push);
	const [busy, setBusy] = useState(false);
	const on = status.kind === "on" || status.kind === "pending";
	const toggle = (value: boolean) => {
		haptic("select");
		setBusy(true);
		void (value ? push.enable() : push.disable()).finally(() => setBusy(false));
	};
	return (
		<View>
			<SectionLabel>Notificações</SectionLabel>
			<GroupedList>
				<Row
					leading={
						<View style={styles.dotSlot}>
							<Feather name="bell" size={17} color={on ? color.wait : color.mute} />
						</View>
					}
					title="Avisos deste aparelho"
					detail={<Text style={styles.rowDetail}>{pushStatusText(status)}</Text>}
					trailing={
						<Switch
							accessibilityLabel="Notificações deste aparelho"
							value={on}
							disabled={busy || status.kind === "unsupported" || status.kind === "unavailable"}
							onValueChange={toggle}
							trackColor={{ true: color.accent, false: color.border }}
						/>
					}
				/>
			</GroupedList>
			{on ? (
				<Text style={styles.footnote}>
					Só o nome do projeto e o tipo do aviso passam pela Expo e pela Apple/Google; o resto chega cifrado ao abrir.
				</Text>
			) : null}
		</View>
	);
}

/** A label and its value in a grouped list: side by side, or stacked when the value runs long. */
function Field({
	label,
	value,
	mono,
	accent,
	stacked,
}: {
	label: string;
	value: string;
	mono?: boolean;
	accent?: boolean;
	stacked?: boolean;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	let shown: ReactNode = (
		<Text style={styles.value} selectable>
			{value}
		</Text>
	);
	if (mono) {
		shown = (
			<Mono style={[styles.value, styles.monoValue, accent && { color: color.accent, fontSize: 15, letterSpacing: 1.5 }]} selectable>
				{value}
			</Mono>
		);
	}
	return (
		<View style={[styles.field, stacked && styles.fieldStacked]} accessible accessibilityLabel={`${label}: ${value}`}>
			<Text style={styles.label}>{label}</Text>
			{shown}
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		content: { paddingHorizontal: 16, gap: 26 },
		version: { color: color.faint, fontSize: 11, textAlign: "center" },
		rowDetail: { color: color.mute, fontSize: 13, lineHeight: 18 },
		footnote: { color: color.mute, fontSize: 12, lineHeight: 17, marginTop: 8, paddingHorizontal: 4 },
		dotSlot: { width: 30, alignItems: "center" },
		tile: { width: 30, height: 30, borderRadius: 9, alignItems: "center", justifyContent: "center" },
		chipButton: {
			flexDirection: "row",
			alignItems: "center",
			gap: 6,
			height: 32,
			paddingHorizontal: 12,
			borderRadius: 999,
			backgroundColor: color.chip,
		},
		chipButtonText: { color: color.fg, fontSize: 13, fontWeight: "600" },
		field: {
			flexDirection: "row",
			alignItems: "center",
			justifyContent: "space-between",
			gap: 12,
			paddingHorizontal: 14,
			paddingVertical: 12,
			minHeight: 50,
		},
		fieldStacked: { flexDirection: "column", alignItems: "flex-start", justifyContent: "center", gap: 3 },
		label: { color: color.fg, fontSize: 15 },
		value: { color: color.mute, fontSize: 14, flexShrink: 1 },
		monoValue: { fontSize: 12 },
	}),
);
