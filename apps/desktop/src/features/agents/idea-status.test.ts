import { describe, expect, it } from "vitest";
import {
	isValidRepositoryName,
	parseIdeaStatus,
	repositoryNameFromCandidate,
} from "./idea-status";

const EMPTY = `# Ideia

## Problema

## Para quem

## Referências

## Hipóteses

## Riscos

## Nome

## Veredito

## MVP

## Arquitetura e stack

## Roadmap

## Registro de decisões
`;

const FILLED = `# Ideia: Plataforma de clínica toda via chat

## Problema
Clínicas pequenas perdem tempo com telas e formulários.

## Para quem
Clínicas de 1 a 5 profissionais.

## Referências
- [Dot](https://example.com/dot) — assistente da OpenAI
- [Grokbot](https://example.com/grok) — bot de atendimento
- [Doctoralia](https://doctoralia.com.br) — agenda online
- Um concorrente sem link

## Hipóteses
- [confirmada] Recepcionistas usam WhatsApp o dia todo — entrevistas
- [derrubada] Pacientes querem app próprio — pesquisa
- texto solto que não é hipótese

## Nome
- Clinichat
- ★ **Salte** — curto e livre
- Salte

## Veredito
**Seguir** — o problema é real e o MVP é pequeno.

## MVP
Agenda + confirmação via chat.

## Arquitetura e stack
Backend em Node com Postgres.

## Roadmap
### Fase 1: agenda
- cadastro de horários

## Registro de decisões
- 2026-10-03: público inicial são clínicas pequenas
`;

describe("parseIdeaStatus", () => {
	it("leaves every milestone open on the empty template", () => {
		const status = parseIdeaStatus(EMPTY);
		expect(status.title).toBeNull();
		expect(status.filledCount).toBe(0);
		expect(status.verdict).toBeNull();
		expect(status.hasRoadmap).toBe(false);
		expect(status.hasArchitecture).toBe(false);
	});

	it("reads a filled idea by its fixed headings and formats", () => {
		const status = parseIdeaStatus(FILLED);
		expect(status.title).toBe("Plataforma de clínica toda via chat");
		expect(status.milestones).toEqual({
			problem: true,
			audience: true,
			references: true,
			hypotheses: true,
			name: true,
		});
		expect(status.filledCount).toBe(5);
		expect(status.referenceCount).toBe(3);
		expect(status.hypotheses).toHaveLength(2);
		expect(status.favoriteName).toBe("Salte");
		expect(status.names).toEqual(["Salte", "Clinichat"]);
		expect(status.verdict).toBe("seguir");
		expect(status.mvp).toBe("Agenda + confirmação via chat.");
		expect(status.hasArchitecture).toBe(true);
		expect(status.hasRoadmap).toBe(true);
	});

	it("keeps a milestone open while a hypothesis is still open or references are few", () => {
		const status = parseIdeaStatus(
			FILLED.replace("[derrubada]", "[aberta]").replace("(https://doctoralia.com.br)", ""),
		);
		expect(status.milestones.hypotheses).toBe(false);
		expect(status.milestones.references).toBe(false);
		expect(status.filledCount).toBe(3);
	});

	it("tolerates headings without accents and an unknown verdict", () => {
		const status = parseIdeaStatus(
			"# Ideia: x\n\n## Referencias\n- https://a.dev\n\n## Veredito\nTalvez\n",
		);
		expect(status.referenceCount).toBe(1);
		expect(status.verdict).toBeNull();
	});
});

describe("repository names", () => {
	it("derives a valid name from a candidate", () => {
		expect(repositoryNameFromCandidate("Salte Saúde!")).toBe("salte-saude");
		expect(isValidRepositoryName("salte-saude")).toBe(true);
		expect(isValidRepositoryName("salte saude")).toBe(false);
		expect(isValidRepositoryName("..")).toBe(false);
		expect(isValidRepositoryName("")).toBe(false);
	});
});
