import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";
import { I18nextProvider, useTranslation } from "react-i18next";
import i18n from "./config";

it.each([
	["pt-BR", "Modelo e execução", "Remover dos favoritos", "Voltar ao pedido"],
	["en", "Model and execution", "Remove from favorites", "Back to prompt"],
])("resolves compact picker labels in %s", (lng, title, remove, back) => {
	const t = i18n.getFixedT(lng, "common");
	expect(t("composer.execution.compactTitle")).toBe(title);
	expect(t("composer.execution.removeFavorite")).toBe(remove);
	expect(t("composer.execution.backToPrompt")).toBe(back);
});

it("updates mounted labels when development reloads a translation bundle", async () => {
	const host = document.createElement("div");
	const root = createRoot(host);
	const key = "developmentReloadProbe";
	function Label() {
		const { t } = useTranslation("common");
		return <span>{t(key)}</span>;
	}
	try {
		await act(async () => root.render(<I18nextProvider i18n={i18n}><Label /></I18nextProvider>));
		expect(host.textContent).toBe(key);
		await act(async () => {
			i18n.addResourceBundle(i18n.language, "common", { [key]: "Atualizado" }, true, true);
		});
		expect(host.textContent).toBe("Atualizado");
	} finally {
		await act(async () => root.unmount());
	}
});
