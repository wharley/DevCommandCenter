/** One line under the header when the channel is not up, with a retry. */
import { useEffect, useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { router } from "expo-router";
import { useConnectionState, useSession } from "../backend/session";
import type { Backend } from "../backend/types";
import { alpha, makeStyles, useTheme } from "../theme";
import { Mono, StateDot } from "./primitives";

export function ConnectionBanner({ backend }: { backend: Backend }) {
	const { color } = useTheme();
	const styles = useStyles();
	const state = useConnectionState(backend);
	const { signOut } = useSession();
	const [now, setNow] = useState(Date.now());
	useEffect(() => {
		if (state.kind !== "offline") return;
		const timer = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(timer);
	}, [state.kind]);

	if (backend.mode === "demo") {
		return (
			<View style={[styles.bar, { borderColor: alpha(color.info, 0.35), backgroundColor: alpha(color.info, 0.06) }]}>
				<Mono style={[styles.label, { color: color.info }]}>demonstração</Mono>
				<Text style={styles.text} numberOfLines={1}>
					Dados fictícios, nada sai do celular.
				</Text>
				<Pressable onPress={() => router.push("/pair")} hitSlop={8}>
					<Text style={[styles.action, { color: color.info }]}>Parear</Text>
				</Pressable>
			</View>
		);
	}
	if (state.kind === "online") return null;
	if (state.kind === "unpaired") {
		return (
			<View style={[styles.bar, { borderColor: alpha(color.danger, 0.4), backgroundColor: alpha(color.danger, 0.06) }]}>
				<StateDot state="fail" />
				<Text style={styles.text} numberOfLines={2}>
					O computador não reconhece mais este aparelho.
				</Text>
				<Pressable onPress={() => void signOut()} hitSlop={8}>
					<Text style={[styles.action, { color: color.danger }]}>Parear de novo</Text>
				</Pressable>
			</View>
		);
	}
	const seconds = state.kind === "offline" ? Math.max(0, Math.ceil((state.retryAt - now) / 1000)) : 0;
	return (
		<View style={[styles.bar, { borderColor: alpha(color.waitFill, 0.35), backgroundColor: alpha(color.waitFill, 0.05) }]}>
			<StateDot state="wait" />
			<Text style={styles.text} numberOfLines={2}>
				{state.kind === "connecting"
					? "Conectando ao computador…"
					: `${state.error} Nova tentativa em ${seconds}s.`}
			</Text>
			{state.kind === "offline" ? (
				<Pressable onPress={() => backend.reconnectNow()} hitSlop={8}>
					<Text style={[styles.action, { color: color.wait }]}>Tentar</Text>
				</Pressable>
			) : null}
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		bar: {
			flexDirection: "row",
			alignItems: "center",
			gap: 10,
			borderWidth: 1,
			borderRadius: 12,
			paddingHorizontal: 12,
			paddingVertical: 9,
			marginBottom: 12,
		},
		label: { fontSize: 10, letterSpacing: 1.5, textTransform: "uppercase" },
		text: { flex: 1, color: color.fg, fontSize: 12, lineHeight: 16 },
		action: { fontSize: 12, fontWeight: "700" },
	}),
);
