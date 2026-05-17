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

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
	interface Register {
		router: typeof router;
	}
}
