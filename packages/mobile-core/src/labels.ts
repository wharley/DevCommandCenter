/**
 * The phone's pt-BR copy for the trace and the access selector, under the
 * app's own i18n keys (`apps/native/locales/pt-BR/common.json`).
 * Metro can't reach apps/native, so the texts are copied here instead of
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
	"sidebar.attention.title": "Precisa de você",
	"sidebar.attention.reason.woke": "Voltou da espera",
	"sidebar.attention.reason.conflicts": "Conflitos para resolver",
	"sidebar.attention.reason.prConflicts": "PR #{{pr}} em conflito com a base",
	"sidebar.attention.reason.checksFailing": "Checks falhando no PR #{{pr}}",
	"sidebar.attention.reason.delegatedReview_one": "{{count}} mudança delegada para revisar",
	"sidebar.attention.reason.delegatedReview_other": "{{count}} mudanças delegadas para revisar",
	"sidebar.attention.reason.permission": "Aguardando aprovação",
	"sidebar.attention.reason.input": "Aguardando você",
	"sidebar.attention.reason.usageLimit": "Pausado por limite",
	"sidebar.agentState.limitWaiting": "Aguardando limite",
	"usageLimit.timelineWaiting": "Limite de uso atingido · volta às {{time}}",
	"usageLimit.timelinePaused": "Limite de uso atingido",
	"usageLimit.timelineResumedAuto": "Retomado sozinho depois do limite",
	"usageLimit.timelineResumedManual": "Retomado depois do limite",
	"usageLimit.continue": "Continuar",
	"sidebar.attention.reason.setup": "Setup pendente",
	"sidebar.attention.reason.aborted": "Interrompido",
	"sidebar.attention.reason.completed": "Resultado novo",
	"composer.executionDock.actions.syncBase": "Atualizar com a base",
	"composer.executionDock.actions.commit": "Commitar",
	"composer.executionDock.actions.createDraft": "Criar draft {{requestLabel}}",
	"composer.executionDock.actions.noLocalChanges": "Não há mudanças locais para commitar.",
	"composer.executionDock.actions.committed": "Commit criado",
	"composer.executionDock.actions.pushed": "Enviado",
	"composer.executionDock.actions.committedAndPushed": "Commit criado e enviado",
	"composer.executionDock.actions.pushBehindRemote": "{{target}} tem commits que você não tem localmente",
	"composer.executionDock.actions.pushBehindRemoteHint":
		"Alguém (ou outra tarefa) enviou antes. Atualizar coloca seus commits por cima dos remotos (ou mescla, se houver alterações não commitadas no checkout) e envia de novo; o DCC para se houver conflitos.",
	"composer.executionDock.actions.syncAndPush": "Atualizar e enviar",
	"composer.executionDock.actions.syncAndPushSuccess": "Atualizado de {{target}} e enviado",
	"composer.executionDock.createRequest.title": "Criar {{requestLabel}}",
	"composer.executionDock.createRequest.route": "{{head}} → {{base}}",
	"composer.executionDock.createRequest.titleLabel": "Título",
	"composer.executionDock.createRequest.descriptionLabel": "Descrição (opcional)",
	"composer.executionDock.createRequest.descriptionPlaceholder": "Adicione contexto para quem vai revisar…",
	"composer.executionDock.createRequest.includeLocalChanges": "Incluir mudanças locais (commitar e enviar)",
	"composer.executionDock.createRequest.localSummary": "{{files}} arquivos · +{{additions}} −{{deletions}}",
	"composer.executionDock.createRequest.draft": "Criar como draft",
	"composer.executionDock.createRequest.created": "{{requestLabel}} criado",
	"commit.modes.create-pr.idle": "Criar {{requestLabel}}",
	"commit.modes.open-pr.idle": "Abrir {{requestLabel}}",
	"commit.modes.commit-and-push.idle": "Commitar e enviar",
	"commit.modes.push.idle": "Enviar",
	"commit.modes.fix.idle": "Corrigir CI",
	"commit.modes.resolve-conflicts.idle": "Resolver",
	"commit.modes.complete-merge.idle": "Concluir",
	"commit.modes.merge.idle": "Mesclar",
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
	"agents.room.handoff.suggests": "{{agent}} sugere passar a vez para {{teammate}}",
	"agents.room.handoff.autoPassed": "Passado automaticamente para {{teammate}}",
	"agents.room.handoff.pass": "Passar a vez",
	"agents.room.handoff.ignore": "Ignorar",
	"agents.room.handoff.passed": "Passado para {{teammate}}",
	"agents.room.task.title": "{{agent}} · “{{task}}”",
	"agents.room.task.open": "Abrir tarefa",
	"agents.room.steps.more_one": "+{{count}} passo",
	"agents.room.steps.more_other": "+{{count}} passos",
	"agents.room.live.stop": "Parar",
	"agents.room.live.stopping": "Parando…",
	"agents.room.live.reading": "Lendo o pedido…",
	"agents.room.live.thinking": "Pensando…",
	"agents.room.live.writing": "Escrevendo…",
	"agents.room.live.long": "Ainda trabalhando: tarefas longas levam um tempo",
	"agents.room.live.stopped": "{{agent}} parou antes de terminar",
	"agents.room.live.retry": "Tentar de novo",
	"agents.room.queued": "Na fila · sai quando o {{agent}} terminar",
	"agents.room.jumpTip": "Ir para o fim",
	"agents.room.newLines_one": "{{count}} nova",
	"agents.room.newLines_other": "{{count}} novas",
	"agents.room.card.done": "{{agent}} terminou em {{task}}",
	"agents.room.card.waiting": "{{agent}} precisa de você em {{task}}",
	"agents.room.card.reviewed": "{{agent}} revisou {{task}}",
	"agents.room.card.reviewing": "{{agent}} está revisando {{task}}",
	"agents.room.card.reviewFailed": "{{agent}} não conseguiu revisar {{task}}",
	"agents.room.card.trigger.prUpdated": "Gatilho · PR atualizado",
	"agents.room.card.trigger.ciFailed": "Gatilho · CI falhou",
	"agents.room.card.trigger.reviewReceived": "Gatilho · review recebido",
	"agents.room.card.trigger.reviewRequested": "Gatilho · PR pede revisão",
	"agents.room.card.trigger.issueAssigned": "Gatilho · issue atribuída",
	"agents.room.card.trigger.taskStalled": "Gatilho · tarefa parada",
	"agents.room.card.trigger.routine": "Gatilho · rotina",
	"agents.room.checkIn.tag": "Check-in",
	"agents.room.checkIn.reviewRequested": "PR pede sua revisão: {{title}}",
	"agents.room.checkIn.issueAssigned": "Issue para você: {{title}}",
	"agents.room.checkIn.taskStalled": "{{title}} está sem sinal há {{minutes}} min",
	"agents.room.limit.text": "{{agent}} chegou ao limite do mês ({{limit}} tokens)",
	"agents.spend.continue": "Continuar mesmo assim",
	"agents.proposal.by": "{{agent}} propõe um agente",
	"agents.proposal.everyProject": "Em todos os projetos",
	"agents.proposal.projects": "Em: {{projects}}",
	"agents.proposal.first": "Primeiro pedido: {{request}}",
	"agents.proposal.create": "Criar",
	"agents.proposal.discard": "Descartar",
	"agents.routines.proposal": "{{agent}} propõe uma rotina · {{when}}",
	"agents.sources.proposal": "{{agent}} quer vigiar",
	"agents.sources.what": "{{connector}} — {{watch}} · a cada {{every}} min",
	"agents.reactions.rulePlaceholder": "O que ele deveria fazer? Ex.: sempre cite o PR",
	"agents.reactions.ruleAsk": "Guardar como regra?",
	"agents.reactions.keep": "Guardar",
	"agents.reactions.skip": "Agora não",
	"agents.reactions.kept": "Regra guardada na memória do agente.",
	"pullRequests.verdict.approve": "Pode aprovar",
	"pullRequests.verdict.requestChanges": "Pedir mudanças",
	"pullRequests.verdict.undecided": "Falta decidir",
	"conversation.permission.reasonPlaceholder": "Por que recusar? O agente lê isso",
	"conversation.permission.denySend": "Enviar recusa",
	"conversation.permission.denyWithReason": "Recusar com motivo",
	"conversation.permission.always": "Sempre permitir",
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
	const bareAction = action.split("__").at(-1) ?? action;
	if (
		bareAction === "dcc_delegate_task" ||
		bareAction === "dcc_propose_task" ||
		bareAction === "dcc_task_message"
	)
		return "agent";
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
