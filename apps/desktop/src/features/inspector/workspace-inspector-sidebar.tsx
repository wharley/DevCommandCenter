import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ProviderCatalogCard } from "@/features/providers/provider-catalog-card";
import type { RuntimeSessionSnapshot } from "@/features/sessions/session-workbench";
import type { ProviderCatalog } from "@dcc/contracts";

type WorkspaceInspectorSidebarProps = {
	providerCatalog: ProviderCatalog | null;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	workspaceId: string;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
	selectedProviderLabel: string | null;
	sessionState: string;
	sessionId: string | null;
};

/**
 * Right rail: stacked context (Helmor inspector model). Terminal is not tabbed here —
 * it lives in the main workbench bottom drawer (t3code `ThreadTerminalDrawer` pattern).
 */
export function WorkspaceInspectorSidebar({
	providerCatalog,
	sessionSnapshot,
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	selectedProviderLabel,
	sessionState,
	sessionId,
}: WorkspaceInspectorSidebarProps) {
	const pathLine =
		workspacePath && workspacePath.length > 0
			? workspacePath.length > 56
				? `…${workspacePath.slice(-55)}`
				: workspacePath
			: null;

	return (
		<div
			className="dcc-inspector flex h-full min-h-0 flex-col overflow-hidden text-foreground"
			data-dcc-inspector-root
		>
			<div className="border-b border-border bg-muted/25 px-3 py-2.5">
				<p className="m-0 text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
					Inspector
				</p>
				<p className="m-0 truncate text-[13px] font-medium leading-tight" title={workspaceName}>
					{workspaceName}
				</p>
				<p className="m-0 truncate text-[11px] text-muted-foreground">
					{workspaceBranch}
					{pathLine ? ` · ${pathLine}` : ""}
				</p>
				<dl className="m-0 mt-1.5 grid gap-0.5 text-[10px] leading-tight text-muted-foreground">
					<div className="flex flex-wrap gap-x-2 gap-y-0">
						<dt className="font-medium text-muted-foreground/90">Workspace id</dt>
						<dd className="m-0 font-mono">{workspaceId}</dd>
					</div>
					{selectedProviderLabel ? (
						<div className="flex flex-wrap gap-x-2 gap-y-0">
							<dt className="font-medium text-muted-foreground/90">Provider</dt>
							<dd className="m-0">{selectedProviderLabel}</dd>
						</div>
					) : null}
					{(sessionId != null || sessionState !== "idle") ? (
						<div className="flex flex-wrap gap-x-2 gap-y-0">
							<dt className="font-medium text-muted-foreground/90">Runtime</dt>
							<dd className="m-0">
								{sessionState}
								{sessionId ? ` · ${sessionId}` : ""}
							</dd>
						</div>
					) : null}
				</dl>
			</div>

			<div className="dcc-inspector-sidebar__body flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-3">
				<ProviderCatalogCard catalog={providerCatalog} />
				<Card className="dcc-session-state-card border-border">
					<CardHeader className="pb-2">
						<div className="dcc-card__meta-row">
							<CardTitle className="text-sm font-medium">Session state</CardTitle>
							<Badge variant="outline">
								{sessionSnapshot?.lastTurnState ?? "pending"}
							</Badge>
						</div>
					</CardHeader>
					<CardContent className="dcc-runtime-feed__content pt-0">
						{sessionSnapshot ? (
							<div className="dcc-runtime-feed__list">
								<div className="dcc-runtime-feed__row">
									<strong>Projection</strong>
									<small>
										turns {sessionSnapshot.turnCount} · checkpoints{" "}
										{sessionSnapshot.checkpointCount}
									</small>
								</div>
								<div className="dcc-runtime-feed__row">
									<strong>Provider</strong>
									<small>{sessionSnapshot.providerId}</small>
								</div>
								<div className="dcc-runtime-feed__row">
									<strong>Last turn</strong>
									<small>{sessionSnapshot.lastTurnPrompt ?? "No turn yet"}</small>
								</div>
							</div>
						) : (
							<p className="dcc-card__description text-muted-foreground">
								No active session. Start one from the composer.
							</p>
						)}
					</CardContent>
				</Card>
			</div>
		</div>
	);
}
