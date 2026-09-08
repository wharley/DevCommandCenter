import { lazy, Suspense } from "react";
import {
	createRootRoute,
	createRoute,
	createRouter,
	Outlet,
} from "@tanstack/react-router";
const DiffRoute = lazy(() =>
	import("./routes/diff").then((m) => ({ default: m.DiffRoute })),
);
import { HomeRoute } from "./routes/home";
const NewThreadRoute = lazy(() =>
	import("./routes/new").then((m) => ({ default: m.NewThreadRoute })),
);
import { PairRoute } from "./routes/pair";
const PermissionsRoute = lazy(() =>
	import("./routes/permissions").then((m) => ({ default: m.PermissionsRoute })),
);
const SettingsRoute = lazy(() =>
	import("./routes/settings").then((m) => ({ default: m.SettingsRoute })),
);
const ThreadRoute = lazy(() =>
	import("./routes/thread").then((m) => ({ default: m.ThreadRoute })),
);

const rootRoute = createRootRoute({
	component: () => (
		<Suspense
			fallback={
				<p role="status" className="p-8 text-center text-mute">
					Carregando DCC…
				</p>
			}
		>
			<Outlet />
		</Suspense>
	),
});

const indexRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/",
	component: HomeRoute,
});

const pairRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/pair",
	component: PairRoute,
});

const threadRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/threads/$threadId",
	component: ThreadRoute,
});

const newThreadRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/new",
	component: NewThreadRoute,
});

const permissionsRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/permissions",
	component: PermissionsRoute,
});

const settingsRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/settings",
	component: SettingsRoute,
});

const diffRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/diff/$combId",
	component: DiffRoute,
});

const routeTree = rootRoute.addChildren([
	indexRoute,
	pairRoute,
	threadRoute,
	newThreadRoute,
	permissionsRoute,
	settingsRoute,
	diffRoute,
]);

// The SPA is mounted at /m/ in production (served by dccd-http) and at / in
// `vite dev`. Matches Vite's `base: "/m/"` config so links and matches resolve
// consistently in both modes.
const basepath = import.meta.env.BASE_URL.replace(/\/$/, "") || "/";

export const router = createRouter({ routeTree, basepath });

declare module "@tanstack/react-router" {
	interface Register {
		router: typeof router;
	}
}
