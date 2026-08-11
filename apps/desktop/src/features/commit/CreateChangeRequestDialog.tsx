import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

export type CreateChangeRequestInput = {
	title: string;
	body: string;
	includeLocalChanges: boolean;
	draft: boolean;
};

type CreateChangeRequestDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	requestLabel: "PR" | "MR";
	headBranch: string | null;
	baseBranch: string | null;
	defaultTitle: string;
	localFiles: number;
	localAdditions: number;
	localDeletions: number;
	loading?: boolean;
	initialDraft?: boolean;
	onSubmit: (input: CreateChangeRequestInput) => Promise<void>;
};

export function CreateChangeRequestDialog({
	open,
	onOpenChange,
	requestLabel,
	headBranch,
	baseBranch,
	defaultTitle,
	localFiles,
	localAdditions,
	localDeletions,
	loading = false,
	initialDraft = false,
	onSubmit,
}: CreateChangeRequestDialogProps) {
	const { t } = useTranslation("common");
	const [title, setTitle] = useState(defaultTitle);
	const [body, setBody] = useState("");
	const [includeLocalChanges, setIncludeLocalChanges] = useState(localFiles > 0);
	const [draft, setDraft] = useState(initialDraft);

	useEffect(() => {
		if (!open) return;
		setTitle(defaultTitle);
		setBody("");
		setIncludeLocalChanges(localFiles > 0);
		setDraft(initialDraft);
	}, [defaultTitle, initialDraft, localFiles, open]);

	async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
		event.preventDefault();
		if (!title.trim() || loading) return;
		await onSubmit({
			title: title.trim(),
			body: body.trim(),
			includeLocalChanges,
			draft,
		});
	}

	return (
		<Dialog open={open} onOpenChange={(next) => !loading && onOpenChange(next)}>
			<DialogContent className="sm:max-w-lg">
				<form onSubmit={handleSubmit}>
					<DialogHeader>
						<DialogTitle>{t("composer.executionDock.createRequest.title", { requestLabel })}</DialogTitle>
						<DialogDescription>
							{t("composer.executionDock.createRequest.route", {
								head: headBranch ?? "HEAD",
								base: baseBranch ?? "main",
							})}
						</DialogDescription>
					</DialogHeader>

					<div className="grid gap-4 py-4">
						<label className="grid gap-1.5 text-[12px] font-medium">
							{t("composer.executionDock.createRequest.titleLabel")}
							<Input value={title} onChange={(event) => setTitle(event.target.value)} autoFocus />
						</label>
						<label className="grid gap-1.5 text-[12px] font-medium">
							{t("composer.executionDock.createRequest.descriptionLabel")}
							<Textarea
								value={body}
								onChange={(event) => setBody(event.target.value)}
								placeholder={t("composer.executionDock.createRequest.descriptionPlaceholder")}
								className="min-h-24 resize-y"
							/>
						</label>
						{localFiles > 0 ? (
							<label className="flex items-start gap-2 rounded-lg border border-border/60 bg-muted/20 p-3 text-[12px]">
								<input
									type="checkbox"
									checked={includeLocalChanges}
									onChange={(event) => setIncludeLocalChanges(event.target.checked)}
									className="mt-0.5 accent-primary"
								/>
								<span>
									<strong className="block font-medium">{t("composer.executionDock.createRequest.includeLocalChanges")}</strong>
									<span className="mt-1 block text-muted-foreground">
										{t("composer.executionDock.createRequest.localSummary", {
											files: localFiles,
											additions: localAdditions,
											deletions: localDeletions,
										})}
									</span>
								</span>
							</label>
						) : null}
						<label className="flex items-center gap-2 text-[12px] font-medium">
							<input type="checkbox" checked={draft} onChange={(event) => setDraft(event.target.checked)} className="accent-primary" />
							{t("composer.executionDock.createRequest.draft")}
						</label>
					</div>

					<DialogFooter>
						<Button type="button" variant="outline" disabled={loading} onClick={() => onOpenChange(false)}>
							{t("composer.executionDock.createRequest.cancel")}
						</Button>
						<Button type="submit" disabled={loading || !title.trim()}>
							{loading ? t("composer.executionDock.createRequest.creating") : t("composer.executionDock.createRequest.submit", { requestLabel })}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
