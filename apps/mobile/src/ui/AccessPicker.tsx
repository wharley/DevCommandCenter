/**
 * The access level for the agent, as the desktop composer offers it
 * (`ComposerApprovalPolicyMenu`): only the levels the agent enforces, the same
 * labels, and a confirmation before "Acesso completo".
 */
import { accessDescription, accessLabel, t, type AccessPolicy } from "@dcc/mobile-core";
import Feather from "@expo/vector-icons/Feather";
import { useState } from "react";
import { Alert, Modal, Pressable, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { alpha, makeStyles, radius, useTheme, type Palette } from "../theme";
import { Mono } from "./primitives";

type Props = {
	available: readonly AccessPolicy[];
	value: AccessPolicy | null;
	onChange: (policy: AccessPolicy) => void;
	/** Shown when the agent manages access itself. */
	providerLabel?: string | null;
	disabled?: boolean;
};

export function accessTint(policy: AccessPolicy | null, color: Palette): string {
	if (policy === "full_access") return color.wait;
	if (policy === "auto") return color.accent;
	return color.mute;
}

export function PolicyIcon({ policy, size = 13 }: { policy: AccessPolicy | null; size?: number }) {
	const { color } = useTheme();
	const name = policy === "full_access" ? "alert-triangle" : "shield";
	return <Feather name={name} size={size} color={accessTint(policy, color)} />;
}

/** "Acesso completo" asks first, like the desktop's dialog. */
export function confirmThen(policy: AccessPolicy, current: AccessPolicy | null, apply: (policy: AccessPolicy) => void) {
	if (policy !== "full_access" || current === "full_access") {
		apply(policy);
		return;
	}
	Alert.alert(
		t("composer.approval.confirm.title"),
		`${t("composer.approval.confirm.description")}\n\n${t("composer.approval.confirm.worktreeWarning")}`,
		[
			{ text: t("composer.approval.confirm.cancel"), style: "cancel" },
			{ text: t("composer.approval.confirm.enable"), style: "destructive", onPress: () => apply(policy) },
		],
	);
}

function Managed({ providerLabel }: { providerLabel?: string | null }) {
	const styles = useStyles();
	return (
		<View style={styles.managed} accessibilityLabel={t("composer.approval.managedHint", { provider: providerLabel ?? "agente" })}>
			<PolicyIcon policy={null} />
			<Mono style={styles.managedText}>{t("composer.approval.managed")}</Mono>
		</View>
	);
}

/** "Nova tarefa": the levels side by side, the chosen one explained below. */
export function AccessChoices({ available, value, onChange, providerLabel, disabled }: Props) {
	const { color } = useTheme();
	const styles = useStyles();
	if (available.length === 0 || !value) {
		return (
			<View style={{ gap: 6 }}>
				<Managed providerLabel={providerLabel} />
				<Text style={styles.hint}>{t("composer.approval.managedHint", { provider: providerLabel ?? "agente" })}</Text>
			</View>
		);
	}
	return (
		<View style={{ gap: 8 }}>
			<View style={styles.chips}>
				{available.map((policy) => {
					const selected = policy === value;
					return (
						<Pressable
							key={policy}
							onPress={() => confirmThen(policy, value, onChange)}
							disabled={disabled}
							accessibilityRole="radio"
							accessibilityState={{ selected, disabled: !!disabled }}
							accessibilityLabel={accessLabel(policy)}
							style={({ pressed }) => [
								styles.chip,
								selected && { borderColor: accessTint(policy, color), backgroundColor: alpha(accessTint(policy, color), 0.08) },
								disabled && !selected && { opacity: 0.4 },
								pressed && { backgroundColor: color.elevated },
							]}
						>
							<PolicyIcon policy={policy} />
							<Text style={[styles.chipText, selected && { color: color.fg }]}>{accessLabel(policy)}</Text>
						</Pressable>
					);
				})}
			</View>
			<Text style={styles.hint}>{accessDescription(value)}</Text>
		</View>
	);
}

/** The levels with what each one allows, for a picker sheet. */
export function AccessList({
	available,
	value,
	onPick,
}: {
	available: readonly AccessPolicy[];
	value: AccessPolicy | null;
	onPick: (policy: AccessPolicy) => void;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<View>
			{available.map((policy) => (
				<Pressable
					key={policy}
					onPress={() => onPick(policy)}
					accessibilityRole="radio"
					accessibilityState={{ selected: policy === value }}
					style={({ pressed }) => [styles.option, pressed && { backgroundColor: color.elevated }]}
				>
					<View style={{ paddingTop: 2 }}>
						<PolicyIcon policy={policy} size={15} />
					</View>
					<View style={{ flex: 1, gap: 2 }}>
						<Text style={[styles.optionLabel, policy === "full_access" && { color: color.wait }]}>{accessLabel(policy)}</Text>
						<Text style={styles.optionDesc}>{accessDescription(policy)}</Text>
					</View>
					{policy === value ? <Feather name="check" size={16} color={color.accent} /> : null}
				</Pressable>
			))}
		</View>
	);
}

/** The thread's composer: a compact pill that opens the levels in a sheet. */
export function AccessPill({ available, value, onChange, providerLabel, disabled }: Props) {
	const { color } = useTheme();
	const styles = useStyles();
	const [open, setOpen] = useState(false);
	const insets = useSafeAreaInsets();
	if (available.length === 0 || !value) return <Managed providerLabel={providerLabel} />;
	const pick = (policy: AccessPolicy) => {
		setOpen(false);
		// iOS drops an alert raised while a modal is still closing.
		setTimeout(() => confirmThen(policy, value, onChange), policy === "full_access" ? 350 : 0);
	};
	return (
		<>
			<Pressable
				onPress={() => setOpen(true)}
				disabled={disabled}
				hitSlop={8}
				accessibilityRole="button"
				accessibilityLabel={`${t("composer.approval.open")}: ${accessLabel(value)}`}
				style={[
					styles.pill,
					value === "full_access" && { backgroundColor: alpha(color.waitFill, 0.1) },
					disabled && { opacity: 0.45 },
				]}
			>
				<PolicyIcon policy={value} size={12} />
				<Mono style={[styles.pillText, { color: accessTint(value, color) }]}>{accessLabel(value)}</Mono>
				<Feather name="chevron-down" size={12} color={color.faint} />
			</Pressable>
			<Modal visible={open} transparent animationType="fade" onRequestClose={() => setOpen(false)}>
				<Pressable style={styles.backdrop} onPress={() => setOpen(false)} accessibilityLabel={t("composer.approval.confirm.cancel")} />
				<View style={[styles.sheet, { paddingBottom: insets.bottom + 12 }]}>
					<Mono style={styles.sheetTitle}>{t("composer.approval.open")}</Mono>
					<AccessList available={available} value={value} onPick={pick} />
				</View>
			</Modal>
		</>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		chips: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
		chip: {
			flexDirection: "row",
			alignItems: "center",
			gap: 7,
			paddingHorizontal: 12,
			paddingVertical: 8,
			minHeight: 38,
			borderRadius: radius.md,
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.panel,
		},
		chipText: { color: color.mute, fontSize: 13, fontWeight: "500" },
		hint: { color: color.faint, fontSize: 12, lineHeight: 17 },
		managed: { flexDirection: "row", alignItems: "center", gap: 6, paddingVertical: 4 },
		managedText: { fontSize: 11, color: color.mute },
		pill: {
			flexDirection: "row",
			alignItems: "center",
			gap: 5,
			alignSelf: "flex-start",
			paddingHorizontal: 8,
			paddingVertical: 4,
			borderRadius: radius.sm,
		},
		pillText: { fontSize: 11, fontWeight: "600" },
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
		option: { flexDirection: "row", alignItems: "flex-start", gap: 12, padding: 10, borderRadius: radius.md },
		optionLabel: { color: color.fg, fontSize: 15, fontWeight: "600" },
		optionDesc: { color: color.mute, fontSize: 12, lineHeight: 17 },
	}),
);
