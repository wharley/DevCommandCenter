import { Code2, FileCode2, PenTool, SquareTerminal } from "lucide-react";

export type EditorId = "cursor" | "zed" | "vscode" | "vscode-insiders" | "trae";

export type EditorOption = {
	id: EditorId;
	label: string;
	icon: typeof Code2;
};

export const EDITOR_STORAGE_KEY = "dcc-preferred-editor";

export const EDITOR_OPTIONS: EditorOption[] = [
	{ id: "cursor", label: "Cursor", icon: Code2 },
	{ id: "zed", label: "Zed", icon: SquareTerminal },
	{ id: "vscode", label: "VS Code", icon: FileCode2 },
	{ id: "vscode-insiders", label: "VS Code Insiders", icon: FileCode2 },
	{ id: "trae", label: "Trae", icon: PenTool },
];

export function getStoredPreferredEditor(): EditorId {
	if (typeof window === "undefined") {
		return "cursor";
	}

	const stored = window.localStorage.getItem(EDITOR_STORAGE_KEY);
	return EDITOR_OPTIONS.some((option) => option.id === stored)
		? (stored as EditorId)
		: "cursor";
}
