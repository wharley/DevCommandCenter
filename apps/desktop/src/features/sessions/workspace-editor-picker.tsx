import { Check, ChevronDown } from "lucide-react";
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
import {
	EDITOR_OPTIONS,
	EDITOR_STORAGE_KEY,
	type EditorId,
	getStoredPreferredEditor,
} from "./workspace-editor-preferences";

export function WorkspaceEditorPicker({
	workspacePath,
}: {
	workspacePath: string | null;
}) {
	const { t } = useTranslation("common");
	const [preferredEditor, setPreferredEditor] = useState<EditorId>(
		getStoredPreferredEditor,
	);
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
	const primaryButtonLabel = workspacePath
		? t("workbench.openInEditorLabel", { editor: activeEditor.label })
		: t("workbench.openInEditorUnavailable");

	return (
		<div className="flex shrink-0 items-center">
			<Button
				type="button"
				variant="ghost"
				size="sm"
				disabled={!workspacePath}
				className={cn(
					"h-8 gap-1.5 rounded-r-none border-r-0 px-2 text-[12px] text-muted-foreground hover:text-foreground",
					"[&_svg]:size-3.5",
				)}
				aria-label={workspacePath ? t("workbench.openInEditorAria") : primaryButtonLabel}
				title={primaryButtonLabel}
				onClick={() => {
					void openWorkspace(preferredEditor);
				}}
			>
				<ActiveIcon strokeWidth={2} />
				<span className="truncate">{activeEditor.label}</span>
			</Button>
			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						disabled={!workspacePath}
						className={cn(
							"h-8 w-7 rounded-l-none border-l-border/40 px-0 text-muted-foreground hover:text-foreground [&_svg]:size-3.5",
						)}
						aria-label={t("workbench.openInEditorMenuLabel")}
					>
						<ChevronDown className="opacity-70" strokeWidth={2} />
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
								<Icon
									className="size-3.5 shrink-0 text-muted-foreground"
									strokeWidth={2}
								/>
								<span className="flex-1">{option.label}</span>
								{selected ? (
									<Check className="size-3.5 shrink-0" strokeWidth={2} />
								) : null}
							</DropdownMenuItem>
						);
					})}
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}
