import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { useBackend, useConnectionState } from "../backend/session";
import { makeStyles, radius, useTheme } from "../theme";
import { ConnectionBanner } from "../ui/ConnectionBanner";
import { Panel, Wordmark } from "../ui/primitives";
import { useHomeData } from "./homeData";
import { openNewTask } from "./rows";

/** Wordmark, settings, connection state, errors and the search field. */
export function HomeHeader({
	search,
	onSearch,
	placeholder,
}: {
	search: string;
	onSearch: (value: string) => void;
	placeholder: string;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	const backend = useBackend();
	const state = useConnectionState(backend);
	const { error, notice, dismissNotice } = useHomeData();
	return (
		<View>
			<View style={styles.top}>
				<Wordmark online={state.kind === "online"} />
				<View style={styles.actions}>
					<Pressable
						onPress={() => openNewTask()}
						hitSlop={6}
						accessibilityRole="button"
						accessibilityLabel="Nova tarefa"
						style={({ pressed }) => [styles.newTask, pressed && { opacity: 0.8 }]}
					>
						<Feather name="plus" size={15} color={color.accentInk} />
						<Text style={styles.newTaskText}>Nova tarefa</Text>
					</Pressable>
					<Pressable
						onPress={() => router.push("/settings")}
						hitSlop={10}
						accessibilityRole="button"
						accessibilityLabel="Configurações"
						style={styles.icon}
					>
						<Feather name="settings" size={18} color={color.mute} />
					</Pressable>
				</View>
			</View>
			<ConnectionBanner backend={backend} />
			{error ? (
				<Panel tone="danger" style={styles.message}>
					<Text style={styles.errorText}>{error}</Text>
				</Panel>
			) : null}
			{notice ? (
				<Pressable onPress={dismissNotice}>
					<Panel style={styles.message}>
						<Text style={styles.noticeText}>{notice}</Text>
					</Panel>
				</Pressable>
			) : null}
			<View style={styles.search}>
				<Feather name="search" size={14} color={color.faint} />
				<TextInput
					value={search}
					onChangeText={onSearch}
					placeholder={placeholder}
					placeholderTextColor={color.faint}
					style={styles.searchInput}
					autoCorrect={false}
					clearButtonMode="while-editing"
					accessibilityLabel={placeholder}
				/>
			</View>
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		top: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", paddingBottom: 16 },
		icon: { padding: 6 },
		actions: { flexDirection: "row", alignItems: "center", gap: 10 },
		newTask: {
			flexDirection: "row",
			alignItems: "center",
			gap: 4,
			backgroundColor: color.accent,
			borderRadius: 999,
			paddingHorizontal: 12,
			paddingVertical: 6,
		},
		newTaskText: { color: color.accentInk, fontSize: 13, fontWeight: "600" },
		message: { marginBottom: 12, paddingVertical: 10 },
		errorText: { color: color.danger, fontSize: 12, lineHeight: 17 },
		noticeText: { color: color.mute, fontSize: 12, lineHeight: 17 },
		search: {
			flexDirection: "row",
			alignItems: "center",
			gap: 8,
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.panel,
			borderRadius: radius.md,
			paddingHorizontal: 12,
			marginBottom: 18,
		},
		searchInput: { flex: 1, color: color.fg, fontSize: 14, paddingVertical: 9 },
	}),
);
