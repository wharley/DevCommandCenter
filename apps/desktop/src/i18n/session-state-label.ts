import type { TFunction } from "i18next";

/** Maps runtime session snapshot state codes to `common` locale strings. */
export function sessionStateLabel(state: string, t: TFunction<"common">): string {
	return t(`workbench.sessionState.${state}`, { defaultValue: state });
}
