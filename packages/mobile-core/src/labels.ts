/**
 * The phone's pt-BR copy for the trace and the access selector, under the
 * desktop's own i18n keys (`apps/desktop/src/i18n/locales/pt-BR/common.json`).
 * Metro can't reach apps/desktop, so the texts are copied here instead of
 * translated again; `labels.test.ts` fails as soon as one drifts from the
 * desktop. Tool kinds follow the desktop's `classifyToolAction`
 * (`turn-activity.logic.ts`), checked by the same test.
 */

export const DESKTOP_PT_BR = {
	"conversation.reasoning.label": "Pensando",
	"conversation.activity.turn.thought": "Pensou",
	"conversation.activity.turn.working": "Trabalhando",
	"conversation.activity.turn.workedNoDuration": "Trabalho do agente",
	"conversation.activity.turn.steps_one": "{{count}} passo",
	"conversation.activity.turn.steps_other": "{{count}} passos",
	"conversation.activity.timeline.filters.failures": "Falhas",
	"conversation.toolCall.failed": "Falhou",
	"conversation.toolCall.failedFallback": "A execução falhou",
	"conversation.toolCall.verbs.command.live": "Rodando",
	"conversation.toolCall.verbs.command.done": "Rodou",
	"conversation.toolCall.verbs.edit.live": "Editando",
	"conversation.toolCall.verbs.edit.done": "Editou",
	"conversation.toolCall.verbs.read.live": "Lendo",
	"conversation.toolCall.verbs.read.done": "Leu",
	"conversation.toolCall.verbs.search.live": "Buscando",
	"conversation.toolCall.verbs.search.done": "Buscou",
	"conversation.toolCall.verbs.web.live": "Pesquisando",
	"conversation.toolCall.verbs.web.done": "Pesquisou",
	"conversation.toolCall.verbs.plan.live": "Atualizando o plano",
	"conversation.toolCall.verbs.plan.done": "Atualizou o plano",
	"conversation.toolCall.verbs.agent.live": "Delegando",
	"conversation.toolCall.verbs.agent.done": "Delegou",
	"conversation.toolCall.verbs.mcp.live": "Usando",
	"conversation.toolCall.verbs.mcp.done": "Usou",
	"conversation.toolCall.verbs.other.live": "Usando",
	"conversation.toolCall.verbs.other.done": "Usou",
	"composer.fileMentions": "Arquivos",
	"composer.attachments.add": "Anexar arquivo ou imagem",
	"composer.attachments.imagesUnsupported":
		"Este provider não suporta imagens; as imagens foram anexadas como referências de arquivo.",
	"conversation.attachment.openImage": "Abrir imagem {{name}}",
	"quickOpen.loading": "Listando arquivos…",
	"quickOpen.empty": "Nenhum arquivo encontrado.",
	"composer.approval.open": "Escolher como as ações devem ser aprovadas",
	"composer.approval.managed": "Gerenciado",
	"composer.approval.managedHint": "As permissões são gerenciadas diretamente pelo {{provider}}.",
	"composer.approval.options.ask.label": "Solicitar aprovação",
	"composer.approval.options.ask.description":
		"Pede sua confirmação antes de editar ou executar ações que exigem acesso.",
	"composer.approval.options.auto.label": "Aprovar por mim",
	"composer.approval.options.auto.description":
		"O provedor libera ações comuns no workspace e pergunta quando houver risco.",
	"composer.approval.options.full_access.label": "Acesso completo",
	"composer.approval.options.full_access.description":
		"Permite ao agente acessar arquivos, comandos e rede sem pedir aprovação.",
	"composer.approval.confirm.title": "Ativar acesso completo?",
	"composer.approval.confirm.description":
		"O agente poderá executar comandos e acessar arquivos e rede sem solicitar sua confirmação.",
	"composer.approval.confirm.worktreeWarning":
		"O worktree protege o checkout Git original, mas não protege outros arquivos, credenciais ou recursos da sua máquina.",
	"composer.approval.confirm.cancel": "Cancelar",
	"composer.approval.confirm.enable": "Ativar acesso completo",
} as const;

export type LabelKey = keyof typeof DESKTOP_PT_BR;
type PluralBase<K> = K extends `${infer B}_one` ? B : never;

/** i18next's subset the phone needs: `{{name}}` and `_one`/`_other` by `count`. */
export function t(
	key: LabelKey | PluralBase<LabelKey>,
	vars: Record<string, string | number> = {},
): string {
	const table = DESKTOP_PT_BR as Record<string, string>;
	const plural =
		typeof vars.count === "number" ? `${key}_${vars.count === 1 ? "one" : "other"}` : null;
	const text = (plural && table[plural]) ?? table[key] ?? key;
	return text.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(vars[name] ?? ""));
}

/** What a tool call did, coarse enough to label a trace step. */
export type ToolKind =
	| "command"
	| "edit"
	| "read"
	| "search"
	| "web"
	| "plan"
	| "agent"
	| "mcp"
	| "other";

const KIND_BY_ACTION: Record<string, ToolKind> = {
	bash: "command",
	shell: "command",
	commandexecution: "command",
	execcommand: "command",
	terminal: "command",
	killshell: "command",
	bashoutput: "command",
	edit: "edit",
	multiedit: "edit",
	write: "edit",
	writefile: "edit",
	applypatch: "edit",
	filechange: "edit",
	notebookedit: "edit",
	read: "read",
	readfile: "read",
	view: "read",
	notebookread: "read",
	grep: "search",
	glob: "search",
	ls: "search",
	searchfiles: "search",
	find: "search",
	codesearch: "search",
	websearch: "web",
	webfetch: "web",
	browser: "web",
	todowrite: "plan",
	updateplan: "plan",
	task: "agent",
	agent: "agent",
};

export function classifyToolAction(action: string): ToolKind {
	if ((action.split("__").at(-1) ?? action) === "dcc_delegate_task") return "agent";
	if (action.startsWith("mcp__")) return "mcp";
	const normalized = action.toLowerCase().replace(/[^a-z0-9]/g, "");
	return KIND_BY_ACTION[normalized] ?? "other";
}

/** Readable name of an MCP tool: `mcp__github__create_issue` → `github · create_issue`. */
export function mcpToolLabel(action: string): string {
	const [, server, ...rest] = action.split("__");
	return rest.length ? `${server} · ${rest.join("__")}` : (server ?? action);
}

function shortPath(path: string): string {
	const segments = path.split("/").filter(Boolean);
	return segments.length <= 2 ? path : segments.slice(-2).join("/");
}

function firstLineOf(value: string, max: number): string {
	const line = value.trim().split("\n")[0] ?? "";
	return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}

/**
 * A tool step as the desktop's row reads it: the verb ("Rodou", "Editando")
 * and what it acted on — the command, the file, or the MCP tool's name.
 */
export function toolStepParts(
	action: string,
	input: unknown,
	live: boolean,
): { verb: string; target: string | null } {
	const kind = classifyToolAction(action);
	const record = input && typeof input === "object" ? (input as Record<string, unknown>) : {};
	const command = typeof record.command === "string" && record.command.trim() ? record.command : null;
	const file = typeof record.file === "string" && record.file.trim() ? record.file : null;
	let target: string | null = null;
	if (kind === "command" && command) target = firstLineOf(command, 120);
	else if ((kind === "edit" || kind === "read") && file) target = shortPath(file);
	else if (command) target = firstLineOf(command, 120);
	else if (file) target = shortPath(file);
	else if (kind === "mcp") target = mcpToolLabel(action);
	else if (kind === "other") target = action;
	const verb = t(`conversation.toolCall.verbs.${kind}.${live ? "live" : "done"}` as LabelKey);
	return { verb, target };
}
