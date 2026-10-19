import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
	ChevronDown,
	ChevronRight,
	Loader2,
	MoreHorizontal,
	Plus,
	Search,
} from "lucide-react";
import { toast } from "sonner";
import type { McpIntegrationRecord, ProviderCatalog } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import {
	disconnectMcpConnector,
	listMcpIntegrations,
	removeMcpIntegration,
	testMcpIntegration,
} from "@/lib/mcp-api";
import { cn } from "@/lib/utils";
import {
	McpIntegrationsPanel,
	MCP_INTEGRATIONS_QUERY_KEY,
} from "../mcp-integrations-panel";
import {
	CONNECTOR_CATEGORIES,
	visibleConnectorPresets,
	type ConnectorPreset,
} from "./catalog";
import { ConnectorGlyph } from "./connector-logos";
import {
	connectorUrl,
	deriveConnectorView,
	findIntegrationForUrl,
	sortConnectorViews,
	type ConnectorTarget,
	type ConnectorView,
} from "./connector-model";
import { ConnectDialog } from "./ConnectDialog";
import { AddCustomConnectorDialog } from "./AddCustomConnectorDialog";
import { ManageConnectorDialog } from "./ManageConnectorDialog";
import { useConnectorNeedsAuthEvents } from "./use-connector-events";

type ConnectorsPanelProps = {
	projectId: string | null;
	sessionId: string | null;
	sessionProviderId: string | null;
	sessionCreatedAt: string | null;
	workspaceName: string | null;
	providerCatalog: ProviderCatalog | null;
};

function isTauriRuntime(): boolean {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function matches(query: string, ...values: Array<string | null | undefined>) {
	const needle = query.trim().toLocaleLowerCase();
	return (
		needle.length === 0 ||
		values.some((value) => value?.toLocaleLowerCase().includes(needle))
	);
}

/** Enabled providers that cannot receive connectors, for the footer. */
function providersWithoutConnectors(catalog: ProviderCatalog | null): string[] {
	return (catalog?.providers ?? [])
		.filter((provider) => provider.enabled !== false)
		.filter((provider) => {
			const support = provider.capabilities.mcpSupport;
			return !(
				support &&
				typeof support === "object" &&
				("runtimeBridge" in support || "verifiedBridge" in support)
			);
		})
		.map((provider) => provider.label);
}

export function ConnectorsPanel(props: ConnectorsPanelProps) {
	const { t, i18n } = useTranslation();
	const queryClient = useQueryClient();
	const [search, setSearch] = useState("");
	const [connectTarget, setConnectTarget] = useState<ConnectorTarget | null>(
		null,
	);
	const [addOpen, setAddOpen] = useState(false);
	const [managed, setManaged] = useState<string | null>(null);
	const [advancedOpen, setAdvancedOpen] = useState(false);
	const [busy, setBusy] = useState<string | null>(null);

	const integrationsQuery = useQuery({
		queryKey: MCP_INTEGRATIONS_QUERY_KEY,
		queryFn: listMcpIntegrations,
	});
	const integrations = integrationsQuery.data?.integrations ?? [];
	const refresh = () =>
		queryClient.invalidateQueries({ queryKey: MCP_INTEGRATIONS_QUERY_KEY });

	useConnectorNeedsAuthEvents();

	const connected = useMemo(
		() =>
			sortConnectorViews(integrations.map(deriveConnectorView)).filter(
				(view) =>
					matches(
						search,
						view.name,
						connectorUrl(view.integration),
						view.preset ? t(`settings.connectors.catalog.${view.preset.id}`) : "",
					),
			),
		[integrations, search, t],
	);
	const shelves = useMemo(
		() =>
			CONNECTOR_CATEGORIES.map((category) => ({
				category,
				presets: visibleConnectorPresets().filter(
					(preset) =>
						preset.category === category &&
						!findIntegrationForUrl(integrations, preset.url) &&
						matches(
							search,
							preset.name,
							t(`settings.connectors.catalog.${preset.id}`),
						),
				),
			})).filter((shelf) => shelf.presets.length > 0),
		[integrations, search, t],
	);
	const managedView = useMemo(() => {
		const integration = integrations.find(
			(candidate) => candidate.definition.id === managed,
		);
		return integration ? deriveConnectorView(integration) : null;
	}, [integrations, managed]);
	const unsupported = providersWithoutConnectors(props.providerCatalog);

	const openPreset = (preset: ConnectorPreset) =>
		setConnectTarget({ name: preset.name, url: preset.url, preset });
	const reconnect = (view: ConnectorView) => {
		const url = connectorUrl(view.integration);
		if (!url) return;
		setConnectTarget({ name: view.name, url, preset: view.preset });
	};

	const runTest = async (integration: McpIntegrationRecord) => {
		const name = integration.definition.displayName;
		setBusy(`test:${integration.definition.id}`);
		try {
			const { check } = await testMcpIntegration({
				definitionId: integration.definition.id,
			});
			if (check.state === "connected") {
				toast.success(
					t("settings.connectors.toast.testConnected", {
						name,
						count: check.tools?.length ?? 0,
					}),
				);
			} else if (check.state === "needsAuth") {
				toast.warning(t("settings.connectors.toast.testNeedsAuth", { name }));
			} else {
				toast.error(
					t("settings.connectors.toast.testFailed", {
						name,
						message: check.message ?? "",
					}),
				);
			}
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		} finally {
			setBusy(null);
			await refresh();
		}
	};

	const disconnect = async (view: ConnectorView) => {
		if (!window.confirm(t("settings.connectors.manage.disconnectConfirm", { name: view.name }))) {
			return;
		}
		setBusy(`disconnect:${view.integration.definition.id}`);
		try {
			await disconnectMcpConnector({
				definitionId: view.integration.definition.id,
			});
			toast.success(t("settings.connectors.toast.disconnected", { name: view.name }));
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		} finally {
			setBusy(null);
			await refresh();
		}
	};

	const remove = async (view: ConnectorView) => {
		if (!window.confirm(t("settings.connectors.manage.removeConfirm", { name: view.name }))) {
			return;
		}
		setBusy(`remove:${view.integration.definition.id}`);
		try {
			if (view.signedIn) {
				await disconnectMcpConnector({
					definitionId: view.integration.definition.id,
				});
			}
			await removeMcpIntegration({
				definitionId: view.integration.definition.id,
				deleteCredentials: true,
			});
			setManaged(null);
			toast.success(t("settings.connectors.toast.removed", { name: view.name }));
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		} finally {
			setBusy(null);
			await refresh();
		}
	};

	const statusLine = (view: ConnectorView) => {
		switch (view.status) {
			case "needsAuth":
				return (
					<span className="text-[var(--warning)]">
						{t("settings.connectors.status.needsAuth")}
					</span>
				);
			case "failed":
				return t("settings.connectors.status.failed");
			case "off":
				return t("settings.connectors.status.off");
			case "unchecked":
				return view.preset
					? t(`settings.connectors.catalog.${view.preset.id}`)
					: t("settings.connectors.status.unchecked");
			default:
				return view.toolCount > 0
					? t("settings.connectors.status.connectedTools", {
							count: view.toolCount,
						})
					: t("settings.connectors.status.connected");
		}
	};

	return (
		<section className="space-y-7">
			<div className="flex items-center gap-2">
				<div className="relative min-w-0 flex-1">
					<Search
						className="pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2 text-muted-foreground"
						aria-hidden="true"
					/>
					<Input
						type="search"
						value={search}
						onChange={(event) => setSearch(event.target.value)}
						onKeyDown={(event) => {
							if (event.key === "Escape" && search.length > 0) {
								event.preventDefault();
								event.stopPropagation();
								setSearch("");
							}
						}}
						placeholder={t("settings.connectors.searchPlaceholder")}
						aria-label={t("settings.connectors.searchPlaceholder")}
						className="h-8 rounded-full pl-8 text-[12px]"
					/>
				</div>
				<Button
					type="button"
					size="sm"
					className="rounded-full"
					onClick={() => setAddOpen(true)}
				>
					<Plus className="size-3.5" />
					<span className="text-[12px]">{t("settings.connectors.addCustom")}</span>
				</Button>
			</div>

			{!isTauriRuntime() ? (
				<p className="text-[12px] text-muted-foreground">
					{t("settings.connectors.desktopOnly")}
				</p>
			) : null}

			{integrationsQuery.isPending ? (
				<div className="flex items-center gap-2 text-[12px] text-muted-foreground">
					<Loader2 className="size-3.5 animate-spin" />
				</div>
			) : integrationsQuery.isError ? (
				<div className="flex items-center gap-3 text-[12px] text-destructive">
					<span>{t("settings.connectors.loadError")}</span>
					<Button
						type="button"
						variant="outline"
						size="xs"
						onClick={() => void integrationsQuery.refetch()}
					>
						<span className="text-[12px]">{t("settings.connectors.retry")}</span>
					</Button>
				</div>
			) : null}

			{connected.length > 0 ? (
				<Shelf title={t("settings.connectors.connectedTitle")}>
					{connected.map((view) => {
						const id = view.integration.definition.id;
						const rowBusy = busy?.endsWith(`:${id}`) ?? false;
						return (
							<div key={id} className={ROW_CLASS}>
								<ConnectorGlyph
									logoId={view.logoId}
									logoUri={view.logoUri}
									name={view.name}
								/>
								<RowText title={view.name}>{statusLine(view)}</RowText>
								{rowBusy ? (
									<Loader2 className="size-3.5 shrink-0 animate-spin text-muted-foreground" />
								) : null}
								{view.status === "needsAuth" && view.isRemote ? (
									<Button
										type="button"
										variant="outline"
										size="xs"
										onClick={() => reconnect(view)}
									>
										<span className="text-[12px]">
											{t("settings.connectors.actions.reconnect")}
										</span>
									</Button>
								) : null}
								<DropdownMenu>
									<DropdownMenuTrigger asChild>
										<Button
											type="button"
											variant="ghost"
											size="icon-sm"
											className="rounded-full"
											aria-label={t("settings.connectors.actions.more", {
												name: view.name,
											})}
											disabled={busy !== null}
										>
											<MoreHorizontal className="size-4" />
										</Button>
									</DropdownMenuTrigger>
									<DropdownMenuContent align="end" className="w-44">
										<DropdownMenuItem onSelect={() => setManaged(id)}>
											{t("settings.connectors.actions.manage")}
										</DropdownMenuItem>
										<DropdownMenuItem onSelect={() => void runTest(view.integration)}>
											{t("settings.connectors.actions.test")}
										</DropdownMenuItem>
										{view.isRemote && view.status !== "needsAuth" ? (
											<DropdownMenuItem onSelect={() => reconnect(view)}>
												{t("settings.connectors.actions.reconnect")}
											</DropdownMenuItem>
										) : null}
										<DropdownMenuSeparator />
										{view.signedIn ? (
											<DropdownMenuItem
												className="text-destructive focus:text-destructive"
												onSelect={() => void disconnect(view)}
											>
												{t("settings.connectors.actions.disconnect")}
											</DropdownMenuItem>
										) : null}
										<DropdownMenuItem
											className="text-destructive focus:text-destructive"
											onSelect={() => void remove(view)}
										>
											{t("settings.connectors.actions.remove")}
										</DropdownMenuItem>
									</DropdownMenuContent>
								</DropdownMenu>
							</div>
						);
					})}
				</Shelf>
			) : null}

			{shelves.map((shelf) => (
				<Shelf
					key={shelf.category}
					title={t(`settings.connectors.categories.${shelf.category}`)}
				>
					{shelf.presets.map((preset) => (
						<button
							key={preset.id}
							type="button"
							className={cn(ROW_CLASS, "text-left hover:bg-muted/50")}
							onClick={() => openPreset(preset)}
						>
							<ConnectorGlyph logoId={preset.logoId} name={preset.name} />
							<RowText title={preset.name}>
								{t(`settings.connectors.catalog.${preset.id}`)}
							</RowText>
							<Plus
								className="size-4 shrink-0 text-muted-foreground"
								aria-label={t("settings.connectors.actions.connect")}
							/>
						</button>
					))}
				</Shelf>
			))}

			{search.trim().length > 0 && connected.length === 0 && shelves.length === 0 ? (
				<p className="text-[12px] text-muted-foreground">
					{t("settings.connectors.noMatch", { query: search.trim() })}
				</p>
			) : null}

			<p className="text-[11px] leading-relaxed text-muted-foreground">
				{t("settings.connectors.footer")}
				{unsupported.length > 0
					? ` ${t("settings.connectors.footerUnsupported", {
							providers: new Intl.ListFormat(i18n.language, {
								style: "long",
								type: "conjunction",
							}).format(unsupported),
						})}`
					: null}
			</p>

			<div className="border-t border-border/50 pt-4">
				<button
					type="button"
					className="flex w-full items-center gap-2 text-left"
					aria-expanded={advancedOpen}
					onClick={() => setAdvancedOpen((open) => !open)}
				>
					{advancedOpen ? (
						<ChevronDown className="size-4 text-muted-foreground" />
					) : (
						<ChevronRight className="size-4 text-muted-foreground" />
					)}
					<span className="text-[12px] font-medium text-foreground">
						{t("settings.connectors.advanced.title")}
					</span>
					<span className="truncate text-[11px] text-muted-foreground">
						{t("settings.connectors.advanced.hint")}
					</span>
				</button>
				{advancedOpen ? (
					<div className="mt-4">
						<McpIntegrationsPanel {...props} />
					</div>
				) : null}
			</div>

			<ConnectDialog
				target={connectTarget}
				integrations={integrations}
				onOpenChange={(open) => {
					if (!open) setConnectTarget(null);
				}}
				onFinished={() => void refresh()}
			/>
			<AddCustomConnectorDialog
				open={addOpen}
				onOpenChange={setAddOpen}
				onSubmit={(target) => {
					setAddOpen(false);
					setConnectTarget(target);
				}}
			/>
			<ManageConnectorDialog
				view={managedView}
				busy={busy}
				onOpenChange={(open) => {
					if (!open) setManaged(null);
				}}
				onTest={(integration) => void runTest(integration)}
				onDisconnect={(view) => void disconnect(view)}
				onRemove={(view) => void remove(view)}
				onReconnect={(view) => {
					setManaged(null);
					reconnect(view);
				}}
				onChanged={() => void refresh()}
			/>
		</section>
	);
}

const ROW_CLASS =
	"flex min-h-14 w-full items-center gap-3 rounded-xl px-3 py-2.5 transition-colors disabled:cursor-default disabled:opacity-60";

function Shelf({ title, children }: { title: string; children: React.ReactNode }) {
	return (
		<div>
			<h4 className="mb-2 px-3 text-[12px] font-medium text-foreground">{title}</h4>
			<div className="grid grid-cols-1 gap-x-4 gap-y-1 md:grid-cols-2">{children}</div>
		</div>
	);
}

function RowText({ title, children }: { title: string; children: React.ReactNode }) {
	return (
		<span className="min-w-0 flex-1">
			<span className="block truncate text-[13px] font-medium text-foreground">{title}</span>
			<span className="block truncate text-[12px] text-muted-foreground">{children}</span>
		</span>
	);
}
