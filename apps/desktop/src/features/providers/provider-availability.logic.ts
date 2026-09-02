import type {
	ProviderAvailabilityOutput,
	SetProviderAvailabilityInput,
} from "@dcc/contracts";

type PersistAvailabilityDependencies = {
	setAvailability: (
		input: SetProviderAvailabilityInput,
	) => Promise<ProviderAvailabilityOutput>;
	invalidateCatalog: () => Promise<unknown>;
};

export type ProviderAvailabilityRequestToken = {
	generation: number;
	requestId: number;
};

export function isProviderAvailabilityRequestCurrent(
	token: ProviderAvailabilityRequestToken,
	current: {
		generation: number;
		requestId: number | undefined;
		mounted: boolean;
		open: boolean;
	},
): boolean {
	return (
		current.mounted &&
		current.open &&
		current.generation === token.generation &&
		current.requestId === token.requestId
	);
}

/** Persists first, then reconciles the catalog; no optimistic authority is kept in the UI. */
export async function persistProviderAvailability(
	input: SetProviderAvailabilityInput,
	dependencies: PersistAvailabilityDependencies,
): Promise<ProviderAvailabilityOutput> {
	let result: ProviderAvailabilityOutput | undefined;
	let persistenceError: unknown;
	try {
		result = await dependencies.setAvailability(input);
	} catch (error) {
		persistenceError = error;
	}

	let reconciliationError: unknown;
	try {
		// The backend may persist Disabled before cleanup fails. Always reconcile
		// the catalog, including failed attempts, without replacing the original
		// persistence error.
		await dependencies.invalidateCatalog();
	} catch (error) {
		reconciliationError = error;
	}

	if (persistenceError !== undefined) {
		throw persistenceError;
	}
	if (reconciliationError !== undefined) {
		throw reconciliationError;
	}
	return result as ProviderAvailabilityOutput;
}
