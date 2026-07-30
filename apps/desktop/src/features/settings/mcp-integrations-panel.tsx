import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
	ArrowLeft,
	Activity,
	AlertTriangle,
	Braces,
	Command,
	Globe2,
	KeyRound,
	Link2,
	Loader2,
	Plus,
	Power,
	Server,
	ShieldCheck,
	Trash2,
	Unlink,
	X,
} from "lucide-react";
import type {
	McpBindingScope,
	McpIntegrationRecord,
	McpSecretTarget,
	McpToolPolicyDecision,
	ProviderCatalog,
} from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
	activateMcpIntegration,
	createMcpIntegration,
	disableMcpIntegration,
	disconnectMcpOauth,
	listMcpIntegrations,
	removeMcpIntegration,
	setMcpToolPolicy,
} from "@/lib/mcp-api";
import { toast } from "sonner";
import {
	listenMcpRuntimeStatusEvents,
	loadMcpRuntimeStatuses,
	startMcpOauth,
} from "@/lib/session-api";
import { openExternal } from "@/lib/shell-api";
import {
	buildMcpIntegrationInput,
	createMcpIntegrationDraft,
	formatMcpTransportPreview,
	mcpIntegrationNeedsTrust,
	type McpIntegrationDraft,
	type McpIntegrationDraftError,
	type McpScopeContext,
} from "./mcp-integration-form";
import {
	deriveMcpIntegrationRuntimeView,
	findOrphanMcpRuntimeStatuses,
	getMcpToolPolicyDecision,
	listMcpToolAnnotationHints,
	listMcpIntegrationTools,
	type McpIntegrationRuntimeKind,
} from "./mcp-integration-runtime";

export const MCP_INTEGRATIONS_QUERY_KEY = ["mcp", "integrations"] as const;

type McpIntegrationsPanelProps = {
	projectId: string | null;
	sessionId: string | null;
	sessionProviderId: string | null;
	sessionCreatedAt: string | null;
	workspaceName: string | null;
	providerCatalog: ProviderCatalog | null;
};

type PanelView =
	| { type: "list" }
	| { type: "create" }
	| { type: "review"; integration: McpIntegrationRecord };

type ProviderSupportKind =
	| "verified"
	| "runtime"
	| "native"
	| "unsupported"
	| "unknown";

function providerSupportKind(
	support: ProviderCatalog["providers"][number]["capabilities"]["mcpSupport"] | null,
): ProviderSupportKind {
	if (support && typeof support === "object") {
		if ("verifiedBridge" in support) return "verified";
		if ("runtimeBridge" in support) return "runtime";
	}
	if (support === "nativeConfig") return "native";
	if (support === "unsupported") return "unsupported";
	return "unknown";
}

function providerRuntimeVersion(
	support: ProviderCatalog["providers"][number]["capabilities"]["mcpSupport"] | null,
): string | null {
	if (support && typeof support === "object" && "runtimeBridge" in support) {
		return support.runtimeBridge?.providerVersion ?? null;
	}
	return null;
}

function scopeTarget(scope: McpBindingScope): string | null {
	if (scope.type === "session") return scope.sessionId;
	if (scope.type === "project") return scope.projectId;
	return null;
}

function credentialTargetLabel(target: McpSecretTarget): string {
	return target.name;
}

function integrationScope(integration: McpIntegrationRecord): McpBindingScope {
	return integration.bindings[0]?.scope ?? { type: "global" };
}

function scopeLabelKey(
	scope: McpBindingScope,
):
	| "settings.integrations.scopeSession"
	| "settings.integrations.scopeProject"
	| "settings.integrations.scopeGlobal" {
	if (scope.type === "session") return "settings.integrations.scopeSession";
	if (scope.type === "project") return "settings.integrations.scopeProject";
	return "settings.integrations.scopeGlobal";
}

function errorMessageKey(error: McpIntegrationDraftError): string {
	return `settings.integrations.errors.${error}`;
}

function runtimeStatusVariant(
	kind: McpIntegrationRuntimeKind,
): "success" | "warn" | "destructive" | "outline" | "secondary" {
	if (kind === "connected" || kind === "serverReachable") return "success";
	if (kind === "failed") return "destructive";
	if (
		kind === "restartRequired" ||
		kind === "needsTrust" ||
		kind === "probingServer" ||
		kind === "attachingProvider"
	) {
		return "warn";
	}
	if (kind === "disabled") return "secondary";
	return "outline";
}

export function McpIntegrationsPanel({
	projectId,
	sessionId,
	sessionProviderId,
	sessionCreatedAt,
	workspaceName,
	providerCatalog,
}: McpIntegrationsPanelProps) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const scopeContext = useMemo<McpScopeContext>(
		() => ({ projectId, sessionId }),
		[projectId, sessionId],
	);
	const [view, setView] = useState<PanelView>({ type: "list" });
	const [draft, setDraft] = useState<McpIntegrationDraft>(() =>
		createMcpIntegrationDraft(scopeContext),
	);
	const [busyAction, setBusyAction] = useState<string | null>(null);
	const [removeTarget, setRemoveTarget] = useState<McpIntegrationRecord | null>(
		null,
	);
	const [deleteCredentials, setDeleteCredentials] = useState(false);

	const integrationsQuery = useQuery({
		queryKey: MCP_INTEGRATIONS_QUERY_KEY,
		queryFn: listMcpIntegrations,
		staleTime: 5_000,
		refetchInterval: 5_000,
		refetchOnWindowFocus: true,
	});
	const integrations = integrationsQuery.data?.integrations ?? [];
	const runtimeQueryKey = useMemo(
		() => ["mcp", "runtime-statuses", sessionId] as const,
		[sessionId],
	);
	const runtimeQuery = useQuery({
		queryKey: runtimeQueryKey,
		queryFn: () =>
			sessionId
				? loadMcpRuntimeStatuses(sessionId)
				: Promise.resolve({ statuses: [] }),
		enabled: Boolean(sessionId),
		staleTime: 3_000,
		refetchInterval: sessionId ? 5_000 : false,
		refetchOnWindowFocus: true,
	});
	const runtimeStatuses = runtimeQuery.data?.statuses ?? [];
	const sessionProvider =
		providerCatalog?.providers.find(
			(provider) => provider.id === sessionProviderId,
		) ?? null;
	const providerSupport = sessionProvider?.capabilities.mcpSupport ?? null;
	const selectedProviderSupportKind = providerSupportKind(providerSupport);
	const selectedProviderRuntimeVersion =
		providerRuntimeVersion(providerSupport);
	const orphanRuntimeStatuses = useMemo(
		() => findOrphanMcpRuntimeStatuses(integrations, runtimeStatuses),
		[integrations, runtimeStatuses],
	);

	useEffect(() => {
		if (!sessionId) return;
		let disposed = false;
		let unlisten: (() => void) | null = null;
		void listenMcpRuntimeStatusEvents((event) => {
			if (event.sessionId === sessionId) {
				queryClient.setQueryData(runtimeQueryKey, {
					statuses: event.statuses,
				});
			}
		})
			.then((dispose) => {
				if (disposed) {
					void dispose();
				} else {
					unlisten = dispose;
				}
			})
			.catch(() => undefined);
		return () => {
			disposed = true;
			if (unlisten) void unlisten();
		};
	}, [queryClient, runtimeQueryKey, sessionId]);

	const refresh = async () => {
		await queryClient.invalidateQueries({ queryKey: MCP_INTEGRATIONS_QUERY_KEY });
	};

	const openCreate = () => {
		setDraft(createMcpIntegrationDraft(scopeContext));
		setView({ type: "create" });
	};

	const updateDraft = <Key extends keyof McpIntegrationDraft>(
		key: Key,
		value: McpIntegrationDraft[Key],
	) => {
		setDraft((current) => ({ ...current, [key]: value }));
	};

	const addCredential = () => {
		setDraft((current) => ({
			...current,
			credentials: [
				...current.credentials,
				{
					id: `credential-${Date.now()}-${current.credentials.length}`,
					name: "",
					secret: "",
				},
			],
		}));
	};

	const updateCredential = (
		id: string,
		key: "name" | "secret",
		value: string,
	) => {
		setDraft((current) => ({
			...current,
			credentials: current.credentials.map((credential) =>
				credential.id === id ? { ...credential, [key]: value } : credential,
			),
		}));
	};

	const removeCredential = (id: string) => {
		setDraft((current) => ({
			...current,
			credentials: current.credentials.filter(
				(credential) => credential.id !== id,
			),
		}));
	};

	const handleCreate = async () => {
		const result = buildMcpIntegrationInput(draft, scopeContext);
		if (!result.ok) {
			toast.error(t(errorMessageKey(result.error)));
			return;
		}

		setBusyAction("create");
		try {
			const output = await createMcpIntegration(result.input);
			setDraft(createMcpIntegrationDraft(scopeContext));
			setView({ type: "review", integration: output.integration });
			await refresh();
			toast.success(t("settings.integrations.created"));
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.integrations.errors.create"),
			);
		} finally {
			setBusyAction(null);
		}
	};

	const handleActivate = async (integration: McpIntegrationRecord) => {
		setBusyAction(`activate:${integration.definition.id}`);
		try {
			await activateMcpIntegration({
				definitionId: integration.definition.id,
				expectedFingerprint:
					integration.definition.trust.currentFingerprint,
			});
			await refresh();
			setView({ type: "list" });
			toast.success(t("settings.integrations.enabledToast"));
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.integrations.errors.activate"),
			);
		} finally {
			setBusyAction(null);
		}
	};

	const handleDisable = async (integration: McpIntegrationRecord) => {
		setBusyAction(`disable:${integration.definition.id}`);
		try {
			await disableMcpIntegration({
				definitionId: integration.definition.id,
			});
			await refresh();
			toast.success(t("settings.integrations.disabledToast"));
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.integrations.errors.disable"),
			);
		} finally {
			setBusyAction(null);
		}
	};

	const handleRemove = async () => {
		if (!removeTarget) return;
		const target = removeTarget;
		setBusyAction(`remove:${target.definition.id}`);
		try {
			await removeMcpIntegration({
				definitionId: target.definition.id,
				deleteCredentials,
			});
			setRemoveTarget(null);
			setDeleteCredentials(false);
			await refresh();
			toast.success(t("settings.integrations.removedToast"));
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.integrations.errors.remove"),
			);
		} finally {
			setBusyAction(null);
		}
	};

	const handleToolPolicy = async (
		integration: McpIntegrationRecord,
		toolName: string,
		decision: McpToolPolicyDecision,
	) => {
		const action = `policy:${integration.definition.id}:${toolName}:${decision}`;
		setBusyAction(action);
		try {
			await setMcpToolPolicy({
				definitionId: integration.definition.id,
				toolName,
				decision,
			});
			await refresh();
			toast.success(t("settings.integrations.toolPolicyUpdated"));
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.integrations.errors.toolPolicy"),
			);
		} finally {
			setBusyAction(null);
		}
	};

	const handleMcpOauth = async (integration: McpIntegrationRecord) => {
		if (!sessionId) return;
		const action = `oauth:${integration.definition.id}`;
		setBusyAction(action);
		try {
			const result = await startMcpOauth({
				sessionId,
				definitionId: integration.definition.id,
			});
			await openExternal(result.authorizationUrl);
			toast.success(t("settings.integrations.oauthOpened"));
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.integrations.errors.oauth"),
			);
		} finally {
			setBusyAction(null);
		}
	};

	const handleDisconnectMcpOauth = async (
		integration: McpIntegrationRecord,
	) => {
		if (!sessionProviderId) return;
		const action = `oauth-disconnect:${integration.definition.id}`;
		setBusyAction(action);
		try {
			await disconnectMcpOauth({
				definitionId: integration.definition.id,
				providerId: sessionProviderId,
			});
			await queryClient.invalidateQueries({
				queryKey: MCP_INTEGRATIONS_QUERY_KEY,
			});
			toast.success(t("settings.integrations.oauthDisconnected"));
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.integrations.errors.oauthDisconnect"),
			);
		} finally {
			setBusyAction(null);
		}
	};

	if (view.type === "create") {
		const isHttp = draft.transport === "http";
		return (
			<section className="space-y-4">
				<div className="flex items-start justify-between gap-4">
					<div>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="-ml-2 mb-2"
							onClick={() => setView({ type: "list" })}
						>
							<ArrowLeft className="size-3.5" />
							{t("settings.integrations.back")}
						</Button>
						<h3 className="text-[14px] font-medium text-foreground">
							{t("settings.integrations.createTitle")}
						</h3>
						<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
							{t("settings.integrations.createHint")}
						</p>
					</div>
					<Badge variant="outline">
						{t("settings.integrations.disabledUntilReview")}
					</Badge>
				</div>

				<div className="space-y-5 rounded-xl border border-border/60 bg-muted/10 p-4">
					<div className="grid gap-2">
						<Label htmlFor="mcp-display-name">
							{t("settings.integrations.nameLabel")}
						</Label>
						<Input
							id="mcp-display-name"
							value={draft.displayName}
							onChange={(event) =>
								updateDraft("displayName", event.target.value)
							}
							placeholder={t("settings.integrations.namePlaceholder")}
							autoComplete="off"
						/>
					</div>

					<div className="grid gap-2">
						<Label>{t("settings.integrations.transportLabel")}</Label>
						<ToggleGroup
							type="single"
							value={draft.transport}
							onValueChange={(value) => {
								if (value === "http" || value === "stdio") {
									setDraft((current) => ({
										...current,
										transport: value,
										credentials: [],
									}));
								}
							}}
							className="grid grid-cols-2 gap-2"
						>
							<ToggleGroupItem
								value="http"
								className="h-auto justify-start gap-2 rounded-lg border border-border/60 px-3 py-2 text-left"
							>
								<Globe2 className="size-4" />
								<span>
									<span className="block text-[12px] font-medium">
										{t("settings.integrations.http")}
									</span>
									<span className="block text-[10px] text-muted-foreground">
										{t("settings.integrations.httpHint")}
									</span>
								</span>
							</ToggleGroupItem>
							<ToggleGroupItem
								value="stdio"
								className="h-auto justify-start gap-2 rounded-lg border border-border/60 px-3 py-2 text-left"
							>
								<Command className="size-4" />
								<span>
									<span className="block text-[12px] font-medium">
										{t("settings.integrations.stdio")}
									</span>
									<span className="block text-[10px] text-muted-foreground">
										{t("settings.integrations.stdioHint")}
									</span>
								</span>
							</ToggleGroupItem>
						</ToggleGroup>
					</div>

					{isHttp ? (
						<div className="grid gap-2">
							<Label htmlFor="mcp-url">
								{t("settings.integrations.urlLabel")}
							</Label>
							<Input
								id="mcp-url"
								type="url"
								value={draft.url}
								onChange={(event) => updateDraft("url", event.target.value)}
								placeholder="https://mcp.example.com/rpc"
								autoComplete="off"
							/>
							<p className="text-[11px] leading-relaxed text-muted-foreground">
								{t("settings.integrations.urlHint")}
							</p>
						</div>
					) : (
						<div className="grid gap-4">
							<div className="grid gap-2">
								<Label htmlFor="mcp-executable">
									{t("settings.integrations.executableLabel")}
								</Label>
								<Input
									id="mcp-executable"
									value={draft.executable}
									onChange={(event) =>
										updateDraft("executable", event.target.value)
									}
									placeholder="/usr/local/bin/mcp-server"
									autoComplete="off"
								/>
								<p className="text-[11px] leading-relaxed text-muted-foreground">
									{t("settings.integrations.executableHint")}
								</p>
							</div>
							<div className="grid gap-2">
								<Label htmlFor="mcp-args">
									{t("settings.integrations.argumentsLabel")}
								</Label>
								<Textarea
									id="mcp-args"
									value={draft.argsText}
									onChange={(event) =>
										updateDraft("argsText", event.target.value)
									}
									placeholder={"--transport\nstdio"}
									className="min-h-20 font-mono text-[12px]"
								/>
								<p className="text-[11px] leading-relaxed text-muted-foreground">
									{t("settings.integrations.argumentsHint")}
								</p>
							</div>
							<div className="grid gap-2">
								<Label htmlFor="mcp-cwd">
									{t("settings.integrations.cwdLabel")}
								</Label>
								<Input
									id="mcp-cwd"
									value={draft.cwd}
									onChange={(event) => updateDraft("cwd", event.target.value)}
									placeholder={t("settings.integrations.optional")}
									autoComplete="off"
								/>
							</div>
						</div>
					)}

					<div className="grid gap-2">
						<Label>{t("settings.integrations.scopeLabel")}</Label>
						<ToggleGroup
							type="single"
							value={draft.scope}
							onValueChange={(value) => {
								if (
									value === "session" ||
									value === "project" ||
									value === "global"
								) {
									updateDraft("scope", value);
								}
							}}
							className="grid grid-cols-3 gap-2"
						>
							<ToggleGroupItem
								value="session"
								disabled={!sessionId}
								className="h-8 rounded-lg border border-border/60 px-2 text-[11px]"
							>
								{t("settings.integrations.scopeSession")}
							</ToggleGroupItem>
							<ToggleGroupItem
								value="project"
								disabled={!projectId}
								className="h-8 rounded-lg border border-border/60 px-2 text-[11px]"
							>
								{t("settings.integrations.scopeProject")}
							</ToggleGroupItem>
							<ToggleGroupItem
								value="global"
								className="h-8 rounded-lg border border-border/60 px-2 text-[11px]"
							>
								{t("settings.integrations.scopeGlobal")}
							</ToggleGroupItem>
						</ToggleGroup>
						<p className="text-[11px] leading-relaxed text-muted-foreground">
							{draft.scope === "session"
								? t("settings.integrations.scopeSessionHint")
								: draft.scope === "project"
									? t("settings.integrations.scopeProjectHint", {
											workspace: workspaceName ?? t("settings.currentWorkspace"),
										})
									: t("settings.integrations.scopeGlobalHint")}
						</p>
					</div>

					<div className="space-y-3 border-t border-border/50 pt-4">
						<div className="flex items-start justify-between gap-4">
							<div>
								<Label>{t("settings.integrations.credentialsLabel")}</Label>
								<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
									{isHttp
										? t("settings.integrations.httpCredentialsHint")
										: t("settings.integrations.stdioCredentialsHint")}
								</p>
							</div>
							<Button
								type="button"
								variant="outline"
								size="sm"
								onClick={addCredential}
							>
								<Plus className="size-3.5" />
								{t("settings.integrations.addCredential")}
							</Button>
						</div>

						{draft.credentials.length === 0 ? (
							<div className="rounded-lg border border-dashed border-border/60 px-3 py-3 text-[11px] text-muted-foreground">
								{t("settings.integrations.noCredentials")}
							</div>
						) : (
							<div className="space-y-2">
								{draft.credentials.map((credential) => (
									<div
										key={credential.id}
										className="grid gap-2 rounded-lg border border-border/50 bg-background p-3 sm:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)_auto]"
									>
										<Input
											value={credential.name}
											onChange={(event) =>
												updateCredential(
													credential.id,
													"name",
													event.target.value,
												)
											}
											placeholder={
												isHttp ? "Authorization" : "SERVICE_API_KEY"
											}
											aria-label={t(
												"settings.integrations.credentialNameLabel",
											)}
											autoComplete="off"
										/>
										<Input
											type="password"
											value={credential.secret}
											onChange={(event) =>
												updateCredential(
													credential.id,
													"secret",
													event.target.value,
												)
											}
											placeholder={t(
												"settings.integrations.credentialValuePlaceholder",
											)}
											aria-label={t(
												"settings.integrations.credentialValueLabel",
											)}
											autoComplete="new-password"
										/>
										<Button
											type="button"
											variant="ghost"
											size="icon"
											onClick={() => removeCredential(credential.id)}
											aria-label={t("settings.integrations.removeCredential")}
										>
											<X className="size-3.5" />
										</Button>
									</div>
								))}
							</div>
						)}
					</div>
				</div>

				<div className="flex items-center justify-end gap-2">
					<Button
						type="button"
						variant="ghost"
						onClick={() => setView({ type: "list" })}
						disabled={busyAction === "create"}
					>
						{t("settings.integrations.cancel")}
					</Button>
					<Button
						type="button"
						onClick={() => void handleCreate()}
						disabled={busyAction === "create"}
					>
						{busyAction === "create" ? (
							<Loader2 className="size-3.5 animate-spin" />
						) : (
							<ShieldCheck className="size-3.5" />
						)}
						{t("settings.integrations.review")}
					</Button>
				</div>
			</section>
		);
	}

	if (view.type === "review") {
		const { integration } = view;
		const { definition } = integration;
		const scope = integrationScope(integration);
		const needsTrust = mcpIntegrationNeedsTrust(integration);
		const activating = busyAction === `activate:${definition.id}`;
		return (
			<section className="space-y-4">
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="-ml-2"
					onClick={() => setView({ type: "list" })}
				>
					<ArrowLeft className="size-3.5" />
					{t("settings.integrations.back")}
				</Button>

				<div className="rounded-xl border border-[color-mix(in_oklab,var(--warning)_45%,var(--border))] bg-[color-mix(in_oklab,var(--warning)_8%,transparent)] p-4">
					<div className="flex items-start gap-3">
						<div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-[color-mix(in_oklab,var(--warning)_16%,transparent)]">
							<ShieldCheck className="size-4" />
						</div>
						<div>
							<h3 className="text-[14px] font-medium text-foreground">
								{needsTrust
									? t("settings.integrations.reviewTrustTitle")
									: t("settings.integrations.reviewEnableTitle")}
							</h3>
							<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
								{t("settings.integrations.reviewTrustHint")}
							</p>
						</div>
					</div>
				</div>

				<div className="space-y-4 rounded-xl border border-border/60 p-4">
					<div className="flex flex-wrap items-start justify-between gap-3">
						<div>
							<div className="text-[14px] font-medium text-foreground">
								{definition.displayName}
							</div>
							<div className="mt-1 flex flex-wrap gap-1.5">
								<Badge variant="outline">
									{definition.transport.type === "http"
										? t("settings.integrations.http")
										: t("settings.integrations.stdio")}
								</Badge>
								<Badge variant="outline">
									{t(scopeLabelKey(scope))}
								</Badge>
								<Badge variant="warn">
									{t("settings.integrations.awaitingConfirmation")}
								</Badge>
							</div>
						</div>
						{integration.credentialCount > 0 ? (
							<Badge variant="secondary">
								<KeyRound className="size-3" />
								{t("settings.integrations.credentialCount", {
									count: integration.credentialCount,
								})}
							</Badge>
						) : null}
					</div>

					<div>
						<div className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							{definition.transport.type === "http"
								? t("settings.integrations.destination")
								: t("settings.integrations.exactCommand")}
						</div>
						<pre className="mt-2 whitespace-pre-wrap break-all rounded-lg border border-border/50 bg-muted/20 p-3 font-mono text-[12px] leading-relaxed text-foreground">
							{formatMcpTransportPreview(definition.transport)}
						</pre>
						{definition.transport.type === "stdio" &&
						definition.transport.cwd ? (
							<p className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
								{t("settings.integrations.cwdPreview", {
									cwd: definition.transport.cwd,
								})}
							</p>
						) : null}
					</div>

					{definition.secretRefs && definition.secretRefs.length > 0 ? (
						<div>
							<div className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								{t("settings.integrations.secretDestinations")}
							</div>
							<div className="mt-2 flex flex-wrap gap-1.5">
								{definition.secretRefs.map((secretRef) => (
									<Badge
										key={`${secretRef.target.type}:${secretRef.target.name}`}
										variant="outline"
										className="font-mono"
									>
										{credentialTargetLabel(secretRef.target)}
									</Badge>
								))}
							</div>
						</div>
					) : null}

					<div>
						<div className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							{t("settings.integrations.fingerprint")}
						</div>
						<p className="mt-2 break-all font-mono text-[10px] leading-relaxed text-muted-foreground">
							{definition.trust.currentFingerprint}
						</p>
					</div>
				</div>

				<div className="flex items-center justify-end gap-2">
					<Button
						type="button"
						variant="ghost"
						onClick={() => setView({ type: "list" })}
						disabled={activating}
					>
						{t("settings.integrations.keepDisabled")}
					</Button>
					<Button
						type="button"
						onClick={() => void handleActivate(integration)}
						disabled={activating}
					>
						{activating ? (
							<Loader2 className="size-3.5 animate-spin" />
						) : (
							<Power className="size-3.5" />
						)}
						{t("settings.integrations.confirmEnable")}
					</Button>
				</div>
			</section>
		);
	}

	return (
		<section className="space-y-4">
			<div className="flex flex-col gap-3 rounded-xl border border-border/60 bg-muted/15 p-4 sm:flex-row sm:items-start sm:justify-between">
				<div className="flex min-w-0 items-start gap-3">
					<div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
						<Braces className="size-4" />
					</div>
					<div>
						<h3 className="text-[14px] font-medium text-foreground">
							{t("settings.integrations.title")}
						</h3>
						<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
							{t("settings.integrations.hint")}
						</p>
						<p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">
							{t("settings.integrations.providerCopy")}
						</p>
					</div>
				</div>
				<Button type="button" size="sm" onClick={openCreate}>
					<Plus className="size-3.5" />
					{t("settings.integrations.add")}
				</Button>
			</div>

			<div className="rounded-xl border border-border/60 bg-background p-4">
				<div className="flex flex-wrap items-start justify-between gap-3">
					<div className="flex min-w-0 items-start gap-3">
						<div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground">
							<Activity className="size-4" />
						</div>
						<div>
							<h3 className="text-[13px] font-medium text-foreground">
								{t("settings.integrations.compatibilityTitle")}
							</h3>
							<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
								{sessionId && sessionProviderId
									? t(
											`settings.integrations.compatibility.${selectedProviderSupportKind}`,
											{
												provider:
													sessionProvider?.label ?? sessionProviderId,
												version: selectedProviderRuntimeVersion ?? "",
											},
										)
									: t("settings.integrations.compatibility.noSession")}
							</p>
						</div>
					</div>
					{sessionId && sessionProviderId ? (
						<div className="flex flex-wrap items-center gap-1.5">
							<Badge variant="outline">
								{sessionProvider?.label ?? sessionProviderId}
							</Badge>
							<Badge
								variant={
									selectedProviderSupportKind === "verified"
										? "success"
										: selectedProviderSupportKind === "runtime"
											? "warn"
										: selectedProviderSupportKind === "unsupported"
											? "destructive"
											: "outline"
								}
							>
								{t(
									`settings.integrations.compatibilityBadge.${selectedProviderSupportKind}`,
								)}
							</Badge>
						</div>
					) : null}
				</div>
				{sessionId ? (
					<div className="mt-3 border-t border-border/50 pt-3 text-[11px] leading-relaxed text-muted-foreground">
						<p>
							{runtimeQuery.isPending
								? t("settings.integrations.runtimeLoading")
								: runtimeQuery.isError
									? t("settings.integrations.runtimeLoadError")
									: t("settings.integrations.runtimeSnapshot", {
											count: runtimeStatuses.length,
										})}
						</p>
						<p className="mt-1.5 text-[10px]">
							{t("settings.integrations.processLifecycleHint")}
						</p>
					</div>
				) : null}
			</div>

			{orphanRuntimeStatuses.length > 0 ? (
				<div className="rounded-xl border border-[color-mix(in_oklab,var(--warning)_45%,var(--border))] bg-[color-mix(in_oklab,var(--warning)_8%,transparent)] p-4">
					<div className="flex items-start gap-3">
						<AlertTriangle className="mt-0.5 size-4 shrink-0 text-[var(--warning)]" />
						<div>
							<p className="text-[12px] font-medium text-foreground">
								{t("settings.integrations.orphanRuntimeTitle", {
									count: orphanRuntimeStatuses.length,
								})}
							</p>
							<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
								{t("settings.integrations.orphanRuntimeHint")}
							</p>
						</div>
					</div>
				</div>
			) : null}

			{integrationsQuery.isPending ? (
				<div className="flex items-center justify-center gap-2 rounded-xl border border-border/60 py-10 text-[12px] text-muted-foreground">
					<Loader2 className="size-4 animate-spin" />
					{t("settings.integrations.loading")}
				</div>
			) : integrationsQuery.isError ? (
				<div className="rounded-xl border border-destructive/30 bg-destructive/5 p-4">
					<p className="text-[12px] text-destructive">
						{t("settings.integrations.errors.loading")}
					</p>
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="mt-3"
						onClick={() => void integrationsQuery.refetch()}
					>
						{t("settings.integrations.retry")}
					</Button>
				</div>
			) : integrations.length === 0 ? (
				<div className="rounded-xl border border-dashed border-border/60 px-5 py-10 text-center">
					<Server className="mx-auto size-6 text-muted-foreground" />
					<h3 className="mt-3 text-[13px] font-medium text-foreground">
						{t("settings.integrations.emptyTitle")}
					</h3>
					<p className="mx-auto mt-1 max-w-md text-[12px] leading-relaxed text-muted-foreground">
						{t("settings.integrations.emptyHint")}
					</p>
					<Button type="button" size="sm" className="mt-4" onClick={openCreate}>
						<Plus className="size-3.5" />
						{t("settings.integrations.add")}
					</Button>
				</div>
			) : (
				<div className="space-y-3">
					{integrations.map((integration) => {
						const { definition } = integration;
						const scope = integrationScope(integration);
						const target = scopeTarget(scope);
						const needsTrust = mcpIntegrationNeedsTrust(integration);
						const runtimeView = deriveMcpIntegrationRuntimeView(
							integration,
							runtimeStatuses,
							{
								projectId,
								sessionId,
								sessionCreatedAt,
								providerId: sessionProviderId,
								providerSupport,
							},
						);
						const reportedStatus = runtimeView.status;
						const needsOauth =
							sessionProviderId === "codex" &&
							reportedStatus?.state === "failed" &&
							reportedStatus.boundedError?.category === "authentication";
						const oauthBusy =
							busyAction === `oauth:${definition.id}`;
						const hasManagedOauth =
							sessionProviderId !== null &&
							integration.oauthProviderIds.some(
								(providerId) => providerId === sessionProviderId,
							);
						const oauthDisconnectBusy =
							busyAction === `oauth-disconnect:${definition.id}`;
						const tools = listMcpIntegrationTools(
							integration,
							reportedStatus?.tools ?? [],
						);
						const removing = busyAction === `remove:${definition.id}`;
						const confirmingRemove =
							removeTarget?.definition.id === definition.id;
						return (
							<div
								key={definition.id}
								className="rounded-xl border border-border/60 bg-background p-4"
							>
								<div className="flex flex-wrap items-start justify-between gap-3">
									<div className="min-w-0">
										<div className="flex flex-wrap items-center gap-2">
											{definition.transport.type === "http" ? (
												<Globe2 className="size-4 text-muted-foreground" />
											) : (
												<Command className="size-4 text-muted-foreground" />
											)}
											<h3 className="text-[13px] font-medium text-foreground">
												{definition.displayName}
											</h3>
											<Badge
												variant={
													definition.enabled
														? "success"
														: needsTrust
															? "warn"
															: "outline"
												}
											>
												{definition.enabled
													? t("settings.integrations.enabled")
													: needsTrust
														? t("settings.integrations.needsTrust")
														: t("settings.integrations.disabled")}
											</Badge>
										</div>
										<p
											className="mt-2 max-w-2xl truncate font-mono text-[11px] text-muted-foreground"
											title={formatMcpTransportPreview(definition.transport)}
										>
											{formatMcpTransportPreview(definition.transport)}
										</p>
										<div className="mt-2 flex flex-wrap gap-1.5">
											<Badge variant="outline">
												{definition.transport.type === "http"
													? t("settings.integrations.http")
													: t("settings.integrations.stdio")}
											</Badge>
											<Badge variant="outline">
												{t(scopeLabelKey(scope))}
											</Badge>
											{target ? (
												<Badge
													variant="ghost"
													className="max-w-52 truncate font-mono"
													title={target}
												>
													{target}
												</Badge>
											) : null}
											{integration.credentialCount > 0 ? (
												<Badge variant="secondary">
													<KeyRound className="size-3" />
													{t("settings.integrations.credentialCount", {
														count: integration.credentialCount,
													})}
												</Badge>
											) : null}
										</div>
									</div>

									<div className="flex items-center gap-2">
										<Switch
											checked={definition.enabled}
											disabled={busyAction !== null}
											onCheckedChange={(checked) => {
												if (checked) {
													setView({ type: "review", integration });
												} else {
													void handleDisable(integration);
												}
											}}
											aria-label={
												definition.enabled
													? t("settings.integrations.disable")
													: t("settings.integrations.enable")
											}
										/>
										<Button
											type="button"
											variant="ghost"
											size="icon-sm"
											disabled={busyAction !== null}
											onClick={() => {
												setRemoveTarget(integration);
												setDeleteCredentials(false);
											}}
											aria-label={t("settings.integrations.remove")}
										>
											<Trash2 className="size-3.5" />
										</Button>
									</div>
								</div>

								<div className="mt-4 rounded-lg border border-border/50 bg-muted/10 p-3">
									<div className="flex flex-wrap items-center justify-between gap-2">
										<div className="flex items-center gap-2">
											<span className="text-[11px] font-medium text-foreground">
												{t("settings.integrations.runtimeStatus")}
											</span>
											<Badge variant={runtimeStatusVariant(runtimeView.kind)}>
												{t(
													`settings.integrations.runtime.${runtimeView.kind}`,
												)}
											</Badge>
										</div>
										{reportedStatus ? (
											<span className="font-mono text-[10px] text-muted-foreground">
												{reportedStatus.providerId}{" "}
												{reportedStatus.providerVersion}
											</span>
										) : null}
									</div>
									{runtimeView.kind === "restartRequired" ? (
										<p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">
											{t("settings.integrations.restartHint")}
										</p>
									) : runtimeView.kind === "notReported" ? (
										<p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">
											{t("settings.integrations.notReportedHint")}
										</p>
									) : null}
									{tools.length > 0 ? (
										<div className="mt-3 space-y-2 border-t border-border/50 pt-3">
											<div>
												<p className="text-[11px] font-medium text-foreground">
													{t("settings.integrations.toolPoliciesTitle")}
												</p>
												<p className="mt-0.5 text-[10px] leading-relaxed text-muted-foreground">
													{t("settings.integrations.toolPoliciesHint")}
												</p>
											</div>
											{tools.map((toolName) => {
												const decision = getMcpToolPolicyDecision(
													integration,
													toolName,
												);
												const annotationHints = listMcpToolAnnotationHints(
													reportedStatus?.tools?.find(
														(tool) => tool.name === toolName,
													),
												);
												const policyBusy = busyAction?.startsWith(
													`policy:${definition.id}:${toolName}:`,
												);
												return (
													<div
														key={toolName}
														className="flex flex-col gap-2 rounded-md border border-border/40 bg-background px-2.5 py-2 sm:flex-row sm:items-center sm:justify-between"
													>
														<div className="min-w-0">
															<span
																className="block truncate font-mono text-[10px] text-foreground"
																title={toolName}
															>
																{toolName}
															</span>
															{annotationHints.length > 0 ? (
																<div className="mt-1.5 flex flex-wrap gap-1">
																	{annotationHints.map((hint) => (
																		<Badge
																			key={hint}
																			variant="outline"
																			className="h-5 px-1.5 text-[8px] font-normal text-muted-foreground"
																			title={t(
																				"settings.integrations.toolAnnotationsDisclaimer",
																			)}
																		>
																			{t(
																				`settings.integrations.toolAnnotation.${hint}`,
																			)}
																		</Badge>
																	))}
																</div>
															) : null}
														</div>
														<ToggleGroup
															type="single"
															value={decision}
															onValueChange={(value) => {
																if (
																	value === "ask" ||
																	value === "allow" ||
																	value === "deny"
																) {
																	void handleToolPolicy(
																		integration,
																		toolName,
																		value,
																	);
																}
															}}
															disabled={busyAction !== null}
															className="grid shrink-0 grid-cols-3 gap-1"
														>
															{(["ask", "allow", "deny"] as const).map(
																(value) => (
																	<ToggleGroupItem
																		key={value}
																		value={value}
																		className="h-7 min-w-14 rounded-md border border-border/50 px-2 text-[10px]"
																		aria-label={t(
																			`settings.integrations.toolPolicy.${value}`,
																		)}
																	>
																		{policyBusy &&
																		busyAction?.endsWith(
																			`:${value}`,
																		) ? (
																			<Loader2 className="size-3 animate-spin" />
																		) : (
																			t(
																				`settings.integrations.toolPolicy.${value}`,
																			)
																		)}
																	</ToggleGroupItem>
																),
															)}
														</ToggleGroup>
													</div>
												);
											})}
										</div>
									) : null}
									{reportedStatus?.boundedError ? (
										<p className="mt-2 break-words text-[11px] leading-relaxed text-destructive">
											{reportedStatus.boundedError.message}
										</p>
									) : null}
									{needsOauth ? (
										<div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border/50 pt-3">
											<Button
												type="button"
												variant="outline"
												size="sm"
												disabled={busyAction !== null}
												onClick={() => void handleMcpOauth(integration)}
											>
												{oauthBusy ? (
													<Loader2 className="size-3.5 animate-spin" />
												) : (
													<Link2 className="size-3.5" />
												)}
												{t("settings.integrations.oauthConnect")}
											</Button>
											<span className="text-[10px] leading-relaxed text-muted-foreground">
												{t("settings.integrations.oauthHint")}
											</span>
										</div>
									) : null}
									{hasManagedOauth ? (
										<div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border/50 pt-3">
											<Button
												type="button"
												variant="outline"
												size="sm"
												disabled={busyAction !== null}
												onClick={() =>
													void handleDisconnectMcpOauth(integration)
												}
											>
												{oauthDisconnectBusy ? (
													<Loader2 className="size-3.5 animate-spin" />
												) : (
													<Unlink className="size-3.5" />
												)}
												{t("settings.integrations.oauthDisconnect")}
											</Button>
											<span className="text-[10px] leading-relaxed text-muted-foreground">
												{t(
													"settings.integrations.oauthDisconnectHint",
												)}
											</span>
										</div>
									) : null}
									{reportedStatus ? (
										<p className="mt-2 text-[10px] text-muted-foreground">
											{t("settings.integrations.checkedAt", {
												date: new Date(
													reportedStatus.checkedAt,
												).toLocaleString(),
											})}
										</p>
									) : null}
								</div>

								{confirmingRemove ? (
									<div className="mt-4 rounded-lg border border-destructive/25 bg-destructive/5 p-3">
										<p className="text-[12px] font-medium text-foreground">
											{t("settings.integrations.removeConfirmTitle")}
										</p>
										<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
											{t("settings.integrations.removeConfirmHint")}
										</p>
										{integration.credentialCount > 0 ? (
											<label className="mt-3 flex items-center justify-between gap-4 rounded-lg border border-border/50 bg-background px-3 py-2">
												<span>
													<span className="block text-[12px] font-medium text-foreground">
														{t("settings.integrations.deleteCredentials")}
													</span>
													<span className="mt-0.5 block text-[10px] text-muted-foreground">
														{t(
															"settings.integrations.deleteCredentialsHint",
														)}
													</span>
												</span>
												<Switch
													checked={deleteCredentials}
													onCheckedChange={setDeleteCredentials}
												/>
											</label>
										) : null}
										<div className="mt-3 flex justify-end gap-2">
											<Button
												type="button"
												variant="ghost"
												size="sm"
												onClick={() => {
													setRemoveTarget(null);
													setDeleteCredentials(false);
												}}
												disabled={removing}
											>
												{t("settings.integrations.cancel")}
											</Button>
											<Button
												type="button"
												variant="destructive"
												size="sm"
												onClick={() => void handleRemove()}
												disabled={removing}
											>
												{removing ? (
													<Loader2 className="size-3.5 animate-spin" />
												) : (
													<Trash2 className="size-3.5" />
												)}
												{t("settings.integrations.remove")}
											</Button>
										</div>
									</div>
								) : null}
							</div>
						);
					})}
				</div>
			)}
		</section>
	);
}
