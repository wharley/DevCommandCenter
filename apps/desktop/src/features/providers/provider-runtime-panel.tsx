import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
	AlertTriangle,
	CheckCircle2,
	Download,
	LoaderCircle,
	LogIn,
	RefreshCcw,
} from "lucide-react";
import type { ProviderCatalog } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	connectAntigravity,
	getAntigravityStatus,
	installAntigravity,
} from "@/lib/provider-api";
import { toast } from "sonner";
import {
	SUBAGENT_CONCURRENCY_OPTIONS,
	draftToProviderRuntimeConfig,
	supportsProviderRuntimeBinary,
	supportsProviderRuntimeHome,
	supportsProviderShadowHome,
	supportsProviderSubagentConcurrency,
	type ProviderRuntimeDraft,
} from "./provider-runtime-settings";

type Provider = ProviderCatalog["providers"][number];

/** Antigravity ships a DCC-managed runtime: install it, then sign in. */
export function AntigravitySetup({
	provider,
	draft,
}: {
	provider: Provider;
	draft: ProviderRuntimeDraft;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [installing, setInstalling] = useState(false);
	const [connecting, setConnecting] = useState(false);
	const runtime = useMemo(
		() => draftToProviderRuntimeConfig(draft, provider.capabilities),
		[draft, provider.capabilities],
	);
	const statusQuery = useQuery({
		queryKey: ["providers", "antigravity", "status", runtime],
		queryFn: () => getAntigravityStatus(runtime),
		staleTime: 30_000,
	});
	const status = statusQuery.data;
	// A path set in Advanced replaces the managed runtime, so the badge speaks
	// of the file in use: green only when it can actually run.
	const customPath = draft.binaryPath.trim();
	const customReady = provider.health == null || provider.health === "Healthy";

	async function refreshAfter(action: () => Promise<string>, failure: string) {
		try {
			const message = await action();
			await Promise.all([
				queryClient.invalidateQueries({ queryKey: ["providers", "catalog"] }),
				queryClient.invalidateQueries({ queryKey: ["providers", "antigravity", "status"] }),
			]);
			toast.success(message);
		} catch (error) {
			toast.error(error instanceof Error ? error.message : failure);
		}
	}

	return (
		<div className="space-y-3">
			<p className="text-[12px] leading-relaxed text-muted-foreground">
				{t("settings.model.antigravitySetupHint")}
			</p>
			{customPath || (status && (status.managedRuntimeInstalled || status.signedIn)) ? (
				<div className="flex flex-wrap gap-1.5">
					{customPath ? (
						<Badge
							variant={customReady ? "success" : "warn"}
							className="gap-1 text-[10px]"
							title={customPath}
						>
							{customReady ? (
								<CheckCircle2 className="size-3" aria-hidden />
							) : (
								<AlertTriangle className="size-3" aria-hidden />
							)}
							{customReady
								? t("settings.model.antigravityCustomRuntime")
								: t("settings.model.antigravityCustomRuntimeBroken")}
						</Badge>
					) : status?.managedRuntimeInstalled ? (
						<Badge variant="success" className="gap-1 text-[10px]">
							<CheckCircle2 className="size-3" aria-hidden />
							{t("settings.model.antigravityRuntimeInstalled", {
								version: status.runtimeVersion,
							})}
						</Badge>
					) : null}
					{status?.signedIn ? (
						<Badge variant="success" className="gap-1 text-[10px]">
							<CheckCircle2 className="size-3" aria-hidden />
							{t("settings.model.antigravitySignedIn", {
								count: status.cachedModelCount,
							})}
						</Badge>
					) : null}
				</div>
			) : null}
			<div className="flex flex-wrap gap-2">
				<Button
					type="button"
					variant="outline"
					size="sm"
					className="gap-1.5"
					disabled={
						installing || connecting || statusQuery.isPending || status?.managedRuntimeInstalled
					}
					onClick={async () => {
						setInstalling(true);
						await refreshAfter(async () => {
							const installed = await installAntigravity();
							return t("settings.model.antigravityInstallSucceeded", {
								version: installed.version,
							});
						}, t("settings.model.antigravityInstallFailed"));
						setInstalling(false);
					}}
				>
					{installing ? (
						<LoaderCircle className="size-4 animate-spin" aria-hidden />
					) : status?.managedRuntimeInstalled ? (
						<CheckCircle2 className="size-4" aria-hidden />
					) : (
						<Download className="size-4" aria-hidden />
					)}
					{installing
						? t("settings.model.antigravityInstalling")
						: status?.managedRuntimeInstalled
							? t("settings.model.antigravityInstalled")
							: t("settings.model.antigravityInstall")}
				</Button>
				<Button
					type="button"
					size="sm"
					className="gap-1.5"
					disabled={installing || connecting}
					onClick={async () => {
						setConnecting(true);
						await refreshAfter(async () => {
							const connected = await connectAntigravity(runtime);
							return t("settings.model.antigravityConnectSucceeded", {
								count: connected.models.length,
							});
						}, t("settings.model.antigravityConnectFailed"));
						setConnecting(false);
					}}
				>
					{connecting ? (
						<LoaderCircle className="size-4 animate-spin" aria-hidden />
					) : status?.signedIn ? (
						<RefreshCcw className="size-4" aria-hidden />
					) : (
						<LogIn className="size-4" aria-hidden />
					)}
					{connecting
						? t("settings.model.antigravityConnecting")
						: status?.signedIn
							? t("settings.model.antigravityRefreshModels")
							: t("settings.model.antigravityConnect")}
				</Button>
			</div>
		</div>
	);
}

/** Executable, homes and subagent limit DCC launches the provider with. */
export function ProviderRuntimeFields({
	provider,
	draft,
	onChangeRuntime,
	onClearRuntime,
}: {
	provider: Provider;
	draft: ProviderRuntimeDraft;
	onChangeRuntime: (providerId: string, draft: ProviderRuntimeDraft) => void;
	onClearRuntime: (providerId: string) => void;
}) {
	const { t } = useTranslation("common");
	const customized = Boolean(
		draft.binaryPath.trim() ||
			draft.homePath.trim() ||
			draft.shadowHomePath.trim() ||
			draft.maxConcurrentSubagents,
	);
	const field = (
		key: "binaryPath" | "homePath" | "shadowHomePath",
		label: string,
		placeholder: string,
		hint?: string,
	) => (
		<div className="space-y-1.5">
			<Label htmlFor={`provider-${key}-${provider.id}`} className="text-[12px]">
				{label}
			</Label>
			<Input
				id={`provider-${key}-${provider.id}`}
				value={draft[key]}
				onChange={(event) => onChangeRuntime(provider.id, { ...draft, [key]: event.target.value })}
				placeholder={placeholder}
				autoComplete="off"
				className="h-8 text-[12px]"
			/>
			{hint ? <p className="text-[11px] leading-relaxed text-muted-foreground">{hint}</p> : null}
		</div>
	);

	return (
		<div className="space-y-4">
			{supportsProviderRuntimeBinary(provider.capabilities)
				? field(
						"binaryPath",
						t("settings.model.runtimeBinaryPath"),
						t("settings.model.runtimeBinaryPlaceholder"),
						t("settings.model.runtimeBinaryHint"),
					)
				: null}
			{supportsProviderRuntimeHome(provider.capabilities)
				? field(
						"homePath",
						t("settings.model.runtimeHomePath"),
						t("settings.model.runtimeHomePlaceholder"),
					)
				: null}
			{supportsProviderShadowHome(provider.capabilities)
				? field(
						"shadowHomePath",
						t("settings.model.runtimeShadowHomePath"),
						t("settings.model.runtimeShadowHomePlaceholder"),
					)
				: null}
			{supportsProviderSubagentConcurrency(provider.capabilities) ? (
				<div className="space-y-1.5">
					<Label htmlFor={`provider-subagents-${provider.id}`} className="text-[12px]">
						{t("settings.model.runtimeSubagentConcurrency")}
					</Label>
					<select
						id={`provider-subagents-${provider.id}`}
						value={draft.maxConcurrentSubagents}
						onChange={(event) =>
							onChangeRuntime(provider.id, {
								...draft,
								maxConcurrentSubagents: event.target.value,
							})
						}
						className="h-8 w-full rounded-md border border-input bg-background px-2.5 text-[12px] text-foreground shadow-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring"
					>
						<option value="">{t("settings.model.runtimeSubagentConcurrencyAuto")}</option>
						{SUBAGENT_CONCURRENCY_OPTIONS.map((option) => (
							<option key={option} value={String(option)}>
								{option}
							</option>
						))}
					</select>
					<p className="text-[11px] leading-relaxed text-muted-foreground">
						{t("settings.model.runtimeSubagentConcurrencyHint")}
					</p>
				</div>
			) : null}
			{customized ? (
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="h-7 gap-1.5 px-2 text-[12px] text-muted-foreground"
					onClick={() => onClearRuntime(provider.id)}
				>
					<RefreshCcw className="size-3.5" aria-hidden />
					{t("settings.model.runtimeReset")}
				</Button>
			) : null}
		</div>
	);
}
