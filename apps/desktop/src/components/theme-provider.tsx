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

export type DccTheme = "dark" | "light";

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

function readStoredTheme(fallback: DccTheme): DccTheme {
	if (typeof window === "undefined") {
		return fallback;
	}

	const stored = window.localStorage.getItem(DCC_THEME_STORAGE_KEY);
	return stored === "light" || stored === "dark" ? stored : fallback;
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

type AppearanceContextValue = {
	theme: DccTheme;
	setTheme: (theme: DccTheme) => void;
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
	defaultTheme = "dark",
}: ThemeProviderProps) {
	const [theme, setThemeState] = useState<DccTheme>(() => {
		const initial = readStoredTheme(defaultTheme);
		applyDccThemeClass(initial);
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

	const value = useMemo(() => ({ theme, setTheme }), [theme, setTheme]);

	return (
		<AppearanceContext.Provider value={value}>
			{children}
		</AppearanceContext.Provider>
	);
}
