/**
 * The last catalog each connection saw. The desktop asks every agent runtime
 * for its models to build it (seconds), so "Nova tarefa" opens with this one
 * and refreshes behind it; the tabs keep it warm.
 */
import type { MobileCatalog } from "@dcc/mobile-core";
import type { Backend } from "./types";

const lastCatalog = new WeakMap<Backend, MobileCatalog>();

export function cachedCatalog(backend: Backend): MobileCatalog | null {
	return lastCatalog.get(backend) ?? null;
}

export async function fetchCatalog(backend: Backend): Promise<MobileCatalog> {
	const catalog = await backend.request<MobileCatalog>("GET", "/api/v1/mobile/catalog");
	lastCatalog.set(backend, catalog);
	return catalog;
}
