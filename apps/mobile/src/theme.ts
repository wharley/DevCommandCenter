/**
 * "Mission control" tokens: cool ink panels, phosphor green only for what is
 * live or actionable, amber for "needs you", red for failures, monospace as
 * the instrument voice.
 */
import { Platform } from "react-native";

export const color = {
	bg: "#0a0b0d",
	panel: "#111419",
	elevated: "#181c23",
	border: "#232834",
	fg: "#e9edf2",
	mute: "#7c8694",
	faint: "#565f6b",
	accent: "#34e2a0",
	accentInk: "#04231b",
	wait: "#f5b13d",
	waitInk: "#241500",
	danger: "#f5604d",
	info: "#6ea8fe",
} as const;

export const font = {
	mono: Platform.select({ ios: "Menlo", default: "monospace" }),
} as const;

export const radius = { sm: 8, md: 12, lg: 16 } as const;

/** Translucent variant of a hex token (`#rrggbb` + alpha). */
export function alpha(hex: string, amount: number): string {
	const a = Math.round(Math.min(1, Math.max(0, amount)) * 255)
		.toString(16)
		.padStart(2, "0");
	return `${hex}${a}`;
}
