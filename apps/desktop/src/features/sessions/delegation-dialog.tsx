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
		setProviderId((current) =>
			delegationProviders.some((provider) => provider.id === current)
				? current
				: (delegationProviders[0]?.id ?? ""),
		);
	}, [delegationProviders, open]);

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

	const submitDisabled =
		isSubmitting ||
		!selectedProvider ||
		(selectedProvider.models.length > 0 && !modelId) ||
		instruction.trim().length === 0;

	const handleSubmit = async () => {
		if (!selectedProvider || submitDisabled) {
			return;
		}
		await onSubmit({
			targetProviderId: selectedProvider.id,
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
							onChange={(event) => setProviderId(event.target.value)}
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
