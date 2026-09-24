import { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import type { ErrorInfo, RootOptions } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { PersistQueryClientProvider } from "@tanstack/react-query-persist-client";
import { SplashScreen } from "./components/SplashScreen";
import { ThemeProvider } from "./components/theme-provider";
import { TooltipProvider } from "./components/ui/tooltip";
import { RenderErrorBoundary } from "./components/RenderErrorBoundary";
import {
	installFrontendErrorListeners,
	recordFrontendError,
} from "./lib/frontend-diagnostics";
import {
	createDccQueryClient,
	dccQueryPersistOptions,
} from "./lib/query-client";
import "./i18n/config";
import "./styles/app.css";

const isQuickComposer =
	new URLSearchParams(window.location.search).get("quick-composer") === "1";
const App = lazy(() => import("./App"));
const QuickComposer = lazy(() =>
	import("./features/quick-composer/QuickComposer").then((module) => ({
		default: module.QuickComposer,
	})),
);

const removeErrorListeners = installFrontendErrorListeners();
if (import.meta.hot) import.meta.hot.dispose(removeErrorListeners);
const queryClient = createDccQueryClient();

function reportReactRootError(
	kind: "uncaught" | "recoverable",
	error: unknown,
	info: ErrorInfo,
) {
	recordFrontendError(`react-${kind}`, error, {
		componentStack: info.componentStack,
	});
	console.error(`[dcc][react] ${kind} root error`, error, {
		componentStack: info.componentStack,
	});
}

const reactRootOptions: RootOptions = {
	onUncaughtError: (error, info) =>
		reportReactRootError("uncaught", error, info),
	onRecoverableError: (error, info) =>
		reportReactRootError("recoverable", error, info),
};

ReactDOM.createRoot(document.getElementById("root")!, reactRootOptions).render(
	<RenderErrorBoundary scope="app">
		<ThemeProvider>
			<TooltipProvider delayDuration={0}>
				{isQuickComposer ? (
					<QueryClientProvider client={queryClient}>
						<Suspense fallback={null}>
							<QuickComposer />
						</Suspense>
					</QueryClientProvider>
				) : (
					<PersistQueryClientProvider
						client={queryClient}
						persistOptions={dccQueryPersistOptions}
					>
						<Suspense fallback={null}>
							<App />
							<SplashScreen />
						</Suspense>
					</PersistQueryClientProvider>
				)}
			</TooltipProvider>
		</ThemeProvider>
	</RenderErrorBoundary>,
);
