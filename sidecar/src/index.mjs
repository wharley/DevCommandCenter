#!/usr/bin/env node

import { randomUUID } from "node:crypto";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import readline from "node:readline";

import { query } from "@anthropic-ai/claude-agent-sdk";

const SIDECAR_VERSION = "0.1.27";

function emit(value) {
	process.stdout.write(`${JSON.stringify(value)}\n`);
}

function resolveClaudeBinPath() {
	const override = process.env.DCC_CLAUDE_CODE_BIN_PATH;
	if (override && override.trim().length > 0) {
		return override.trim();
	}

	const require = createRequire(import.meta.url);
	const pkgJson = require.resolve("@anthropic-ai/claude-code/package.json");
	return join(dirname(pkgJson), "bin", "claude.exe");
}

const CLAUDE_BIN_PATH = resolveClaudeBinPath();

function handleAuthStatus() {
	const result = spawnSync(CLAUDE_BIN_PATH, ["auth", "status"], {
		stdio: ["ignore", "pipe", "pipe"],
		env: process.env,
		encoding: "utf8",
	});

	if (result.stdout) {
		process.stdout.write(result.stdout);
	}
	if (result.stderr) {
		process.stderr.write(result.stderr);
	}
	process.exit(result.status ?? 1);
}

function updateResumeSessionId(message, state) {
	if (
		message &&
		message.type === "system" &&
		message.subtype === "init" &&
		typeof message.session_id === "string" &&
		message.session_id.length > 0
	) {
		state.resumeSessionId = message.session_id;
	}
}

function normalizeEffort(effort) {
	if (typeof effort !== "string") {
		return undefined;
	}

	switch (effort.trim().toLowerCase()) {
		case "low":
		case "medium":
		case "high":
		case "xhigh":
		case "max":
			return effort.trim().toLowerCase();
		default:
			return undefined;
	}
}

function buildSystemPrompt(fastMode, toolInstructions) {
	const appendParts = [];
	if (fastMode === true) {
		appendParts.push("Prefer concise assistant replies unless the user explicitly needs detail.");
	}
	if (typeof toolInstructions === "string" && toolInstructions.trim().length > 0) {
		appendParts.push(toolInstructions.trim());
	}

	if (appendParts.length === 0) {
		return undefined;
	}

	return {
		type: "preset",
		preset: "claude_code",
		append: appendParts.join("\n\n"),
	};
}

function normalizeQuestions(input) {
	const rawQuestions = Array.isArray(input?.questions)
		? input.questions
		: typeof input?.question === "string"
			? [
					{
						header: "Question",
						question: input.question,
						options: Array.isArray(input?.options) ? input.options : [],
					},
				]
			: [];

	return rawQuestions.map((question, index) => ({
		id:
			typeof question?.question === "string" && question.question.trim().length > 0
				? question.question.trim()
				: `q-${index + 1}`,
		header:
			typeof question?.header === "string" && question.header.trim().length > 0
				? question.header.trim()
				: `Question ${index + 1}`,
		question:
			typeof question?.question === "string" ? question.question.trim() : "",
		options: Array.isArray(question?.options)
			? question.options
					.map((option) => ({
						label: typeof option?.label === "string" ? option.label.trim() : "",
						description:
							typeof option?.description === "string"
								? option.description.trim()
								: "",
					}))
					.filter((option) => option.label.length > 0)
			: [],
	}));
}

function normalizeAnswerEntries(rawAnswers) {
	if (Array.isArray(rawAnswers)) {
		return rawAnswers
			.map((entry) => ({
				question:
					typeof entry?.question === "string" ? entry.question.trim() : "",
				answer: typeof entry?.answer === "string" ? entry.answer.trim() : "",
			}))
			.filter((entry) => entry.question.length > 0);
	}

	if (!rawAnswers || typeof rawAnswers !== "object") {
		return [];
	}

	return Object.entries(rawAnswers)
		.map(([question, answer]) => ({
			question: question.trim(),
			answer: typeof answer === "string" ? answer.trim() : "",
		}))
		.filter((entry) => entry.question.length > 0);
}

function answersToMap(answers) {
	return Object.fromEntries(
		answers
			.filter((entry) => entry.question.length > 0)
			.map((entry) => [entry.question, entry.answer]),
	);
}

function extractPlanMarkdown(input) {
	if (!input || typeof input !== "object") {
		return null;
	}

	return typeof input.plan === "string" && input.plan.trim().length > 0
		? input.plan.trim()
		: null;
}

function toolInputCommand(input) {
	if (!input || typeof input !== "object") {
		return null;
	}

	const candidates = [
		input.command,
		input.cmd,
		input.script,
		input.shell_command,
		input.shellCommand,
	];
	for (const candidate of candidates) {
		if (typeof candidate === "string" && candidate.trim().length > 0) {
			return candidate.trim();
		}
	}
	return null;
}

function toolInputFile(input) {
	if (!input || typeof input !== "object") {
		return null;
	}

	const candidates = [
		input.file_path,
		input.filePath,
		input.path,
		input.file,
		input.target_file,
		input.targetFile,
	];
	for (const candidate of candidates) {
		if (typeof candidate === "string" && candidate.trim().length > 0) {
			return candidate.trim();
		}
	}
	return null;
}

async function handleAskUserQuestion(input, options, state) {
	const requestId =
		typeof options?.toolUseID === "string" && options.toolUseID.trim().length > 0
			? options.toolUseID.trim()
			: randomUUID();
	const questions = normalizeQuestions(input);
	let aborted = false;

	emit({
		type: "dcc_user_input_request",
		request_id: requestId,
		questions,
	});

	const answers = await new Promise((resolve) => {
		state.pendingUserInputs.set(requestId, { resolve });
		options.signal.addEventListener(
			"abort",
			() => {
				if (!state.pendingUserInputs.delete(requestId)) {
					return;
				}
				aborted = true;
				resolve([]);
			},
			{ once: true },
		);
	});

	state.pendingUserInputs.delete(requestId);

	const normalizedAnswers = normalizeAnswerEntries(answers);
	emit({
		type: "dcc_user_input_resolved",
		request_id: requestId,
		answers: normalizedAnswers,
	});

	if (aborted) {
		return {
			behavior: "deny",
			message: "User input request was cancelled.",
		};
	}

	return {
		behavior: "allow",
		updatedInput: {
			...(input && typeof input === "object" ? input : {}),
			questions: Array.isArray(input?.questions)
				? input.questions
				: questions.map((question) => ({
						header: question.header,
						question: question.question,
						options: question.options,
					})),
			answers: answersToMap(normalizedAnswers),
		},
	};
}

async function handlePermissionRequest(toolName, input, options, state) {
	const requestId =
		typeof options?.toolUseID === "string" && options.toolUseID.trim().length > 0
			? options.toolUseID.trim()
			: randomUUID();
	let aborted = false;

	emit({
		type: "dcc_permission_request",
		request_id: requestId,
		tool_name: typeof toolName === "string" ? toolName : "Tool",
		title:
			typeof options?.title === "string" && options.title.trim().length > 0
				? options.title.trim()
				: null,
		description:
			typeof options?.description === "string" &&
			options.description.trim().length > 0
				? options.description.trim()
				: null,
		command: toolInputCommand(input),
		file: toolInputFile(input),
	});

	const behavior = await new Promise((resolve) => {
		state.pendingPermissions.set(requestId, { resolve });
		options.signal.addEventListener(
			"abort",
			() => {
				if (!state.pendingPermissions.delete(requestId)) {
					return;
				}
				aborted = true;
				resolve("deny");
			},
			{ once: true },
		);
	});

	state.pendingPermissions.delete(requestId);

	emit({
		type: "dcc_permission_resolved",
		request_id: requestId,
		behavior,
	});

	if (aborted || behavior !== "allow") {
		return {
			behavior: "deny",
			message: "User denied tool execution.",
		};
	}

	return {
		behavior: "allow",
		updatedInput: input,
	};
}

async function runTurn(payload, state) {
	const prompt = typeof payload?.prompt === "string" ? payload.prompt : "";
	const permissionMode = payload?.planMode === true ? "plan" : "acceptEdits";
	let additionalDirectories = [];
	try {
		const configuredDirectories = JSON.parse(
			process.env.DCC_ADDITIONAL_DIRECTORIES || "[]",
		);
		if (Array.isArray(configuredDirectories)) {
			additionalDirectories = configuredDirectories.filter(
				(value) => typeof value === "string" && value.trim().length > 0,
			);
		}
	} catch {
		// Rust validates and serializes this value. Ignore malformed manual overrides.
	}
	const q = query({
		prompt,
		options: {
			cwd: process.cwd(),
			additionalDirectories,
			pathToClaudeCodeExecutable: CLAUDE_BIN_PATH,
			model: process.env.DCC_MODEL || undefined,
			...(state.resumeSessionId ? { resume: state.resumeSessionId } : {}),
			permissionMode,
			sandbox: {
				enabled: true,
				failIfUnavailable: true,
				autoAllowBashIfSandboxed: true,
				allowUnsandboxedCommands: false,
				filesystem: {
					allowWrite: [process.cwd(), ...additionalDirectories],
				},
			},
			includePartialMessages: true,
			settingSources: ["user", "project", "local"],
			effort: normalizeEffort(payload?.effort),
			systemPrompt: buildSystemPrompt(payload?.fastMode, payload?.toolInstructions),
			canUseTool: async (toolName, input, options) => {
				if (toolName === "AskUserQuestion") {
					return handleAskUserQuestion(input, options, state);
				}
				if (toolName === "ExitPlanMode") {
					const plan = extractPlanMarkdown(input);
					if (plan) {
						emit({
							type: "dcc_plan_captured",
							tool_use_id: options?.toolUseID ?? null,
							plan,
						});
					}
					return {
						behavior: "deny",
						message:
							"The client captured your proposed plan. Stop here and wait for the user's next turn.",
					};
				}
				return handlePermissionRequest(toolName, input, options, state);
			},
		},
	});
	let sawTerminalResult = false;

	try {
		for await (const message of q) {
			updateResumeSessionId(message, state);
			if (message && message.type === "result") {
				sawTerminalResult = true;
			}
			emit(message);
		}
	} catch (error) {
		if (!sawTerminalResult) {
			emit({
				type: "result",
				is_error: true,
				result: error instanceof Error ? error.message : String(error),
				session_id: state.resumeSessionId ?? null,
			});
		}
	} finally {
		try {
			q.close();
		} catch {
			// Ignore close failures during shutdown.
		}
	}
}

async function main() {
	if (process.argv.includes("--version")) {
		process.stdout.write(`dcc-claude-sidecar ${SIDECAR_VERSION}\n`);
		return;
	}

	if (process.argv.includes("--auth-status")) {
		handleAuthStatus();
		return;
	}

	const state = {
		resumeSessionId: null,
		running: false,
		activeTurnPromise: null,
		pendingUserInputs: new Map(),
		pendingPermissions: new Map(),
	};

	const rl = readline.createInterface({
		input: process.stdin,
		crlfDelay: Infinity,
		terminal: false,
	});

	for await (const rawLine of rl) {
		const line = rawLine.trim();
		if (!line) {
			continue;
		}

		let payload;
		try {
			payload = JSON.parse(line);
		} catch {
			emit({
				type: "result",
				is_error: true,
				result: "invalid Claude sidecar input payload",
				session_id: state.resumeSessionId ?? null,
			});
			continue;
		}

		if (payload.type === "user_input_response") {
			const requestId =
				typeof payload.requestId === "string" ? payload.requestId.trim() : "";
			const pending = requestId ? state.pendingUserInputs.get(requestId) : null;
			if (!pending) {
				emit({
					type: "result",
					is_error: true,
					result: "unknown pending user input request",
					session_id: state.resumeSessionId ?? null,
				});
				continue;
			}
			pending.resolve(normalizeAnswerEntries(payload.answers));
			continue;
		}

		if (payload.type === "permission_response") {
			const requestId =
				typeof payload.requestId === "string" ? payload.requestId.trim() : "";
			const pending = requestId ? state.pendingPermissions.get(requestId) : null;
			if (!pending) {
				emit({
					type: "result",
					is_error: true,
					result: "unknown pending permission request",
					session_id: state.resumeSessionId ?? null,
				});
				continue;
			}
			pending.resolve(
				typeof payload.behavior === "string" && payload.behavior.trim().length > 0
					? payload.behavior.trim()
					: "deny",
			);
			continue;
		}

		if (payload.type !== "input" || typeof payload.prompt !== "string") {
			emit({
				type: "result",
				is_error: true,
				result: "unsupported Claude sidecar message",
				session_id: state.resumeSessionId ?? null,
			});
			continue;
		}

		if (state.running) {
			emit({
				type: "result",
				is_error: true,
				result: "Claude sidecar already has an active turn",
				session_id: state.resumeSessionId ?? null,
			});
			continue;
		}

		state.running = true;
		state.activeTurnPromise = runTurn(payload, state)
			.catch((error) => {
				emit({
					type: "result",
					is_error: true,
					result:
						error instanceof Error ? error.message : String(error),
					session_id: state.resumeSessionId ?? null,
				});
			})
			.finally(() => {
				for (const pending of state.pendingUserInputs.values()) {
					pending.resolve([]);
				}
				state.pendingUserInputs.clear();
				for (const pending of state.pendingPermissions.values()) {
					pending.resolve("deny");
				}
				state.pendingPermissions.clear();
				state.running = false;
				state.activeTurnPromise = null;
			});
	}
}

main().catch((error) => {
	process.stderr.write(
		`[dcc-claude-sidecar] ${error instanceof Error ? error.stack ?? error.message : String(error)}\n`,
	);
	process.exit(1);
});
