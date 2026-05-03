import {
	Check,
	ChevronDown,
	Code2,
	FileCode2,
	PenTool,
	SquareTerminal,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { openInEditor } from "@/lib/shell-api";
import { cn } from "@/lib/utils";
import { toast } from "sonner";

type EditorId = "cursor" | "zed" | "vscode" | "vscode-insiders" | "trae";

type EditorOption = {
	id: EditorId;
	label: string;
	icon: typeof Code2;
};

const EDITOR_STORAGE_KEY = "dcc-preferred-editor";

const EDITOR_OPTIONS: EditorOption[] = [
	{ id: "cursor", label: "Cursor", icon: Code2 },
	{ id: "zed", label: "Zed", icon: SquareTerminal },
	{ id: "vscode", label: "VS Code", icon: FileCode2 },
	{ id: "vscode-insiders", label: "VS Code Insiders", icon: FileCode2 },
	{ id: "trae", label: "Trae", icon: PenTool },
];

function getInitialEditor(): EditorId {
	if (typeof window === "undefined") {
		return "cursor";
	}

	const stored = window.localStorage.getItem(EDITOR_STORAGE_KEY);
	return EDITOR_OPTIONS.some((option) => option.id === stored)
		? (stored as EditorId)
		: "cursor";
}

export function WorkspaceEditorPicker({
	workspacePath,
}: {
	workspacePath: string | null;
}) {
	const { t } = useTranslation("common");
	const [preferredEditor, setPreferredEditor] = useState<EditorId>(getInitialEditor);
	const activeEditor =
		EDITOR_OPTIONS.find((option) => option.id === preferredEditor) ?? EDITOR_OPTIONS[0];

	useEffect(() => {
		if (typeof window === "undefined") {
			return;
		}
		window.localStorage.setItem(EDITOR_STORAGE_KEY, preferredEditor);
	}, [preferredEditor]);

	const openWorkspace = async (editor: EditorId) => {
		if (!workspacePath) {
			return;
		}

		setPreferredEditor(editor);
		try {
			await openInEditor(workspacePath, editor);
		} catch (error) {
			toast.error(
				error instanceof Error ? error.message : `Failed to open workspace in ${editor}`,
			);
		}
	};

	const ActiveIcon = activeEditor.icon;

	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					disabled={!workspacePath}
					className={cn(
						"h-8 shrink-0 gap-1.5 rounded-md px-2 text-[12px] text-muted-foreground hover:text-foreground",
						"[&_svg]:size-3.5",
					)}
					aria-label={
						workspacePath
							? t("workbench.openInEditorAria")
							: t("workbench.openInEditorUnavailable")
					}
					>
					<ActiveIcon strokeWidth={2} />
					<span className="truncate">{activeEditor.label}</span>
					<ChevronDown className="size-3.5 opacity-70" strokeWidth={2} />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end" className="min-w-48">
				<DropdownMenuLabel>{t("workbench.openInEditorMenuLabel")}</DropdownMenuLabel>
				<DropdownMenuSeparator />
				{EDITOR_OPTIONS.map((option) => {
					const Icon = option.icon;
					const selected = option.id === preferredEditor;

					return (
						<DropdownMenuItem
							key={option.id}
							onSelect={() => {
								void openWorkspace(option.id);
							}}
						>
							<Icon className="size-3.5 shrink-0 text-muted-foreground" strokeWidth={2} />
							<span className="flex-1">{option.label}</span>
							{selected ? <Check className="size-3.5 shrink-0" strokeWidth={2} /> : null}
						</DropdownMenuItem>
					);
				})}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
