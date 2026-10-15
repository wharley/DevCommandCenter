/**
 * A Browser or computer-use request from an agent, answered from the phone.
 * The desktop shows the same request in its own dialog; whoever answers
 * first wins and the card goes away on both sides.
 */
import {
	agentRequestTitle,
	allowBlocker,
	BROWSER_HANDOFF_NOTICE,
	COMPUTER_CONTROL_WARNING,
	secondsLeft,
	type AgentRequest,
} from "@dcc/mobile-core";
import Feather from "@expo/vector-icons/Feather";
import { useState, type ReactNode } from "react";
import { Alert, Pressable, StyleSheet, Text, View } from "react-native";
import type { AgentRequestAnswer } from "../backend/agentRequests";
import { alpha, makeStyles, radius, useTheme } from "../theme";
import { Button, Mono, StateDot } from "./primitives";

export function AgentRequestCard({
	request,
	busy,
	onRespond,
	header,
}: {
	request: AgentRequest;
	busy: boolean;
	onRespond: (answer: AgentRequestAnswer) => void;
	/** The thread it belongs to, on the home screen. */
	header?: ReactNode;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	const [picked, setPicked] = useState<string[]>([]);
	const left = secondsLeft(request);
	const blocker = allowBlocker(request, picked);
	const computer = request.kind === "computerControl";

	const toggle = (bundleId: string) =>
		setPicked((prev) => (prev.includes(bundleId) ? prev.filter((id) => id !== bundleId) : [...prev, bundleId]));

	const allow = () => {
		if (!computer) return onRespond({ allow: true });
		const names = request.apps.filter((app) => picked.includes(app.bundleId)).map((app) => app.name);
		Alert.alert(
			"Permitir o controle do Mac?",
			`${COMPUTER_CONTROL_WARNING}\n\nApps: ${names.join(", ")}.`,
			[
				{ text: "Cancelar", style: "cancel" },
				{ text: "Permitir", style: "destructive", onPress: () => onRespond({ allow: true, allowedBundleIds: picked }) },
			],
		);
	};

	return (
		<View style={styles.card} accessibilityLabel={agentRequestTitle(request)}>
			{header}
			<View style={styles.head}>
				<StateDot state="wait" />
				<Mono style={styles.eyebrow}>{agentRequestTitle(request).toLowerCase()}</Mono>
				<Mono style={styles.timer}>{left > 0 ? `${left}s` : "expirou"}</Mono>
			</View>
			{request.kind === "browserOpen" ? (
				<Mono style={styles.url} selectable numberOfLines={3}>
					{request.url}
				</Mono>
			) : null}
			{request.reason ? (
				<Text style={styles.reason} selectable>
					{request.reason}
				</Text>
			) : null}
			{request.kind === "browserOpen" ? (
				<Notice icon="monitor">{BROWSER_HANDOFF_NOTICE}</Notice>
			) : (
				<>
					<Notice icon="alert-triangle" strong>
						{COMPUTER_CONTROL_WARNING}
					</Notice>
					{request.apps.length > 0 ? (
						<View style={styles.apps}>
							<Mono style={styles.appsLabel}>apps que o agente pode controlar</Mono>
							{request.apps.map((app) => {
								const on = picked.includes(app.bundleId);
								return (
									<Pressable
										key={app.bundleId}
										onPress={() => toggle(app.bundleId)}
										disabled={busy}
										style={styles.app}
										accessibilityRole="checkbox"
										accessibilityState={{ checked: on }}
										accessibilityLabel={app.name}
									>
										<Feather name={on ? "check-square" : "square"} size={16} color={on ? color.wait : color.mute} />
										<Text style={styles.appName}>{app.name}</Text>
									</Pressable>
								);
							})}
						</View>
					) : null}
				</>
			)}
			{blocker && left > 0 && !(computer && picked.length === 0 && !request.blocker) ? (
				<Text style={styles.blocker}>{blocker}</Text>
			) : null}
			<View style={styles.choices}>
				<Button label="Negar" variant="ghost" disabled={busy || left === 0} style={{ flex: 1 }} onPress={() => onRespond({ allow: false })} />
				<Button
					label="Permitir"
					variant="wait"
					busy={busy}
					disabled={busy || blocker !== null}
					style={{ flex: 1 }}
					onPress={allow}
				/>
			</View>
		</View>
	);
}

function Notice({ icon, strong, children }: { icon: "monitor" | "alert-triangle"; strong?: boolean; children: ReactNode }) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<View style={[styles.notice, strong && styles.noticeStrong]}>
			<Feather name={icon} size={14} color={strong ? color.wait : color.mute} style={{ marginTop: 1 }} />
			<Text style={[styles.noticeText, strong && { color: color.fg }]}>{children}</Text>
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		card: {
			borderWidth: 1,
			borderColor: alpha(color.waitFill, 0.4),
			backgroundColor: alpha(color.waitFill, 0.05),
			borderRadius: radius.lg,
			padding: 14,
			gap: 10,
		},
		head: { flexDirection: "row", alignItems: "center", gap: 8 },
		eyebrow: { fontSize: 10, letterSpacing: 0.8, color: color.wait, textTransform: "uppercase", flex: 1 },
		timer: { fontSize: 10, color: color.mute },
		url: { color: color.fg, fontSize: 12 },
		reason: { color: color.fg, fontSize: 13, lineHeight: 19 },
		notice: { flexDirection: "row", gap: 8, padding: 10, borderRadius: radius.sm, backgroundColor: color.bg, borderWidth: 1, borderColor: color.border },
		noticeStrong: { borderColor: alpha(color.waitFill, 0.5) },
		noticeText: { flex: 1, color: color.mute, fontSize: 12, lineHeight: 17 },
		apps: { gap: 2 },
		appsLabel: { fontSize: 10, color: color.faint, marginBottom: 4 },
		app: { flexDirection: "row", alignItems: "center", gap: 10, paddingVertical: 7 },
		appName: { color: color.fg, fontSize: 14 },
		blocker: { color: color.mute, fontSize: 12 },
		choices: { flexDirection: "row", gap: 8 },
	}),
);
