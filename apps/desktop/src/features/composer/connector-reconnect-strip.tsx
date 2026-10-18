import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { PlugZap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { listMcpIntegrations } from "@/lib/mcp-api";
import { ConnectDialog } from "@/features/settings/connectors/ConnectDialog";
import {
	connectorUrl,
	deriveConnectorView,
	type ConnectorTarget,
} from "@/features/settings/connectors/connector-model";
import { useConnectorNeedsAuthEvents } from "@/features/settings/connectors/use-connector-events";
import { MCP_INTEGRATIONS_QUERY_KEY } from "@/features/settings/mcp-integrations-panel";

/**
 * Shows connectors whose sign-in expired right where the user is about to
 * send, with a one-click Reconnect. Nothing renders while all is well.
 */
export function ConnectorReconnectStrip() {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [target, setTarget] = useState<ConnectorTarget | null>(null);
	useConnectorNeedsAuthEvents();
	const query = useQuery({
		queryKey: MCP_INTEGRATIONS_QUERY_KEY,
		queryFn: listMcpIntegrations,
	});
	const integrations = query.data?.integrations ?? [];
	const lost = useMemo(
		() =>
			integrations
				.filter((integration) => integration.bindings.some((binding) => binding.enabled))
				.map(deriveConnectorView)
				.filter((view) => view.status === "needsAuth" && view.isRemote),
		[integrations],
	);
	if (lost.length === 0) return null;
	const first = lost[0]!;
	const reconnect = () => {
		const url = connectorUrl(first.integration);
		if (url) setTarget({ name: first.name, url, preset: first.preset });
	};

	return (
		<>
			<div
				className="mb-2 flex items-center gap-2 rounded-xl border border-amber-500/35 bg-amber-500/10 px-3 py-1.5"
				role="status"
			>
				<PlugZap
					className="size-3.5 shrink-0 text-amber-600 dark:text-amber-300"
					aria-hidden
				/>
				<p className="min-w-0 flex-1 truncate text-[12px] text-foreground">
					{lost.length === 1
						? t("composer.connectorReconnect.one", { name: first.name })
						: t("composer.connectorReconnect.many", {
								name: first.name,
								count: lost.length - 1,
							})}
				</p>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="h-6 shrink-0 px-2"
					onClick={reconnect}
				>
					<span className="text-[12px]">{t("settings.connectors.actions.reconnect")}</span>
				</Button>
			</div>
			<ConnectDialog
				target={target}
				integrations={integrations}
				onOpenChange={(open) => {
					if (!open) setTarget(null);
				}}
				onFinished={() =>
					void queryClient.invalidateQueries({ queryKey: MCP_INTEGRATIONS_QUERY_KEY })
				}
			/>
		</>
	);
}
