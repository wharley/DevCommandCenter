/**
 * A choice that opens from a chip (project, agent, access): a sheet from the
 * bottom over a scrim, local to the screen so the choice stays in its state.
 */
import type { ReactNode } from "react";
import { Modal, Pressable, ScrollView, StyleSheet, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { makeStyles, radius } from "../theme";
import { SheetHeader } from "./surfaces";

export function PickerSheet({
	visible,
	title,
	onClose,
	children,
}: {
	visible: boolean;
	title: string;
	onClose: () => void;
	children: ReactNode;
}) {
	const styles = useStyles();
	const insets = useSafeAreaInsets();
	return (
		<Modal visible={visible} transparent animationType="fade" onRequestClose={onClose}>
			<Pressable style={styles.scrim} onPress={onClose} accessibilityLabel="Fechar" />
			<View style={[styles.sheet, { paddingBottom: insets.bottom + 12 }]}>
				<View style={styles.grabber} />
				<SheetHeader title={title} onClose={onClose} />
				<ScrollView style={styles.body} contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled">
					{children}
				</ScrollView>
			</View>
		</Modal>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		scrim: { flex: 1, backgroundColor: color.scrim },
		sheet: {
			maxHeight: "78%",
			backgroundColor: color.bg,
			borderTopLeftRadius: radius.sheet,
			borderTopRightRadius: radius.sheet,
		},
		grabber: { alignSelf: "center", width: 38, height: 5, borderRadius: 999, backgroundColor: color.border, marginTop: 8 },
		body: { flexGrow: 0 },
		content: { paddingHorizontal: 16, gap: 12 },
	}),
);
