import { SessionProvider, useSession } from "@/backend/session";
import { isPairingLink, stashPairingLink } from "@/backend/offerLink";
import { color } from "@/theme";
import * as Linking from "expo-linking";
import { router, Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect } from "react";
import { View } from "react-native";
import { SafeAreaProvider } from "react-native-safe-area-context";

export default function RootLayout() {
	return (
		<SafeAreaProvider>
			<SessionProvider>
				<StatusBar style="light" />
				<View style={{ flex: 1, backgroundColor: color.bg }}>
					<PairingLinks />
					<Routes />
				</View>
			</SessionProvider>
		</SafeAreaProvider>
	);
}

/** Links that arrive while the app is already open (native-intent covers launch). */
function PairingLinks() {
	useEffect(() => {
		const subscription = Linking.addEventListener("url", ({ url }) => {
			if (!isPairingLink(url)) return;
			stashPairingLink(url);
			router.navigate("/pair");
		});
		return () => subscription.remove();
	}, []);
	return null;
}

function Routes() {
	const { session } = useSession();
	if (session.status === "loading") return null;
	const ready = session.status === "remote" || session.status === "demo";
	return (
		<Stack
			screenOptions={{
				headerShown: false,
				contentStyle: { backgroundColor: color.bg },
				animation: "default",
			}}
		>
			<Stack.Protected guard={ready}>
				<Stack.Screen name="(tabs)" />
				<Stack.Screen name="thread/[id]" />
				<Stack.Screen name="settings" options={{ presentation: "modal" }} />
			</Stack.Protected>
			<Stack.Screen name="pair" options={{ presentation: ready ? "modal" : "card", gestureEnabled: ready }} />
		</Stack>
	);
}
