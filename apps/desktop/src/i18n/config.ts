import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en/common.json";
import ptBR from "./locales/pt-BR/common.json";

export const LOCALE_STORAGE_KEY = "dcc.ui.locale";

export type AppLocale = "pt-BR" | "en";

function readStoredLocale(): AppLocale {
	if (typeof window === "undefined") {
		return "pt-BR";
	}
	try {
		const raw = window.localStorage.getItem(LOCALE_STORAGE_KEY);
		if (raw === "pt-BR" || raw === "en") {
			return raw;
		}
	} catch {
		/* localStorage unavailable */
	}
	return "pt-BR";
}

function applyDocumentLang(lng: string) {
	if (typeof document === "undefined") {
		return;
	}
	document.documentElement.lang = lng === "en" ? "en" : "pt-BR";
}

void i18n.use(initReactI18next).init({
	resources: {
		"pt-BR": { common: ptBR },
		en: { common: en },
	},
	lng: readStoredLocale(),
	fallbackLng: "en",
	defaultNS: "common",
	ns: ["common"],
	interpolation: { escapeValue: false },
});

applyDocumentLang(i18n.language);

if (typeof window !== "undefined") {
	i18n.on("languageChanged", (lng) => {
		applyDocumentLang(lng);
		try {
			window.localStorage.setItem(LOCALE_STORAGE_KEY, lng);
		} catch {
			/* ignore */
		}
	});
}

export default i18n;
