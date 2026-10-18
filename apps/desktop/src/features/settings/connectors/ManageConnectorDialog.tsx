import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";
import type { McpIntegrationRecord, McpToolPolicyDecision } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogTitle,
} from "@/components/ui/dialog";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Switch } from "@/components/ui/switch";
import { setMcpProjectEnabled, setMcpToolPolicy } from "@/lib/mcp-api";
import { listRepositories } from "@/lib/workspace-api";
import {
	getMcpToolPolicyDecision,
	listMcpToolAnnotationHints,
} from "../mcp-integration-runtime";
import { ConnectorGlyph } from "./connector-logos";
import type { ConnectorView } from "./connector-model";

type ManageConnectorDialogProps = {
	view: ConnectorView | null;
	busy: string | null;
	onOpenChange: (open: boolean) => void;
	onTest: (integration: McpIntegrationRecord) => void;
	onDisconnect: (view: ConnectorView) => void;
	onRemove: (view: ConnectorView) => void;
	onChanged: () => void;
};

export function ManageConnectorDialog({
	view,
	busy,
	onOpenChange,
	onTest,
	onDisconnect,
	onRemove,
	onChanged,
}: ManageConnectorDialogProps) {
	const { t } = useTranslation();
	const [policyBusy, setPolicyBusy] = useState<string | null>(null);
	const repositories = useQuery({
		queryKey: ["connectorProjects"],
		queryFn: listRepositories,
		staleTime: 60_000,
		enabled: view !== null,
	});
	// One row per project; a project may hold several repositories.
	const projects = useMemo(() => {
		const byId = new Map<string, string>();
		for (const repository of repositories.data?.repositories ?? []) {
			if (!byId.has(repository.projectId)) {
				byId.set(repository.projectId, repository.displayName ?? repository.name);
			}
		}
		return [...byId.entries()]
			.map(([id, name]) => ({ id, name }))
			.sort((left, right) => left.name.localeCompare(right.name));
	}, [repositories.data]);

	if (!view) {
		return (
			<Dialog open={false} onOpenChange={onOpenChange}>
				{null}
			</Dialog>
		);
	}
	const { integration } = view;
	const id = integration.definition.id;
	const check = integration.connectionCheck ?? null;
	const tools = check?.tools ?? [];
	const testing = busy === `test:${id}`;

	// "ask" is the stored default, which now follows the task's mode.
	const policyLabel = (value: McpToolPolicyDecision) =>
		value === "ask"
			? t("settings.connectors.manage.policyDefault")
			: t(`settings.integrations.toolPolicy.${value}`);

	const excluded = new Set(integration.excludedProjectIds ?? []);
	const appliesGlobally = integration.bindings.some(
		(binding) => binding.enabled && binding.scope.type === "global",
	);
	const toggleProject = async (projectId: string, enabled: boolean) => {
		setPolicyBusy(`project:${projectId}`);
		try {
			await setMcpProjectEnabled({ definitionId: id, projectId, enabled });
			onChanged();
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		} finally {
			setPolicyBusy(null);
		}
	};

	const changePolicy = async (toolName: string, decision: McpToolPolicyDecision) => {
		setPolicyBusy(`${toolName}:${decision}`);
		try {
			await setMcpToolPolicy({ definitionId: id, toolName, decision });
			onChanged();
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		} finally {
			setPolicyBusy(null);
		}
	};

	const statusText =
		view.status === "needsAuth"
			? t("settings.connectors.status.needsAuth")
			: view.status === "failed"
				? (check?.message ?? t("settings.connectors.status.failed"))
				: view.status === "off"
					? t("settings.connectors.status.off")
					: view.status === "unchecked"
						? t("settings.connectors.status.unchecked")
						: t("settings.connectors.status.connectedTools", {
								count: view.toolCount,
							});

	return (
		<Dialog open onOpenChange={onOpenChange}>
			<DialogContent className="flex max-h-[min(640px,calc(100vh-4rem))] w-[520px] max-w-[calc(100vw-4rem)] flex-col gap-0 p-0 sm:max-w-[520px]">
				<div className="flex items-center gap-3 border-b border-border/50 px-5 py-4 pr-12">
					<ConnectorGlyph logoId={view.logoId} name={view.name} />
					<div className="min-w-0 flex-1">
						<DialogTitle className="truncate text-[14px] font-semibold">
							{view.name}
						</DialogTitle>
						<DialogDescription
							className={
								view.status === "needsAuth"
									? "truncate text-[12px] text-[var(--warning)]"
									: "truncate text-[12px] text-muted-foreground"
							}
						>
							{statusText}
						</DialogDescription>
					</div>
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={() => onTest(integration)}
						disabled={busy !== null}
					>
						{testing ? <Loader2 className="size-3.5 animate-spin" /> : null}
						<span className="text-[12px]">
							{testing
								? t("settings.connectors.actions.testing")
								: t("settings.connectors.actions.test")}
						</span>
					</Button>
				</div>

				<div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
					{appliesGlobally && projects.length > 0 ? (
						<div className="mb-5">
							<p className="text-[12px] font-medium text-foreground">
								{t("settings.connectors.manage.projectsTitle")}
							</p>
							<p className="mt-0.5 text-[11px] leading-relaxed text-muted-foreground">
								{t("settings.connectors.manage.projectsHint")}
							</p>
							<div className="mt-2 space-y-1">
								{projects.map((project) => (
									<label
										key={project.id}
										className="flex items-center justify-between gap-3 rounded-lg px-3 py-1.5 hover:bg-muted/40"
									>
										<span className="truncate text-[12px] text-foreground">{project.name}</span>
										<Switch
											size="sm"
											checked={!excluded.has(project.id)}
											disabled={policyBusy !== null}
											onCheckedChange={(checked) => void toggleProject(project.id, checked)}
										/>
									</label>
								))}
							</div>
						</div>
					) : null}
					<p className="text-[12px] font-medium text-foreground">
						{t("settings.connectors.manage.toolsTitle")}
					</p>
					<p className="mt-0.5 text-[11px] leading-relaxed text-muted-foreground">
						{tools.length > 0
							? t("settings.connectors.manage.toolsHint")
							: t("settings.connectors.manage.noTools")}
					</p>
					<div className="mt-3 space-y-1.5">
						{tools.map((tool) => {
							const decision = getMcpToolPolicyDecision(integration, tool.name);
							const hints = listMcpToolAnnotationHints(tool);
							return (
								<div
									key={tool.name}
									className="flex items-center justify-between gap-3 rounded-lg border border-border/40 px-3 py-2"
								>
									<div className="min-w-0">
										<span
											className="block truncate font-mono text-[11px] text-foreground"
											title={tool.name}
										>
											{tool.name}
										</span>
										{hints.length > 0 ? (
											<div className="mt-1 flex flex-wrap gap-1">
												{hints.map((hint) => (
													<Badge
														key={hint}
														variant="outline"
														className="h-4 px-1 text-[9px] font-normal text-muted-foreground"
														title={t("settings.integrations.toolAnnotationsDisclaimer")}
													>
														{t(`settings.integrations.toolAnnotation.${hint}`)}
													</Badge>
												))}
											</div>
										) : null}
									</div>
									<ToggleGroup
										type="single"
										value={decision}
										onValueChange={(value) => {
											if (value === "ask" || value === "allow" || value === "deny") {
												void changePolicy(tool.name, value);
											}
										}}
										disabled={policyBusy !== null}
										className="grid shrink-0 grid-cols-3 gap-1"
									>
										{(["ask", "allow", "deny"] as const).map((value) => (
											<ToggleGroupItem
												key={value}
												value={value}
												className="h-6 min-w-12 rounded-md border border-border/50 px-1.5"
												aria-label={policyLabel(value)}
											>
												{policyBusy === `${tool.name}:${value}` ? (
													<Loader2 className="size-3 animate-spin" />
												) : (
													<span className="text-[10px]">{policyLabel(value)}</span>
												)}
											</ToggleGroupItem>
										))}
									</ToggleGroup>
								</div>
							);
						})}
					</div>
				</div>

				<div className="flex items-center justify-between gap-2 border-t border-border/50 px-5 py-3">
					<span className="truncate text-[11px] text-muted-foreground">
						{check
							? t("settings.connectors.manage.lastChecked", {
									date: new Date(check.checkedAt).toLocaleString(),
								})
							: null}
					</span>
					<div className="flex shrink-0 gap-2">
						{view.signedIn ? (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => onDisconnect(view)}
								disabled={busy !== null}
							>
								<span className="text-[12px]">
									{t("settings.connectors.actions.disconnect")}
								</span>
							</Button>
						) : null}
						<Button
							type="button"
							variant="destructive"
							size="sm"
							onClick={() => onRemove(view)}
							disabled={busy !== null}
						>
							<span className="text-[12px]">{t("settings.connectors.actions.remove")}</span>
						</Button>
					</div>
				</div>
			</DialogContent>
		</Dialog>
	);
}
