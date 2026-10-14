import { AgentRequestsProvider } from "@/backend/agentRequests";
import { TaskProposalsProvider } from "@/backend/taskProposals";
import { SessionProvider, useSession } from "@/backend/session";
import { isPairingLink, stashPairingLink } from "@/backend/offerLink";
import { configureNotificationDisplay, onNotificationTap } from "@/notifications/native";
import { sessionFromNotificationData } from "@/notifications/registration";
import { color } from "@/theme";
import { UpdateBanner } from "@/updates";
import * as Linking from "expo-linking";
import { router, Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect, useRef } from "react";
import { View } from "react-native";
import { KeyboardProvider } from "react-native-keyboard-controller";
import { SafeAreaProvider } from "react-native-safe-area-context";

configureNotificationDisplay();

export default function RootLayout() {
	return (
		<SafeAreaProvider>
			{/* SDK 55 draws Android edge-to-edge: the window no longer resizes for
			    the keyboard, so screens follow its frames from here (as Paseo,
			    Happy and t3code do). */}
			<KeyboardProvider>
				<SessionProvider>
					<StatusBar style="light" />
					<View style={{ flex: 1, backgroundColor: color.bg }}>
						<PairingLinks />
						<NotificationTaps />
						<OfferNotifications />
						<AgentRequestsProvider>
							<TaskProposalsProvider>
								<Routes />
							</TaskProposalsProvider>
						</AgentRequestsProvider>
						<UpdateBanner />
					</View>
				</SessionProvider>
			</KeyboardProvider>
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

/** A tapped notification opens its thread, once the desktop is reachable. */
function NotificationTaps() {
	const { session } = useSession();
	const ready = session.status === "remote";
	const pending = useRef<string | null>(null);
	const readyRef = useRef(ready);
	readyRef.current = ready;

	const flush = () => {
		const sessionId = pending.current;
		if (!sessionId || !readyRef.current) return;
		pending.current = null;
		router.push({ pathname: "/thread/[id]", params: { id: sessionId } });
	};

	useEffect(
		() =>
			onNotificationTap((data) => {
				pending.current = sessionFromNotificationData(data);
				flush();
			}),
		[],
	);
	useEffect(flush, [ready]);
	return null;
}

/** Right after pairing (or the first launch paired), offer notifications once. */
function OfferNotifications() {
	const { session } = useSession();
	useEffect(() => {
		if (session.status !== "remote") return;
		let cancelled = false;
		const timer = setTimeout(() => {
			void session.push.shouldOffer().then((offer) => {
				if (offer && !cancelled) router.push("/notifications");
			});
		}, 800);
		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
	}, [session]);
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
				<Stack.Screen name="notifications" options={{ presentation: "modal" }} />
				<Stack.Screen name="new-task" options={{ presentation: "modal" }} />
				<Stack.Screen name="delivery/[id]" options={{ presentation: "modal" }} />
			</Stack.Protected>
			<Stack.Screen name="pair" options={{ presentation: ready ? "modal" : "card", gestureEnabled: ready }} />
		</Stack>
	);
}
