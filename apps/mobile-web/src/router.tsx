import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { HomeRoute } from "./routes/home";
import { NewThreadRoute } from "./routes/new";
import { PairRoute } from "./routes/pair";
import { PermissionsRoute } from "./routes/permissions";
import { SettingsRoute } from "./routes/settings";
import { ThreadRoute } from "./routes/thread";

const rootRoute = createRootRoute({
	component: () => <Outlet />,
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

const routeTree = rootRoute.addChildren([
	indexRoute,
	pairRoute,
	threadRoute,
	newThreadRoute,
	permissionsRoute,
	settingsRoute,
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
