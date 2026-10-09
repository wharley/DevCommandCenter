import { parseRoute } from "./protocol.ts";
import type { Env } from "./room.ts";

export { RelayRoom } from "./room.ts";

export default {
	async fetch(request: Request, env: Env): Promise<Response> {
		const url = new URL(request.url);
		if (url.pathname === "/health") return new Response("ok");
		const route = parseRoute(url.pathname);
		if (!route) return new Response("not found", { status: 404 });
		if (route.kind === "traffic") {
			if (env.DCC_RELAY_DEBUG_TRAFFIC !== "1") return new Response("not found", { status: 404 });
		} else if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
			return new Response("expected a WebSocket upgrade", { status: 426 });
		}
		const room = env.ROOMS.get(env.ROOMS.idFromName(route.room));
		return room.fetch(request);
	},
} satisfies ExportedHandler<Env>;
