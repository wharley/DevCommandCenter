import {
	createContext,
	useCallback,
	useContext,
	useEffect,
	useMemo,
	useState,
	type ReactNode,
} from "react";

/** Persisted preference; canonical key for docs and tooling. */
export const DCC_THEME_STORAGE_KEY = "dcc-theme";
export const DCC_DENSITY_STORAGE_KEY = "dcc-density";

/** Resolved theme actually painted on screen. */
export type DccTheme = "dark" | "light";
/** What the user picked; "system" follows the OS appearance live. */
export type DccThemePreference = DccTheme | "system";
export type DccDensity = "comfortable" | "compact";

/** PWA/tab chrome tint — matches approximate shell `--background`. */
export const DCC_THEME_COLOR_META: Record<DccTheme, string> = {
	dark: "#151515",
	light: "#fafafa",
};

type ThemeProviderProps = {
	children: ReactNode;
	/** Applied when nothing valid is stored yet. */
	defaultTheme?: DccThemePreference;
};

const SYSTEM_DARK_QUERY = "(prefers-color-scheme: dark)";

export function isDccThemePreference(
	value: unknown,
): value is DccThemePreference {
	return value === "light" || value === "dark" || value === "system";
}

function readStoredThemePreference(
	fallback: DccThemePreference = "system",
): DccThemePreference {
	if (typeof window === "undefined") {
		return fallback;
	}

	try {
		const stored = window.localStorage.getItem(DCC_THEME_STORAGE_KEY);
		return isDccThemePreference(stored) ? stored : fallback;
	} catch {
		return fallback;
	}
}

function getSystemTheme(): DccTheme {
	if (typeof window === "undefined" || !window.matchMedia) {
		return "dark";
	}

	return window.matchMedia(SYSTEM_DARK_QUERY).matches ? "dark" : "light";
}

function isLinuxWebview(): boolean {
	return (
		typeof navigator !== "undefined" &&
		/Linux/.test(navigator.userAgent) &&
		!/Android/.test(navigator.userAgent)
	);
}

/**
 * Theme to hand to Tauri's app-level `setTheme`. On macOS/Windows, `null`
 * gives the window back to the OS; forcing the resolved theme would pin the
 * webview's prefers-color-scheme and stop "system" from tracking the OS.
 * On Linux, tao turns `null` into "prefer light" in GTK (which WebKitGTK
 * reads), so we push the portal's color-scheme instead — window `theme()`
 * reports it while no per-window theme is set, and tao keeps GTK in sync
 * with later portal changes on its own.
 */
async function resolveNativeTheme(
	preference: DccThemePreference,
): Promise<DccTheme | null> {
	if (preference !== "system") {
		return preference;
	}
	if (!isLinuxWebview()) {
		return null;
	}
	const { getCurrentWindow } = await import("@tauri-apps/api/window");
	return getCurrentWindow().theme();
}

export function resolveDccTheme(
	preference: DccThemePreference,
	systemTheme: DccTheme,
): DccTheme {
	return preference === "system" ? systemTheme : preference;
}

function readStoredDensity(): DccDensity {
	if (typeof window === "undefined") {
		return "comfortable";
	}
	return window.localStorage.getItem(DCC_DENSITY_STORAGE_KEY) === "compact"
		? "compact"
		: "comfortable";
}

export function applyDccThemeClass(theme: DccTheme) {
	if (typeof document === "undefined") {
		return;
	}

	const root = document.documentElement;
	root.classList.toggle("dark", theme === "dark");
	root.dataset.theme = theme;
	root.style.colorScheme = theme;
	const meta = document.getElementById("dcc-theme-color");
	if (meta) {
		meta.setAttribute("content", DCC_THEME_COLOR_META[theme]);
	}
}

export function applyDccDensity(density: DccDensity) {
	if (typeof document === "undefined") {
		return;
	}
	document.documentElement.dataset.density = density;
}

type AppearanceContextValue = {
	/** Resolved theme — use for rendering (diff colors, toasts, …). */
	theme: DccTheme;
	/** Stored choice, including "system". */
	themePreference: DccThemePreference;
	setTheme: (preference: DccThemePreference) => void;
	density: DccDensity;
	setDensity: (density: DccDensity) => void;
};

const AppearanceContext = createContext<AppearanceContextValue | null>(null);

export function useAppearance(): AppearanceContextValue {
	const ctx = useContext(AppearanceContext);
	if (!ctx) {
		throw new Error("useAppearance must be used within ThemeProvider");
	}
	return ctx;
}

export function ThemeProvider({
	children,
	defaultTheme,
}: ThemeProviderProps) {
	const [themePreference, setThemePreferenceState] =
		useState<DccThemePreference>(() => {
			const initial = readStoredThemePreference(defaultTheme);
			applyDccThemeClass(resolveDccTheme(initial, getSystemTheme()));
			return initial;
		});
	const [systemTheme, setSystemTheme] = useState<DccTheme>(getSystemTheme);
	const theme = resolveDccTheme(themePreference, systemTheme);
	const [density, setDensityState] = useState<DccDensity>(() => {
		const initial = readStoredDensity();
		applyDccDensity(initial);
		return initial;
	});

	const setTheme = useCallback((next: DccThemePreference) => {
		setThemePreferenceState(next);
		try {
			window.localStorage.setItem(DCC_THEME_STORAGE_KEY, next);
		} catch {
			/* localStorage unavailable */
		}
	}, []);

	useEffect(() => {
		applyDccThemeClass(theme);
	}, [theme]);

	// Follow OS appearance changes while the app is open.
	useEffect(() => {
		if (typeof window === "undefined" || !window.matchMedia) {
			return;
		}
		const query = window.matchMedia(SYSTEM_DARK_QUERY);
		const sync = () => setSystemTheme(query.matches ? "dark" : "light");
		sync();
		query.addEventListener("change", sync);
		return () => query.removeEventListener("change", sync);
	}, []);

	const setDensity = useCallback((next: DccDensity) => {
		setDensityState(next);
		try {
			window.localStorage.setItem(DCC_DENSITY_STORAGE_KEY, next);
		} catch {
			/* localStorage unavailable */
		}
		applyDccDensity(next);
	}, []);

	// The quick composer and main workbench share preferences, not React state.
	useEffect(() => {
		const syncAppearance = (event: StorageEvent) => {
			if (event.key === DCC_THEME_STORAGE_KEY && isDccThemePreference(event.newValue)) {
				setThemePreferenceState(event.newValue);
			}
			if (event.key === DCC_DENSITY_STORAGE_KEY && (event.newValue === "comfortable" || event.newValue === "compact")) {
				setDensityState(event.newValue);
				applyDccDensity(event.newValue);
			}
		};
		window.addEventListener("storage", syncAppearance);
		return () => window.removeEventListener("storage", syncAppearance);
	}, []);

	useEffect(() => {
		if (typeof window === "undefined") {
			return;
		}
		if (!("__TAURI_INTERNALS__" in window)) {
			return;
		}

		void resolveNativeTheme(themePreference)
			.then(async (native) => {
				const { setTheme } = await import("@tauri-apps/api/app");
				await setTheme(native);
			})
			.catch(() => {
				/* native theme API unavailable */
			});
	}, [themePreference]);

	const value = useMemo(
		() => ({ theme, themePreference, setTheme, density, setDensity }),
		[density, setDensity, setTheme, theme, themePreference],
	);

	return (
		<AppearanceContext.Provider value={value}>
			{children}
		</AppearanceContext.Provider>
	);
}
