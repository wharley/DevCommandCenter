import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Folder, GitBranch, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { Repository } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { repositoryDisplayName } from "./repository-display-name";

function normalizedDisplayName(value: string, repository: Repository) {
	const trimmed = value.trim();
	if (!trimmed || trimmed.toLocaleLowerCase() === repository.name.trim().toLocaleLowerCase()) {
		return null;
	}
	return trimmed;
}

export function ProjectEditDialog({
	repository,
	open,
	onOpenChange,
	onSave,
}: {
	repository: Repository | null;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onSave: (repositoryId: string, displayName: string | null) => Promise<void>;
}) {
	const { t } = useTranslation("common");
	const [name, setName] = useState("");
	const [isSaving, setIsSaving] = useState(false);

	useEffect(() => {
		if (open && repository) {
			setName(repositoryDisplayName(repository));
			setIsSaving(false);
		}
	}, [open, repository]);

	const nextDisplayName = repository
		? normalizedDisplayName(name, repository)
		: null;
	const currentDisplayName = repository?.displayName?.trim() || null;
	const isDirty = useMemo(
		() => nextDisplayName !== currentDisplayName,
		[currentDisplayName, nextDisplayName],
	);

	async function handleSubmit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		if (!repository || !isDirty || isSaving) return;
		setIsSaving(true);
		try {
			await onSave(repository.id, nextDisplayName);
			toast.success(t("projectEditor.saved"));
			onOpenChange(false);
		} catch (error) {
			toast.error(t("projectEditor.saveError"), {
				description: error instanceof Error ? error.message : undefined,
			});
		} finally {
			setIsSaving(false);
		}
	}

	return (
		<Dialog open={open} onOpenChange={(nextOpen) => !isSaving && onOpenChange(nextOpen)}>
			<DialogContent
				showCloseButton={!isSaving}
				className="w-[min(calc(100vw-2rem),32rem)] max-w-[32rem] gap-5 p-5"
			>
				<DialogHeader className="space-y-1">
					<DialogTitle className="text-[15px] font-medium tracking-[-0.015em]">
						{t("projectEditor.title")}
					</DialogTitle>
					<DialogDescription className="text-[12px] leading-5">
						{t("projectEditor.description")}
					</DialogDescription>
				</DialogHeader>

				{repository ? (
					<form className="space-y-5" onSubmit={handleSubmit}>
						<div className="space-y-2">
							<div className="flex items-center justify-between gap-3">
								<Label htmlFor="project-display-name" className="text-[12px] font-medium">
									{t("projectEditor.nameLabel")}
								</Label>
								<Button
									type="button"
									variant="ghost"
									size="sm"
									className="h-7 gap-1.5 px-2 text-[11px] text-muted-foreground"
									disabled={name === repository.name || isSaving}
									onClick={() => setName(repository.name)}
								>
									<RotateCcw className="size-3" aria-hidden />
									{t("projectEditor.useRepositoryName")}
								</Button>
							</div>
							<Input
								id="project-display-name"
								value={name}
								autoFocus
								maxLength={80}
								disabled={isSaving}
								placeholder={repository.name}
								onChange={(event) => setName(event.target.value)}
							/>
							<p className="text-[10.5px] leading-4 text-muted-foreground">
								{t("projectEditor.nameHint")}
							</p>
						</div>

						<div className="space-y-2 rounded-xl border border-border/60 bg-muted/15 p-3">
							<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
								{t("projectEditor.technicalIdentity")}
							</p>
							<div className="grid min-w-0 grid-cols-[auto_1fr] items-center gap-x-2.5 gap-y-2 text-[11px]">
								<Folder className="size-3.5 text-muted-foreground" aria-hidden />
								<div className="min-w-0">
									<div className="truncate font-medium text-foreground">{repository.name}</div>
									<div className="truncate font-mono text-[10px] text-muted-foreground">
										{repository.rootPath}
									</div>
								</div>
								<GitBranch className="size-3.5 text-muted-foreground" aria-hidden />
								<span className="truncate">{repository.baseBranch}</span>
							</div>
						</div>

						<DialogFooter className="gap-2 sm:gap-2">
							<Button
								type="button"
								variant="ghost"
								disabled={isSaving}
								onClick={() => onOpenChange(false)}
							>
								{t("projectEditor.cancel")}
							</Button>
							<Button type="submit" disabled={!isDirty || isSaving}>
								{isSaving ? t("projectEditor.saving") : t("projectEditor.save")}
							</Button>
						</DialogFooter>
					</form>
				) : null}
			</DialogContent>
		</Dialog>
	);
}
