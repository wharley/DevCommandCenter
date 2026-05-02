import React from "react";
import ReactDOM from "react-dom/client";
import { PersistQueryClientProvider } from "@tanstack/react-query-persist-client";
import App from "./App";
import { SplashScreen } from "./components/SplashScreen";
import { ThemeProvider } from "./components/theme-provider";
import { TooltipProvider } from "./components/ui/tooltip";
import { createDccQueryClient, dccQueryPersister } from "./lib/query-client";
import "./i18n/config";
import "./styles/app.css";

const queryClient = createDccQueryClient();

ReactDOM.createRoot(document.getElementById("root")!).render(
	<React.StrictMode>
		<ThemeProvider>
			<TooltipProvider delayDuration={0}>
				<PersistQueryClientProvider
					client={queryClient}
					persistOptions={{ persister: dccQueryPersister }}
				>
					<App />
					<SplashScreen />
				</PersistQueryClientProvider>
			</TooltipProvider>
		</ThemeProvider>
	</React.StrictMode>,
);
