import { HomeDataProvider } from "@/screens/homeData";
import { useTheme } from "@/theme";
import { sheetOptions } from "@/ui/surfaces";
import { Stack } from "expo-router";

/** The home and its project sheet share one data provider (scan, workspaces, catalog, scope). */
export default function HomeLayout() {
	const { color } = useTheme();
	return (
		<HomeDataProvider>
			<Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: color.bg } }}>
				<Stack.Screen name="index" />
				<Stack.Screen name="scope" options={sheetOptions(color)} />
			</Stack>
		</HomeDataProvider>
	);
}
