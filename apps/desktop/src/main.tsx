import ReactDOM from "react-dom/client";
import type { ErrorInfo, RootOptions } from "react-dom/client";
import { PersistQueryClientProvider } from "@tanstack/react-query-persist-client";
import App from "./App";
import { SplashScreen } from "./components/SplashScreen";
import { ThemeProvider } from "./components/theme-provider";
import { TooltipProvider } from "./components/ui/tooltip";
import {
	createDccQueryClient,
	dccQueryPersistOptions,
} from "./lib/query-client";
import "./i18n/config";
import "./styles/app.css";

const queryClient = createDccQueryClient();

function reportReactRootError(kind: "uncaught" | "recoverable", error: unknown, info: ErrorInfo) {
	console.error(`[dcc][react] ${kind} root error`, error, {
		componentStack: info.componentStack,
	});
}

const reactRootOptions: RootOptions | undefined = import.meta.env.DEV
	? {
			onUncaughtError: (error, info) => reportReactRootError("uncaught", error, info),
			onRecoverableError: (error, info) =>
				reportReactRootError("recoverable", error, info),
		}
	: undefined;

ReactDOM.createRoot(document.getElementById("root")!, reactRootOptions).render(
	<ThemeProvider>
		<TooltipProvider delayDuration={0}>
			<PersistQueryClientProvider
				client={queryClient}
				persistOptions={dccQueryPersistOptions}
			>
				<App />
				<SplashScreen />
			</PersistQueryClientProvider>
		</TooltipProvider>
	</ThemeProvider>,
);
