import { useEffect, type ReactNode } from "react";

type ThemeProviderProps = {
	children: ReactNode;
	defaultTheme?: "dark" | "light";
};

export function ThemeProvider({
	children,
	defaultTheme = "dark",
}: ThemeProviderProps) {
	useEffect(() => {
		const root = document.documentElement;
		const stored = window.localStorage.getItem("dcc-theme");
		const theme = stored === "light" || stored === "dark" ? stored : defaultTheme;
		root.classList.toggle("dark", theme === "dark");
		root.dataset.theme = theme;
	}, [defaultTheme]);

	return <>{children}</>;
}
