import { randomUUID } from "node:crypto";

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

export async function handlePermissionRequest(
	toolName,
	input,
	options,
	state,
	emit,
) {
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
