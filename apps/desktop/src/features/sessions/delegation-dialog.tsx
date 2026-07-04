import { useEffect, useMemo, useState } from "react";
import type {
	DelegationContextPolicy,
	DelegationMode,
	ProviderCatalog,
} from "@dcc/contracts";
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

type ContextPolicyKey = "minimal" | "review_current_diff" | "full_reanchor";

const CONTEXT_POLICY_OPTIONS: Array<{
	value: ContextPolicyKey;
	label: string;
}> = [
	{ value: "review_current_diff", label: "Current diff" },
	{ value: "full_reanchor", label: "Full reanchor" },
	{ value: "minimal", label: "Minimal" },
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
	onOpenChange,
	onSubmit,
}: {
	open: boolean;
	providers: ProviderCatalog["providers"];
	isSubmitting: boolean;
	onOpenChange: (open: boolean) => void;
	onSubmit: (request: ManualDelegationRequest) => Promise<void>;
}) {
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
	const [mode, setMode] =
		useState<Extract<DelegationMode, "review" | "explain" | "implement">>("review");
	const [contextPolicy, setContextPolicy] =
		useState<ContextPolicyKey>("review_current_diff");
	const [instruction, setInstruction] = useState("");

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
					<DialogTitle>Delegate</DialogTitle>
					<DialogDescription>
						Send read-only review or explanation work to another provider.
					</DialogDescription>
				</DialogHeader>

				<div className="grid gap-4">
					<div className="grid gap-2">
						<Label htmlFor="delegate-provider">Provider</Label>
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

					{multiTargetMode ? (
						<div className="grid gap-2">
							<Label>Targets</Label>
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
									</label>
								))}
							</div>
							<p className="text-[11.5px] leading-4 text-muted-foreground">
								Selected model applies to the primary provider. Other targets use
								their recommended model.
							</p>
						</div>
					) : null}

					<div className="grid gap-2">
						<Label htmlFor="delegate-model">Model</Label>
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
						<Label>Mode</Label>
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
								Review
							</ToggleGroupItem>
							<ToggleGroupItem className="h-8 px-3" value="explain">
								Explain
							</ToggleGroupItem>
							<ToggleGroupItem
								className="h-8 px-3"
								value="implement"
								disabled={!selectedProvider?.capabilities.supportsEditDelegation}
							>
								Implement
							</ToggleGroupItem>
						</ToggleGroup>
						{mode === "implement" ? (
							<p className="text-[11.5px] leading-4 text-muted-foreground">
								Allows file edits. Result must be reviewed in Inspector before it is marked complete.
							</p>
						) : null}
					</div>

					<div className="grid gap-2">
						<Label htmlFor="delegate-context">Context</Label>
						<select
							id="delegate-context"
							value={contextPolicy}
							onChange={(event) =>
								setContextPolicy(event.target.value as ContextPolicyKey)
							}
							className="h-9 rounded-lg border border-input bg-background px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
							disabled={isSubmitting}
						>
							{CONTEXT_POLICY_OPTIONS.map((option) => (
								<option key={option.value} value={option.value}>
									{option.label}
								</option>
							))}
						</select>
					</div>

					<div className="grid gap-2">
						<Label htmlFor="delegate-instruction">Instruction</Label>
						<Textarea
							id="delegate-instruction"
							value={instruction}
							onChange={(event) => setInstruction(event.target.value)}
							placeholder="Review the current diff for regressions and missing tests."
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
						Cancel
					</Button>
					<Button type="button" onClick={handleSubmit} disabled={submitDisabled}>
						{isSubmitting ? "Delegating..." : "Delegate"}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
