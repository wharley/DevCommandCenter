import type { ComponentType, SVGProps } from "react";
import {
	CursorEditorIcon,
	TraeEditorIcon,
	VsCodeEditorIcon,
	VsCodeInsidersEditorIcon,
	ZedEditorIcon,
} from "@/components/editor-brand-icons";

export type EditorId = "cursor" | "zed" | "vscode" | "vscode-insiders" | "trae";

export type EditorOption = {
	id: EditorId;
	label: string;
	icon: ComponentType<SVGProps<SVGSVGElement>>;
};

export const EDITOR_STORAGE_KEY = "dcc-preferred-editor";

export const EDITOR_OPTIONS: EditorOption[] = [
	{ id: "cursor", label: "Cursor", icon: CursorEditorIcon },
	{ id: "zed", label: "Zed", icon: ZedEditorIcon },
	{ id: "vscode", label: "VS Code", icon: VsCodeEditorIcon },
	{
		id: "vscode-insiders",
		label: "VS Code Insiders",
		icon: VsCodeInsidersEditorIcon,
	},
	{ id: "trae", label: "Trae", icon: TraeEditorIcon },
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
