import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { HomeRoute } from "./routes/home";
import { PairRoute } from "./routes/pair";

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

const routeTree = rootRoute.addChildren([indexRoute, pairRoute]);

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
