import { describe, expect, it } from "vitest";
import { deriveTaskTitle } from "./task-title";

describe("deriveTaskTitle", () => {
	it("removes conversational Portuguese boilerplate", () => {
		expect(
			deriveTaskTitle("Preciso que você corrija o login do checkout"),
		).toBe("Corrija o login do checkout");
	});

	it("uses the first meaningful line and strips composer commands", () => {
		expect(
			deriveTaskTitle("\n/plan integrar pedidos ao Melhor Envio\nCom segurança"),
		).toBe("Integrar pedidos ao Melhor Envio");
	});

	it("keeps sidebar titles compact", () => {
		const title = deriveTaskTitle(
			"Analise cuidadosamente toda a arquitetura existente e implemente uma solução robusta para autenticação compartilhada",
		);
		expect(title.endsWith("…")).toBe(true);
		expect(title.length).toBeLessThanOrEqual(57);
	});
});
