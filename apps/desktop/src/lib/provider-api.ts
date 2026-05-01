import { invoke } from "@tauri-apps/api/core";
import { PROVIDER_METHODS } from "@dcc/contracts";
import type { ListProvidersOutput } from "@dcc/contracts";

export function listProviders() {
	return invoke<ListProvidersOutput>(PROVIDER_METHODS.listProviders);
}
