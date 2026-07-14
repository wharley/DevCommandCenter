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

export type DccTheme = "dark" | "light";
export type DccDensity = "comfortable" | "compact";

/** PWA/tab chrome tint — matches approximate shell `--background`. */
export const DCC_THEME_COLOR_META: Record<DccTheme, string> = {
	dark: "#151515",
	light: "#fafafa",
};

type ThemeProviderProps = {
	children: ReactNode;
	/** Applied when nothing valid is stored yet. */
	defaultTheme?: DccTheme;
};

function readStoredTheme(fallback?: DccTheme): DccTheme | null {
	if (typeof window === "undefined") {
		return fallback ?? null;
	}

	const stored = window.localStorage.getItem(DCC_THEME_STORAGE_KEY);
	if (stored === "light" || stored === "dark") {
		return stored;
	}

	return fallback ?? null;
}

function getSystemTheme(): DccTheme {
	if (typeof window === "undefined") {
		return "dark";
	}

	return window.matchMedia("(prefers-color-scheme: dark)").matches
		? "dark"
		: "light";
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
	theme: DccTheme;
	setTheme: (theme: DccTheme) => void;
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
	const [theme, setThemeState] = useState<DccTheme>(() => {
		const initial =
			readStoredTheme(defaultTheme) ?? getSystemTheme();
		applyDccThemeClass(initial);
		return initial;
	});
	const [density, setDensityState] = useState<DccDensity>(() => {
		const initial = readStoredDensity();
		applyDccDensity(initial);
		return initial;
	});

	const setTheme = useCallback((next: DccTheme) => {
		setThemeState(next);
		try {
			window.localStorage.setItem(DCC_THEME_STORAGE_KEY, next);
		} catch {
			/* localStorage unavailable */
		}
		applyDccThemeClass(next);
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

	useEffect(() => {
		if (typeof window === "undefined") {
			return;
		}
		if (!("__TAURI_INTERNALS__" in window)) {
			return;
		}

		void import("@tauri-apps/api/app")
			.then(({ setTheme }) => setTheme(theme))
			.catch(() => {
				/* native theme API unavailable */
			});
	}, [theme]);

	const value = useMemo(
		() => ({ theme, setTheme, density, setDensity }),
		[density, setDensity, setTheme, theme],
	);

	return (
		<AppearanceContext.Provider value={value}>
			{children}
		</AppearanceContext.Provider>
	);
}
