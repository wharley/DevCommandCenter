import { isPairingLink, onPairingLink, stashPairingLink, takePairingLink } from "@/backend/offerLink";
import { pairingErrorMessage, pairWithDesktop, readOffer, type PairingProgress } from "@/backend/pairing";
import { useSession } from "@/backend/session";
import { Button, Mono, Panel, StateDot, Wordmark } from "@/ui/primitives";
import { alpha, makeStyles, radius, useTheme } from "@/theme";
import { CameraView, useCameraPermissions } from "expo-camera";
import * as Clipboard from "expo-clipboard";
import * as Linking from "expo-linking";
import { router } from "expo-router";
import { useCallback, useEffect, useRef, useState } from "react";
import {
	ActivityIndicator,
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

	return (
		<KeyboardAwareScrollView
			style={styles.root}
			contentContainerStyle={[styles.content, { paddingTop: insets.top + 20, paddingBottom: insets.bottom + 24 }]}
			keyboardShouldPersistTaps="handled"
			bottomOffset={24}
		>
			<View style={styles.header}>
				<Wordmark />
				{session.status === "demo" || session.status === "remote" ? (
					<Pressable onPress={() => router.back()} hitSlop={10}>
						<Text style={styles.close}>Fechar</Text>
					</Pressable>
				) : null}
			</View>

			{phase.kind === "scan" ? (
				<Scanner onLink={(url) => void pair(url)} onCancel={() => setPhase({ kind: "choose" })} />
			) : phase.kind === "working" ? (
				<Working progress={phase.progress} onCancel={cancel} />
			) : (
				<>
					<Text style={styles.title}>Conecte seu computador</Text>
					<Text style={styles.body}>
						No DCC do computador, abra Configurações → Dispositivos → Celular de qualquer rede e
						escaneie o QR. A conexão é cifrada de ponta a ponta: o relay só repassa bytes.
					</Text>

					{phase.kind === "error" ? (
						<Panel tone="danger" style={{ marginTop: 16 }}>
							<Text style={styles.error}>{phase.message}</Text>
						</Panel>
					) : null}

					<Button label="Escanear QR" onPress={() => setPhase({ kind: "scan" })} style={{ marginTop: 20 }} />

					<Mono style={styles.eyebrow}>ou cole o link</Mono>
					<TextInput
						value={link}
						onChangeText={setLink}
						placeholder="dcc://pair#v=1&…"
						placeholderTextColor={color.faint}
						autoCapitalize="none"
						autoCorrect={false}
						style={styles.input}
						accessibilityLabel="Link de pareamento"
					/>
					<View style={styles.row}>
						<Button
							label="Colar"
							variant="ghost"
							style={{ flex: 1 }}
							onPress={() => {
								void Clipboard.getStringAsync().then((text) => setLink(text.trim()));
							}}
						/>
						<Button
							label="Parear"
							variant="ghost"
							style={{ flex: 1 }}
							disabled={!link.trim()}
							onPress={() => void pair(link)}
						/>
					</View>

					{session.status !== "demo" ? (
						<Panel style={{ marginTop: 28 }}>
							<Text style={styles.demoTitle}>Sem computador por perto?</Text>
							<Text style={styles.body}>
								Explore o app com projetos e agentes fictícios. Nada sai do celular.
							</Text>
							<Button
								label="Explorar demonstração"
								variant="ghost"
								style={{ marginTop: 12 }}
								onPress={() => {
									void startDemo().then(() => router.replace("/"));
								}}
							/>
						</Panel>
					) : null}
				</>
			)}
		</KeyboardAwareScrollView>
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
			<Panel style={{ marginTop: 20 }}>
				<Text style={styles.body}>
					Sem acesso à câmera. Libere em Ajustes → DCC → Câmera, ou cole o link de pareamento.
				</Text>
				<Button label="Voltar" variant="ghost" style={{ marginTop: 12 }} onPress={onCancel} />
			</Panel>
		);
	}
	return (
		<View style={{ marginTop: 20, gap: 14 }}>
			<View style={styles.camera}>
				<CameraView
					style={StyleSheet.absoluteFill}
					facing="back"
					barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
					onBarcodeScanned={({ data }) => {
						if (handled.current || !isPairingLink(data)) return;
						handled.current = true;
						onLink(data);
					}}
				/>
				<View style={styles.reticle} pointerEvents="none" />
			</View>
			<Text style={styles.body}>Aponte para o QR na tela do computador.</Text>
			<Button label="Cancelar" variant="ghost" onPress={onCancel} />
		</View>
	);
}

function Working({ progress, onCancel }: { progress: PairingProgress | null; onCancel: () => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	const pending = progress?.step === "pending";
	return (
		<View style={{ marginTop: 24, gap: 16 }}>
			<View style={styles.row}>
				<StateDot state={pending ? "wait" : "live"} />
				<Text style={styles.title}>{pending ? "Aguardando aprovação" : "Conectando…"}</Text>
			</View>
			<Text style={styles.body}>
				{pending
					? "O computador está perguntando se este aparelho pode entrar. Confira os códigos e aprove lá."
					: "Abrindo um canal cifrado com o computador pelo relay."}
			</Text>
			{progress ? (
				<Panel style={{ gap: 12 }}>
					<Fingerprint label="Computador (ao lado do QR)" value={progress.desktopFingerprint} />
					{progress.step === "pending" ? (
						<Fingerprint label="Este aparelho (no diálogo de aprovação)" value={progress.deviceFingerprint} highlight />
					) : null}
				</Panel>
			) : (
				<ActivityIndicator color={color.mute} />
			)}
			<Button label="Cancelar" variant="ghost" onPress={onCancel} />
		</View>
	);
}

function Fingerprint({ label, value, highlight }: { label: string; value: string; highlight?: boolean }) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<View style={{ gap: 4 }}>
			<Mono style={styles.eyebrowInline}>{label}</Mono>
			<Mono style={[styles.fingerprint, highlight && { color: color.wait }]} accessibilityLabel={`Código ${value}`}>
				{value}
			</Mono>
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		content: { paddingHorizontal: 20 },
		header: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", marginBottom: 28 },
		close: { color: color.mute, fontSize: 14 },
		title: { color: color.fg, fontSize: 20, fontWeight: "600" },
		body: { color: color.mute, fontSize: 14, lineHeight: 20, marginTop: 6 },
		error: { color: color.danger, fontSize: 13, lineHeight: 18 },
		eyebrow: { marginTop: 24, marginBottom: 8, fontSize: 10, letterSpacing: 1.8, color: color.mute, textTransform: "uppercase" },
		eyebrowInline: { fontSize: 10, letterSpacing: 1.5, color: color.mute, textTransform: "uppercase" },
		input: {
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.panel,
			borderRadius: radius.md,
			color: color.fg,
			padding: 12,
			fontSize: 14,
		},
		row: { flexDirection: "row", alignItems: "center", gap: 10, marginTop: 10 },
		demoTitle: { color: color.fg, fontSize: 15, fontWeight: "600" },
		camera: { height: 320, borderRadius: radius.lg, overflow: "hidden", backgroundColor: color.panel },
		reticle: {
			position: "absolute",
			top: 60,
			bottom: 60,
			left: 60,
			right: 60,
			borderWidth: 2,
			borderColor: alpha(color.accent, 0.8),
			borderRadius: radius.lg,
		},
		fingerprint: { fontSize: 22, letterSpacing: 2, color: color.accent },
	}),
);
