import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
	DelegationContextPolicy,
	DelegationMode,
	ProviderCatalog,
} from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

export type ManualDelegationRequest = {
	targetProviderId: string;
	targetProviderIds?: string[];
	targetModelId: string | null;
	mode: Extract<DelegationMode, "review" | "explain" | "implement">;
	contextPolicy: DelegationContextPolicy;
	instruction: string;
};

type DelegationDialogMode = ManualDelegationRequest["mode"];

type ContextPolicyKey = "minimal" | "review_current_diff" | "full_reanchor";

export type DelegationDialogPrefill = {
	nonce: number;
	instruction: string;
	mode?: DelegationDialogMode;
	contextPolicy?: ContextPolicyKey;
	targetProviderId?: string;
	targetModelId?: string;
};

const CONTEXT_POLICY_KEYS: ContextPolicyKey[] = [
	"review_current_diff",
	"full_reanchor",
	"minimal",
];

function contextPolicyFromKey(key: ContextPolicyKey): DelegationContextPolicy {
	switch (key) {
		case "review_current_diff":
			return { type: "review_current_diff" };
		case "full_reanchor":
			return { type: "full_reanchor" };
		case "minimal":
			return { type: "minimal" };
	}
}

export function DelegationDialog({
	open,
	providers,
	isSubmitting,
	prefill,
	onOpenChange,
	onSubmit,
}: {
	open: boolean;
	providers: ProviderCatalog["providers"];
	isSubmitting: boolean;
	prefill?: DelegationDialogPrefill | null;
	onOpenChange: (open: boolean) => void;
	onSubmit: (request: ManualDelegationRequest) => Promise<void>;
}) {
	const { t } = useTranslation("common");
	const delegationProviders = useMemo(
		() =>
			providers.filter(
				(provider) =>
					provider.capabilities.canBeDelegationTarget &&
					provider.capabilities.supportsReadOnlyDelegation,
			),
		[providers],
	);
	const [providerId, setProviderId] = useState("");
	const [providerIds, setProviderIds] = useState<string[]>([]);
	const selectedProvider =
		delegationProviders.find((provider) => provider.id === providerId) ??
		delegationProviders[0] ??
		null;
	const [modelId, setModelId] = useState("");
	const [mode, setMode] = useState<DelegationDialogMode>("review");
	const [contextPolicy, setContextPolicy] =
		useState<ContextPolicyKey>("review_current_diff");
	const [instruction, setInstruction] = useState("");
	const prefillNonce = prefill?.nonce ?? null;

	useEffect(() => {
		if (!open) {
			return;
		}
		const nextProviderId = delegationProviders.some((provider) => provider.id === providerId)
			? providerId
			: (delegationProviders[0]?.id ?? "");
		setProviderId(nextProviderId);
		setProviderIds((current) => {
			const availableIds = new Set(delegationProviders.map((provider) => provider.id));
			const next = current.filter((id) => availableIds.has(id));
			if (nextProviderId && !next.includes(nextProviderId)) {
				next.unshift(nextProviderId);
			}
			return next.length > 0 ? next : nextProviderId ? [nextProviderId] : [];
		});
	}, [delegationProviders, open, providerId]);

	useEffect(() => {
		if (!selectedProvider) {
			setModelId("");
			return;
		}
		setModelId((current) =>
			selectedProvider.models.some((model) => model.id === current)
				? current
				: (selectedProvider.models.find((model) => model.recommended)?.id ??
					selectedProvider.models[0]?.id ??
					""),
		);
	}, [selectedProvider]);

	useEffect(() => {
		if (mode === "implement" && !selectedProvider?.capabilities.supportsEditDelegation) {
			setMode("review");
		}
	}, [mode, selectedProvider]);

	useEffect(() => {
		if (!open || !prefill) {
			return;
		}
		setInstruction(prefill.instruction);
		if (
			prefill.targetProviderId &&
			delegationProviders.some((provider) => provider.id === prefill.targetProviderId)
		) {
			const targetProviderId = prefill.targetProviderId;
			setProviderId(targetProviderId);
			setProviderIds((current) =>
				current.includes(targetProviderId)
					? current
					: [targetProviderId, ...current],
			);
		}
		if (prefill.mode) {
			setMode(prefill.mode);
		}
		if (prefill.contextPolicy) {
			setContextPolicy(prefill.contextPolicy);
		}
	}, [delegationProviders, open, prefill, prefillNonce]);

	useEffect(() => {
		if (
			!open ||
			!prefill?.targetModelId ||
			!selectedProvider?.models.some((model) => model.id === prefill.targetModelId)
		) {
			return;
		}
		setModelId(prefill.targetModelId);
	}, [open, prefill, prefillNonce, selectedProvider]);

	const multiTargetMode = mode !== "implement";
	const selectedTargetProviderIds = useMemo(() => {
		if (!multiTargetMode) {
			return selectedProvider ? [selectedProvider.id] : [];
		}
		const availableIds = new Set(delegationProviders.map((provider) => provider.id));
		const next = providerIds.filter((id) => availableIds.has(id));
		if (selectedProvider && !next.includes(selectedProvider.id)) {
			return [selectedProvider.id, ...next];
		}
		return next;
	}, [delegationProviders, multiTargetMode, providerIds, selectedProvider]);

	const submitDisabled =
		isSubmitting ||
		!selectedProvider ||
		selectedTargetProviderIds.length === 0 ||
		(selectedProvider.models.length > 0 && !modelId) ||
		instruction.trim().length === 0;

	const handleProviderChange = (nextProviderId: string) => {
		setProviderId(nextProviderId);
		setProviderIds((current) =>
			current.includes(nextProviderId) ? current : [nextProviderId, ...current],
		);
	};

	const toggleTargetProvider = (targetId: string, checked: boolean) => {
		const nextSelectedIds = checked
			? providerIds.includes(targetId)
				? providerIds
				: [...providerIds, targetId]
			: providerIds.filter((id) => id !== targetId);
		if (checked && !providerId) {
			setProviderId(targetId);
		} else if (!checked && providerId === targetId) {
			setProviderId(nextSelectedIds[0] ?? "");
		}
		setProviderIds((current) => {
			return checked
				? current.includes(targetId)
					? current
					: [...current, targetId]
				: current.filter((id) => id !== targetId);
		});
	};

	const handleSubmit = async () => {
		if (!selectedProvider || submitDisabled) {
			return;
		}
		await onSubmit({
			targetProviderId: selectedProvider.id,
			targetProviderIds: selectedTargetProviderIds,
			targetModelId: modelId || null,
			mode,
			contextPolicy: contextPolicyFromKey(contextPolicy),
			instruction: instruction.trim(),
		});
		setInstruction("");
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-[34rem]">
				<DialogHeader>
					<DialogTitle>{t("delegation.dialog.title")}</DialogTitle>
					<DialogDescription>{t("delegation.dialog.description")}</DialogDescription>
				</DialogHeader>

				<div className="grid gap-4">
					<div className="grid gap-2">
						<Label>{t("delegation.dialog.mode")}</Label>
						<ToggleGroup
							type="single"
							value={mode}
							onValueChange={(value) => {
								if (
									value === "review" ||
									value === "explain" ||
									(value === "implement" &&
										selectedProvider?.capabilities.supportsEditDelegation)
								) {
									setMode(value);
								}
							}}
							className="justify-start"
						>
							<ToggleGroupItem className="h-8 px-3" value="review">
								{t("delegation.dialog.modeReview")}
							</ToggleGroupItem>
							<ToggleGroupItem className="h-8 px-3" value="explain">
								{t("delegation.dialog.modeExplain")}
							</ToggleGroupItem>
							<ToggleGroupItem
								className="h-8 px-3"
								value="implement"
								disabled={!selectedProvider?.capabilities.supportsEditDelegation}
							>
								{t("delegation.dialog.modeImplement")}
							</ToggleGroupItem>
						</ToggleGroup>
						<p className="text-[11.5px] leading-4 text-muted-foreground">
							{t(`delegation.dialog.modeHint.${mode}`)}
						</p>
					</div>

					<div className="grid gap-2">
						<Label htmlFor="delegate-provider">
							{multiTargetMode
								? t("delegation.dialog.primaryProvider")
								: t("delegation.dialog.provider")}
						</Label>
						<select
							id="delegate-provider"
							value={providerId}
							onChange={(event) => handleProviderChange(event.target.value)}
							className="h-9 rounded-lg border border-input bg-background px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
							disabled={isSubmitting || delegationProviders.length === 0}
						>
							{delegationProviders.map((provider) => (
								<option key={provider.id} value={provider.id}>
									{provider.label}
								</option>
							))}
						</select>
					</div>

					{multiTargetMode && delegationProviders.length > 1 ? (
						<div className="grid gap-2">
							<Label>{t("delegation.dialog.targets")}</Label>
							<div className="grid gap-1.5 rounded-md border border-border/60 bg-muted/10 p-2">
								{delegationProviders.map((provider) => (
									<label
										key={provider.id}
										className="flex min-h-8 items-center gap-2 rounded-sm px-1.5 text-sm"
									>
										<input
											type="checkbox"
											className="size-4 accent-primary"
											checked={selectedTargetProviderIds.includes(provider.id)}
											disabled={isSubmitting}
											onChange={(event) =>
												toggleTargetProvider(provider.id, event.target.checked)
											}
										/>
										<span className="min-w-0 flex-1 truncate">{provider.label}</span>
										{provider.id === selectedProvider?.id ? (
											<Badge
												variant="outline"
												className="h-5 shrink-0 px-1.5 text-[10px] font-normal text-muted-foreground"
											>
												{t("delegation.dialog.primaryBadge")}
											</Badge>
										) : null}
									</label>
								))}
							</div>
							<p className="text-[11.5px] leading-4 text-muted-foreground">
								{t("delegation.dialog.targetsHint")}
							</p>
						</div>
					) : null}

					<div className="grid gap-2">
						<Label htmlFor="delegate-model">{t("delegation.dialog.model")}</Label>
						<select
							id="delegate-model"
							value={modelId}
							onChange={(event) => setModelId(event.target.value)}
							className="h-9 rounded-lg border border-input bg-background px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
							disabled={isSubmitting || !selectedProvider?.models.length}
						>
							{selectedProvider?.models.map((model) => (
								<option key={model.id} value={model.id}>
									{model.label}
								</option>
							))}
						</select>
					</div>

					<div className="grid gap-2">
						<Label htmlFor="delegate-context">{t("delegation.dialog.context")}</Label>
						<select
							id="delegate-context"
							value={contextPolicy}
							onChange={(event) =>
								setContextPolicy(event.target.value as ContextPolicyKey)
							}
							className="h-9 rounded-lg border border-input bg-background px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
							disabled={isSubmitting}
						>
							{CONTEXT_POLICY_KEYS.map((key) => (
								<option key={key} value={key}>
									{t(`delegation.dialog.contextOptions.${key}`)}
								</option>
							))}
						</select>
						<p className="text-[11.5px] leading-4 text-muted-foreground">
							{t(`delegation.dialog.contextHint.${contextPolicy}`)}
						</p>
					</div>

					<div className="grid gap-2">
						<Label htmlFor="delegate-instruction">
							{t("delegation.dialog.instruction")}
						</Label>
						<Textarea
							id="delegate-instruction"
							value={instruction}
							onChange={(event) => setInstruction(event.target.value)}
							placeholder={t("delegation.dialog.instructionPlaceholder")}
							disabled={isSubmitting}
							className="min-h-28"
						/>
					</div>
				</div>

				<DialogFooter>
					<Button
						type="button"
						variant="outline"
						onClick={() => onOpenChange(false)}
						disabled={isSubmitting}
					>
						{t("delegation.dialog.cancel")}
					</Button>
					<Button type="button" onClick={handleSubmit} disabled={submitDisabled}>
						{isSubmitting
							? t("delegation.dialog.submitting")
							: multiTargetMode && selectedTargetProviderIds.length > 1
								? t("delegation.dialog.submitFanOut", {
										count: selectedTargetProviderIds.length,
									})
								: t("delegation.dialog.submit")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
