/**
 * "Mission control" tokens, light and dark: quiet surfaces, green only for
 * what is live or the primary action, amber for "needs you", red for
 * failures, monospace as the instrument voice. The scheme follows the system.
 *
 * Token names are the same in both palettes. Every key below `info` is a
 * `#rrggbb` hex so `alpha()` can tint it; the glass/shadow keys are rgba.
 */
import { Platform, StyleSheet, useColorScheme } from "react-native";

const dark = {
	bg: "#0a0b0d",
	panel: "#111419",
	elevated: "#181c23",
	border: "#232834",
	fg: "#e9edf2",
	mute: "#7c8694",
	faint: "#565f6b",
	accent: "#34e2a0",
	accentInk: "#04231b",
	/** Amber as text and icons. */
	wait: "#f5b13d",
	/** Amber as a fill (buttons, badges, card tints) and the ink on it. */
	waitFill: "#f5b13d",
	waitInk: "#241500",
	danger: "#f5604d",
	info: "#6ea8fe",
	/** Neutral fill for chips and secondary round buttons. */
	chip: "#232831",
	/** The person's message in a thread. */
	bubble: "#232831",
	/** Floating controls where the system has no glass (Android, iOS < 26). */
	glass: "rgba(30,34,41,0.72)",
	glassBorder: "rgba(255,255,255,0.08)",
	shadow: "rgba(0,0,0,0.45)",
	scrim: "rgba(0,0,0,0.55)",
};

export type Palette = { [K in keyof typeof dark]: string };

/**
 * In light the green is darker so it works both as text on white and as a
 * fill under white ink; amber splits into a dark text tone and the bright
 * fill (≥4.5:1 either way).
 */
const light: Palette = {
	bg: "#f2f3f5",
	panel: "#ffffff",
	elevated: "#e9ecef",
	border: "#dce0e5",
	fg: "#0e1116",
	mute: "#5b6472",
	faint: "#8a93a0",
	accent: "#0a7f58",
	accentInk: "#ffffff",
	wait: "#8f5400",
	waitFill: "#f5b13d",
	waitInk: "#241500",
	danger: "#c93a2a",
	info: "#2457c5",
	chip: "#eceef1",
	bubble: "#e2e5ea",
	glass: "rgba(255,255,255,0.78)",
	glassBorder: "rgba(255,255,255,0.9)",
	shadow: "rgba(14,17,22,0.12)",
	scrim: "rgba(14,17,22,0.30)",
};

export type Scheme = "light" | "dark";

export const palettes: Record<Scheme, Palette> = { light, dark };

export const font = {
	mono: Platform.select({ ios: "Menlo", default: "monospace" }),
} as const;

export const radius = { sm: 8, md: 12, lg: 16, xl: 20, sheet: 32 } as const;

/** The scheme the system asks for; dark until it answers. */
export function useScheme(): Scheme {
	return useColorScheme() === "light" ? "light" : "dark";
}

export function useTheme(): { scheme: Scheme; color: Palette } {
	const scheme = useScheme();
	return { scheme, color: palettes[scheme] };
}

/**
 * Styles that follow the scheme. The factory runs once per scheme and the
 * result is cached, so a screen pays `StyleSheet.create` at most twice.
 *
 *     const useStyles = makeStyles((color) => StyleSheet.create({ … }));
 *     const styles = useStyles();
 */
export function makeStyles<T>(factory: (color: Palette) => T): () => T {
	const cache: Partial<Record<Scheme, T>> = {};
	return function useStyles() {
		const scheme = useScheme();
		return (cache[scheme] ??= factory(palettes[scheme]));
	};
}

/** Translucent variant of a hex token (`#rrggbb` + alpha). */
export function alpha(hex: string, amount: number): string {
	const a = Math.round(Math.min(1, Math.max(0, amount)) * 255)
		.toString(16)
		.padStart(2, "0");
	return `${hex}${a}`;
}

/** A hairline that reads in both schemes. */
export const hairline = StyleSheet.hairlineWidth;
