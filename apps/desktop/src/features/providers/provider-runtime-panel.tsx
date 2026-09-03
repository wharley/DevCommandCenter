import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
	CheckCircle2,
	Download,
	LoaderCircle,
	LogIn,
	RefreshCcw,
} from "lucide-react";
import type { ProviderCatalog } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import {
	connectAntigravity,
	getAntigravityStatus,
	installAntigravity,
} from "@/lib/provider-api";
import { toast } from "sonner";
import {
	SUBAGENT_CONCURRENCY_OPTIONS,
	draftToProviderRuntimeConfig,
	getProviderRuntimeDraft,
	supportsProviderRuntime,
	supportsProviderRuntimeBinary,
	supportsProviderRuntimeHome,
	supportsProviderShadowHome,
	supportsProviderSubagentConcurrency,
	type ProviderRuntimeDraft,
	type ProviderRuntimeSettings,
} from "./provider-runtime-settings";

type ProviderRuntimePanelProps = {
	providers: ProviderCatalog["providers"];
	runtimeSettings: ProviderRuntimeSettings;
	onChangeRuntime: (providerId: string, draft: ProviderRuntimeDraft) => void;
	onClearRuntime: (providerId: string) => void;
	className?: string;
};

function ProviderRuntimeCard({
	provider,
	draft,
	onChangeRuntime,
	onClearRuntime,
}: {
	provider: ProviderCatalog["providers"][number];
	draft: ProviderRuntimeDraft;
	onChangeRuntime: (providerId: string, draft: ProviderRuntimeDraft) => void;
	onClearRuntime: (providerId: string) => void;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [installing, setInstalling] = useState(false);
	const [connecting, setConnecting] = useState(false);
	const antigravityRuntime = useMemo(
		() => draftToProviderRuntimeConfig(draft, provider.capabilities),
		[draft, provider.capabilities],
	);
	const antigravityStatusQuery = useQuery({
		queryKey: ["providers", "antigravity", "status", antigravityRuntime],
		queryFn: () => getAntigravityStatus(antigravityRuntime),
		enabled: provider.id === "antigravity",
		staleTime: 30_000,
	});
	const antigravityStatus = antigravityStatusQuery.data;

	async function installManagedAntigravity() {
		setInstalling(true);
		try {
			const installed = await installAntigravity();
			await Promise.all([
				queryClient.invalidateQueries({ queryKey: ["providers", "catalog"] }),
				queryClient.invalidateQueries({
					queryKey: ["providers", "antigravity", "status"],
				}),
			]);
			toast.success(
				t("settings.model.antigravityInstallSucceeded", {
					version: installed.version,
				}),
			);
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.model.antigravityInstallFailed"),
			);
		} finally {
			setInstalling(false);
		}
	}

	async function signInToAntigravity() {
		setConnecting(true);
		try {
			const connected = await connectAntigravity(antigravityRuntime);
			await Promise.all([
				queryClient.invalidateQueries({ queryKey: ["providers", "catalog"] }),
				queryClient.invalidateQueries({
					queryKey: ["providers", "antigravity", "status"],
				}),
			]);
			toast.success(
				t("settings.model.antigravityConnectSucceeded", {
					count: connected.models.length,
				}),
			);
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.model.antigravityConnectFailed"),
			);
		} finally {
			setConnecting(false);
		}
	}

	return (
		<Card className="rounded-2xl border-border/60 bg-muted/10 p-0 shadow-none">
			<CardHeader className="border-b border-border/40 px-4 py-3">
				<div className="min-w-0 flex-1">
					<div className="flex items-center gap-2">
						<CardTitle className="text-[14px] font-medium leading-tight">
							{provider.label}
						</CardTitle>
						<Badge
							variant={provider.stable ? "success" : "outline"}
							className="h-6 px-2 text-[10px]"
						>
							{provider.id}
						</Badge>
					</div>
					<CardDescription className="mt-1 text-[12px] leading-relaxed">
						{t("settings.model.runtimeCardBody")}
					</CardDescription>
				</div>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="h-8 gap-1.5"
					onClick={() => onClearRuntime(provider.id)}
				>
					<RefreshCcw className="size-3.5" />
					{t("settings.model.runtimeReset")}
				</Button>
			</CardHeader>
			<CardContent className="space-y-3 px-4 py-4">
				{provider.id === "antigravity" ? (
					<div className="space-y-2 rounded-xl border border-border/50 bg-background/60 p-3">
						<div className="space-y-3">
							<div className="min-w-0">
								<p className="text-[12px] font-medium">
									{t("settings.model.antigravitySetupTitle")}
								</p>
								<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
									{t("settings.model.antigravitySetupHint")}
								</p>
								{antigravityStatus ? (
									<div className="mt-2 flex flex-wrap gap-1.5">
										{antigravityStatus.managedRuntimeInstalled ? (
											<Badge variant="success" className="gap-1 text-[10px]">
												<CheckCircle2 className="size-3" aria-hidden />
												{t("settings.model.antigravityRuntimeInstalled", {
													version: antigravityStatus.runtimeVersion,
												})}
											</Badge>
										) : null}
										{antigravityStatus.signedIn ? (
											<Badge variant="success" className="gap-1 text-[10px]">
												<CheckCircle2 className="size-3" aria-hidden />
												{t("settings.model.antigravitySignedIn", {
													count: antigravityStatus.cachedModelCount,
												})}
											</Badge>
										) : null}
									</div>
								) : null}
							</div>
							<div className="flex min-w-0 flex-wrap gap-2">
								<Button
									type="button"
									variant="outline"
									size="sm"
									disabled={
										installing ||
										connecting ||
										antigravityStatusQuery.isPending ||
										antigravityStatus?.managedRuntimeInstalled
									}
									onClick={() => void installManagedAntigravity()}
								>
									{installing ? (
										<LoaderCircle className="size-4 animate-spin" aria-hidden />
									) : antigravityStatus?.managedRuntimeInstalled ? (
										<CheckCircle2 className="size-4" aria-hidden />
									) : (
										<Download className="size-4" aria-hidden />
									)}
									{installing
										? t("settings.model.antigravityInstalling")
										: antigravityStatus?.managedRuntimeInstalled
											? t("settings.model.antigravityInstalled")
											: t("settings.model.antigravityInstall")}
								</Button>
								<Button
									type="button"
									size="sm"
									disabled={installing || connecting}
									onClick={() => void signInToAntigravity()}
								>
									{connecting ? (
										<LoaderCircle className="size-4 animate-spin" aria-hidden />
									) : antigravityStatus?.signedIn ? (
										<RefreshCcw className="size-4" aria-hidden />
									) : (
										<LogIn className="size-4" aria-hidden />
									)}
									{connecting
										? t("settings.model.antigravityConnecting")
										: antigravityStatus?.signedIn
											? t("settings.model.antigravityRefreshModels")
											: t("settings.model.antigravityConnect")}
								</Button>
							</div>
						</div>
					</div>
				) : null}
				{supportsProviderRuntimeBinary(provider.capabilities) ? (
					<div className="space-y-1.5">
						<Label htmlFor={`provider-binary-${provider.id}`} className="text-[12px]">
							{t("settings.model.runtimeBinaryPath")}
						</Label>
						<Input
							id={`provider-binary-${provider.id}`}
							value={draft.binaryPath}
							onChange={(event) =>
								onChangeRuntime(provider.id, {
									...draft,
									binaryPath: event.target.value,
								})
							}
							placeholder={t("settings.model.runtimeBinaryPlaceholder")}
							autoComplete="off"
						/>
						<p className="text-[11px] leading-relaxed text-muted-foreground">
							{t("settings.model.runtimeBinaryHint")}
						</p>
					</div>
				) : null}
				{supportsProviderSubagentConcurrency(provider.capabilities) ? (
					<div className="space-y-1.5 rounded-xl border border-border/50 bg-background/60 p-3">
						<Label
							htmlFor="codex-max-concurrent-subagents"
							className="text-[12px]"
						>
							{t("settings.model.runtimeSubagentConcurrency")}
						</Label>
						<select
							id="codex-max-concurrent-subagents"
							value={draft.maxConcurrentSubagents}
							onChange={(event) =>
								onChangeRuntime(provider.id, {
									...draft,
									maxConcurrentSubagents: event.target.value,
								})
							}
							className="h-9 w-full rounded-md border border-input bg-background px-3 text-[13px] text-foreground shadow-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring"
						>
							<option value="">
								{t("settings.model.runtimeSubagentConcurrencyAuto")}
							</option>
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

				{supportsProviderRuntimeHome(provider.capabilities) ? (
					<div className="space-y-1.5">
						<Label htmlFor={`provider-home-${provider.id}`} className="text-[12px]">
							{t("settings.model.runtimeHomePath")}
						</Label>
						<Input
							id={`provider-home-${provider.id}`}
							value={draft.homePath}
							onChange={(event) =>
								onChangeRuntime(provider.id, {
									...draft,
									homePath: event.target.value,
								})
							}
							placeholder={t("settings.model.runtimeHomePlaceholder")}
							autoComplete="off"
						/>
					</div>
				) : null}

				{supportsProviderShadowHome(provider.capabilities) ? (
					<div className="space-y-1.5">
						<Label htmlFor={`provider-shadow-${provider.id}`} className="text-[12px]">
							{t("settings.model.runtimeShadowHomePath")}
						</Label>
						<Input
							id={`provider-shadow-${provider.id}`}
							value={draft.shadowHomePath}
							onChange={(event) =>
								onChangeRuntime(provider.id, {
									...draft,
									shadowHomePath: event.target.value,
								})
							}
							placeholder={t("settings.model.runtimeShadowHomePlaceholder")}
							autoComplete="off"
						/>
					</div>
				) : null}
			</CardContent>
		</Card>
	);
}

export function ProviderRuntimePanel({
	providers,
	runtimeSettings,
	onChangeRuntime,
	onClearRuntime,
	className,
}: ProviderRuntimePanelProps) {
	const { t } = useTranslation("common");
	const supportedProviders = useMemo(
		() =>
			providers.filter((provider) =>
				supportsProviderRuntime(provider.capabilities),
			),
		[providers],
	);

	return (
		<div
			className={cn(
				"space-y-3 rounded-2xl border border-border/60 bg-background p-4",
				className,
			)}
		>
			<div className="flex items-start justify-between gap-4">
				<div className="min-w-0">
					<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						{t("settings.model.runtimeTitle")}
					</p>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
						{t("settings.model.runtimeHint")}
					</p>
				</div>
				<Badge
					variant="outline"
					className="h-8 px-3 text-[12px] font-normal text-muted-foreground"
				>
					{t("settings.model.runtimeBadge")}
				</Badge>
			</div>

			{supportedProviders.length === 0 ? (
				<div className="rounded-xl border border-dashed border-border/60 bg-muted/10 p-4 text-[12px] text-muted-foreground">
					{t("settings.model.runtimeNoSupportedProviders")}
				</div>
			) : (
				<div className="grid gap-3 xl:grid-cols-2">
					{supportedProviders.map((provider) => (
						<ProviderRuntimeCard
							key={provider.id}
							provider={provider}
							draft={getProviderRuntimeDraft(runtimeSettings, provider.id)}
							onChangeRuntime={onChangeRuntime}
							onClearRuntime={onClearRuntime}
						/>
					))}
				</div>
			)}
		</div>
	);
}
