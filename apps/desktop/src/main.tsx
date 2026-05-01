import React from "react";
import ReactDOM from "react-dom/client";
import { PersistQueryClientProvider } from "@tanstack/react-query-persist-client";
import { Toaster } from "sonner";
import App from "./App";
import { ThemeProvider, useAppearance } from "./components/theme-provider";
import { TooltipProvider } from "./components/ui/tooltip";
import { createDccQueryClient, dccQueryPersister } from "./lib/query-client";
import "./styles/color-theme.css";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";

const queryClient = createDccQueryClient();

function ThemeAwareToaster() {
	const { theme } = useAppearance();
	return <Toaster richColors theme={theme} />;
}

ReactDOM.createRoot(document.getElementById("root")!).render(
	<React.StrictMode>
		<ThemeProvider defaultTheme="dark">
			<TooltipProvider delayDuration={150}>
				<PersistQueryClientProvider
					client={queryClient}
					persistOptions={{ persister: dccQueryPersister }}
				>
					<App />
					<ThemeAwareToaster />
				</PersistQueryClientProvider>
			</TooltipProvider>
		</ThemeProvider>
	</React.StrictMode>,
);
