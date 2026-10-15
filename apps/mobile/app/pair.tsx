import { isPairingLink, onPairingLink, stashPairingLink, takePairingLink } from "@/backend/offerLink";
import { pairingErrorMessage, pairWithDesktop, readOffer, type PairingProgress } from "@/backend/pairing";
import { useSession } from "@/backend/session";
import { alpha, font, makeStyles, radius, useTheme } from "@/theme";
import { GlassButton } from "@/ui/glass";
import { haptic } from "@/ui/haptics";
import { Button, Mono, Panel, SectionLabel } from "@/ui/primitives";
import { AgentMascot } from "@/ui/sprites";
import { DitherHero, GroupedList, Row } from "@/ui/surfaces";
import Feather from "@expo/vector-icons/Feather";
import { CameraView, useCameraPermissions } from "expo-camera";
import * as Clipboard from "expo-clipboard";
import * as Linking from "expo-linking";
import { router } from "expo-router";
import { useCallback, useEffect, useRef, useState } from "react";
import {
	ActivityIndicator,
	Platform,
	Pressable,
	StyleSheet,
	Text,
	TextInput,
	View,
} from "react-native";
import { KeyboardAwareScrollView } from "react-native-keyboard-controller";
import { useSafeAreaInsets } from "react-native-safe-area-context";

type Phase =
	| { kind: "choose" }
	| { kind: "scan" }
	| { kind: "working"; progress: PairingProgress | null }
	| { kind: "error"; message: string };

export default function PairScreen() {
	const { color } = useTheme();
	const styles = useStyles();
	const insets = useSafeAreaInsets();
	const { session, startDemo, completePairing } = useSession();
	const [phase, setPhase] = useState<Phase>({ kind: "choose" });
	const [link, setLink] = useState("");
	const attempt = useRef(0);

	const pair = useCallback(
		async (raw: string) => {
			let offer;
			try {
				offer = readOffer(raw);
			} catch (error) {
				setPhase({ kind: "error", message: (error as Error).message });
				return;
			}
			const id = ++attempt.current;
			setPhase({ kind: "working", progress: null });
			try {
				const { pairing, client } = await pairWithDesktop(offer, (progress) => {
					if (attempt.current === id) setPhase({ kind: "working", progress });
				});
				if (attempt.current !== id) {
					client.close();
					return;
				}
				await completePairing(pairing, client);
				router.replace("/");
			} catch (error) {
				if (attempt.current === id) setPhase({ kind: "error", message: pairingErrorMessage(error) });
			}
		},
		[completePairing],
	);

	// A deep link that opened (or reached) the app. Everything goes through
	// the stash, which hands each link out once.
	useEffect(() => {
		const unsubscribe = onPairingLink(() => {
			const url = takePairingLink();
			if (url) void pair(url);
		});
		const stashed = takePairingLink();
		if (stashed) void pair(stashed);
		else
			void Linking.getInitialURL().then((url) => {
				if (url) stashPairingLink(url);
			});
		return unsubscribe;
	}, [pair]);

	const cancel = () => {
		attempt.current += 1;
		setPhase({ kind: "choose" });
	};

	// A sheet over the app once paired (iOS starts it below the status bar); the first screen otherwise.
	const ready = session.status === "demo" || session.status === "remote";
	const topInset = ready && Platform.OS === "ios" ? 8 : insets.top;

	return (
		<View style={styles.root}>
			<DitherHero tint={color.accent} height={420} />
			<KeyboardAwareScrollView
				style={{ flex: 1 }}
				contentContainerStyle={[styles.content, { paddingTop: topInset + 64, paddingBottom: insets.bottom + 24 }]}
				keyboardShouldPersistTaps="handled"
				bottomOffset={24}
			>
				{phase.kind === "scan" ? (
					<Scanner onLink={(url) => void pair(url)} onCancel={() => setPhase({ kind: "choose" })} />
				) : phase.kind === "working" ? (
					<Working progress={phase.progress} onCancel={cancel} />
				) : (
					<>
						<Hero
							state="idle"
							title="Conecte seu computador"
							body="No DCC do computador, abra Configurações → Dispositivos → Celular de qualquer rede e escaneie o QR. A conexão é cifrada de ponta a ponta: o relay só repassa bytes."
						/>

						{phase.kind === "error" ? (
							<Panel tone="danger">
								<Text style={styles.error}>{phase.message}</Text>
							</Panel>
						) : null}

						<Button label="Escanear QR" style={styles.primary} onPress={() => setPhase({ kind: "scan" })} />

						<View>
							<SectionLabel>ou cole o link</SectionLabel>
							<View style={styles.field}>
								<TextInput
									value={link}
									onChangeText={setLink}
									placeholder="dcc://pair#v=1&…"
									placeholderTextColor={color.faint}
									autoCapitalize="none"
									autoCorrect={false}
									returnKeyType="go"
									onSubmitEditing={() => link.trim() && void pair(link)}
									style={styles.input}
									accessibilityLabel="Link de pareamento"
								/>
								<RoundButton
									icon="clipboard"
									accessibilityLabel="Colar"
									onPress={() => {
										void Clipboard.getStringAsync().then((text) => setLink(text.trim()));
									}}
								/>
								<RoundButton icon="arrow-right" accessibilityLabel="Parear" primary disabled={!link.trim()} onPress={() => void pair(link)} />
							</View>
						</View>

						{session.status !== "demo" ? (
							<View>
								<SectionLabel>Sem computador por perto?</SectionLabel>
								<GroupedList>
									<Row
										leading={
											<View style={styles.tile}>
												<Feather name="compass" size={16} color={color.accent} />
											</View>
										}
										title="Explorar demonstração"
										detail={<Text style={styles.rowDetail}>Projetos e agentes fictícios. Nada sai do celular.</Text>}
										chevron
										onPress={() => {
											haptic("tap");
											void startDemo().then(() => router.replace("/"));
										}}
									/>
								</GroupedList>
							</View>
						) : null}
					</>
				)}
			</KeyboardAwareScrollView>

			{ready ? (
				<View style={[styles.topBar, { top: topInset + 8 }]}>
					<GlassButton icon="x" accessibilityLabel="Fechar" onPress={() => router.back()} />
				</View>
			) : null}
		</View>
	);
}

/** The robot over the dither, a title and a line of help. */
function Hero({ state, title, body }: { state: "idle" | "working" | "waiting"; title: string; body: string }) {
	const styles = useStyles();
	return (
		<View style={styles.hero}>
			<AgentMascot provider={null} state={state} size={72} />
			<Text style={styles.title} accessibilityRole="header">
				{title}
			</Text>
			<Text style={styles.body}>{body}</Text>
		</View>
	);
}

/** A 44pt round action beside the link field. */
function RoundButton({
	icon,
	onPress,
	accessibilityLabel,
	primary,
	disabled,
}: {
	icon: keyof typeof Feather.glyphMap;
	onPress: () => void;
	accessibilityLabel: string;
	primary?: boolean;
	disabled?: boolean;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<Pressable
			onPress={() => {
				haptic("tap");
				onPress();
			}}
			disabled={disabled}
			hitSlop={4}
			accessibilityRole="button"
			accessibilityLabel={accessibilityLabel}
			accessibilityState={{ disabled: !!disabled }}
			style={({ pressed }) => [
				styles.round,
				{ backgroundColor: primary ? color.accent : color.chip, opacity: disabled ? 0.4 : pressed ? 0.75 : 1 },
			]}
		>
			<Feather name={icon} size={17} color={primary ? color.accentInk : color.fg} />
		</Pressable>
	);
}

function Scanner({ onLink, onCancel }: { onLink: (url: string) => void; onCancel: () => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	const [permission, requestPermission] = useCameraPermissions();
	const handled = useRef(false);

	useEffect(() => {
		if (permission && !permission.granted && permission.canAskAgain) void requestPermission();
	}, [permission, requestPermission]);

	if (!permission) return <ActivityIndicator color={color.mute} style={{ marginTop: 40 }} />;
	if (!permission.granted) {
		return (
			<>
				<Hero state="idle" title="Sem acesso à câmera" body="Libere em Ajustes → DCC → Câmera, ou cole o link de pareamento." />
				<Button label="Voltar" variant="soft" style={styles.primary} onPress={onCancel} />
			</>
		);
	}
	return (
		<>
			<View style={styles.camera}>
				<CameraView
					style={StyleSheet.absoluteFill}
					facing="back"
					barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
					onBarcodeScanned={({ data }) => {
						if (handled.current || !isPairingLink(data)) return;
						handled.current = true;
						haptic("success");
						onLink(data);
					}}
				/>
				<View style={styles.reticle} pointerEvents="none" />
			</View>
			<Text style={styles.body}>Aponte para o QR na tela do computador.</Text>
			<Button label="Cancelar" variant="soft" style={styles.primary} onPress={onCancel} />
		</>
	);
}

function Working({ progress, onCancel }: { progress: PairingProgress | null; onCancel: () => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	const pending = progress?.step === "pending";
	return (
		<>
			<Hero
				state={pending ? "waiting" : "working"}
				title={pending ? "Aguardando aprovação" : "Conectando…"}
				body={
					pending
						? "O computador está perguntando se este aparelho pode entrar. Confira os códigos e aprove lá."
						: "Abrindo um canal cifrado com o computador pelo relay."
				}
			/>
			{progress ? (
				<GroupedList>
					<Fingerprint label="Computador" where="ao lado do QR" value={progress.desktopFingerprint} />
					{progress.step === "pending" ? (
						<Fingerprint label="Este aparelho" where="no diálogo de aprovação" value={progress.deviceFingerprint} highlight />
					) : null}
				</GroupedList>
			) : (
				<ActivityIndicator color={color.mute} />
			)}
			<Button label="Cancelar" variant="soft" style={styles.primary} onPress={onCancel} />
		</>
	);
}

function Fingerprint({ label, where, value, highlight }: { label: string; where: string; value: string; highlight?: boolean }) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<View style={styles.fingerprintRow}>
			<View style={{ flex: 1, minWidth: 0, gap: 2 }}>
				<Text style={styles.fingerprintLabel}>{label}</Text>
				<Text style={styles.rowDetail}>{where}</Text>
			</View>
			<Mono style={[styles.fingerprint, highlight && { color: color.wait }]} accessibilityLabel={`Código ${value}`} selectable>
				{value}
			</Mono>
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		content: { flexGrow: 1, paddingHorizontal: 16, gap: 22 },
		topBar: { position: "absolute", left: 16 },
		// The hero takes the slack, so the actions sit low, within reach of a thumb.
		hero: { flexGrow: 1, alignItems: "center", justifyContent: "center", gap: 10, paddingVertical: 12, paddingHorizontal: 12 },
		title: { color: color.fg, fontSize: 28, lineHeight: 33, fontWeight: "700", letterSpacing: -0.7, textAlign: "center", marginTop: 8 },
		body: { color: color.mute, fontSize: 15, lineHeight: 22, textAlign: "center" },
		error: { color: color.danger, fontSize: 13, lineHeight: 18 },
		primary: { minHeight: 52, borderRadius: 999 },
		field: {
			flexDirection: "row",
			alignItems: "center",
			gap: 8,
			paddingLeft: 14,
			paddingRight: 6,
			height: 56,
			borderRadius: radius.lg,
			backgroundColor: color.panel,
		},
		input: { flex: 1, color: color.fg, fontSize: 14, fontFamily: font.mono },
		round: { width: 44, height: 44, borderRadius: 999, alignItems: "center", justifyContent: "center" },
		tile: { width: 30, height: 30, borderRadius: 9, alignItems: "center", justifyContent: "center", backgroundColor: alpha(color.accent, 0.12) },
		rowDetail: { color: color.mute, fontSize: 13 },
		camera: { height: 340, borderRadius: radius.xl, overflow: "hidden", backgroundColor: color.panel },
		reticle: {
			position: "absolute",
			top: 70,
			bottom: 70,
			left: 70,
			right: 70,
			borderWidth: 2,
			borderColor: alpha(color.accent, 0.8),
			borderRadius: radius.lg,
		},
		fingerprintRow: { flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 14, paddingVertical: 12, minHeight: 56 },
		fingerprintLabel: { color: color.fg, fontSize: 15, fontWeight: "500" },
		fingerprint: { fontSize: 17, letterSpacing: 1.5, color: color.accent },
	}),
);
