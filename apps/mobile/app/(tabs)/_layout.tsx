import { useAgentRequests } from "@/backend/agentRequests";
import { useTaskProposals } from "@/backend/taskProposals";
import { HomeDataProvider, useHomeData } from "@/screens/homeData";
import { font, useTheme } from "@/theme";
import Feather from "@expo/vector-icons/Feather";
import { Tabs } from "expo-router";

export default function TabsLayout() {
	return (
		<HomeDataProvider>
			<TabBar />
		</HomeDataProvider>
	);
}

function TabBar() {
	const { color } = useTheme();
	const { scan, attention } = useHomeData();
	const { requests } = useAgentRequests();
	const { proposals } = useTaskProposals();
	const needsYou = (attention?.length ?? scan?.needsYou.length ?? 0) + requests.length + proposals.length;
	return (
		<Tabs
			screenOptions={{
				headerShown: false,
				sceneStyle: { backgroundColor: color.bg },
				tabBarStyle: { backgroundColor: color.panel, borderTopColor: color.border },
				tabBarActiveTintColor: color.fg,
				tabBarInactiveTintColor: color.faint,
				tabBarLabelStyle: { fontFamily: font.mono, fontSize: 10, letterSpacing: 1.2, textTransform: "uppercase" },
			}}
		>
			<Tabs.Screen
				name="index"
				options={{
					title: "Agentes",
					tabBarIcon: ({ color: tint, size }) => <Feather name="cpu" color={tint} size={size - 4} />,
					tabBarBadge: needsYou > 0 ? needsYou : undefined,
					tabBarBadgeStyle: { backgroundColor: color.waitFill, color: color.waitInk, fontSize: 10 },
				}}
			/>
			<Tabs.Screen
				name="workspaces"
				options={{
					title: "Workspaces",
					tabBarIcon: ({ color: tint, size }) => <Feather name="git-branch" color={tint} size={size - 4} />,
				}}
			/>
		</Tabs>
	);
}
