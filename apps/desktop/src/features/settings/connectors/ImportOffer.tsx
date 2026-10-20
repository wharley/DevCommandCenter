import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Download, Loader2 } from "lucide-react";
import { toast } from "sonner";
import type { McpIntegrationRecord } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import {
	importNativeMcpServers,
	listNativeMcpServers,
	testMcpIntegration,
} from "@/lib/mcp-api";
import {
	importCandidates,
	joinNames,
	NATIVE_APP_NAMES,
	readOfferedUrls,
	rememberOffered,
} from "./import-offer";

/**
 * "Found ClickUp and Linear in Claude Code. Bring them into DCC?" Shown once
 * per server; the other apps' files are only read.
 */
export function ImportOffer({
	integrations,
	onImported,
}: {
	integrations: McpIntegrationRecord[];
	onImported: () => Promise<unknown>;
}) {
	const { t, i18n } = useTranslation();
	const [offered, setOffered] = useState(readOfferedUrls);
	const [importing, setImporting] = useState(false);
	const found = useQuery({
		queryKey: ["mcp", "native-servers"],
		queryFn: listNativeMcpServers,
		staleTime: Number.POSITIVE_INFINITY,
	});
	const candidates = useMemo(
		() => importCandidates(found.data?.servers ?? [], integrations, offered),
		[found.data, integrations, offered],
	);
	if (candidates.length === 0) return null;

	const dismiss = () => {
		const urls = candidates.map((candidate) => candidate.url);
		rememberOffered(urls);
		setOffered((current) => [...current, ...urls]);
	};

	const bring = async () => {
		setImporting(true);
		try {
			const { integrations: imported } = await importNativeMcpServers({
				servers: candidates.map((candidate) => ({
					app: candidate.app,
					name: candidate.name,
					displayName: candidate.displayName,
				})),
			});
			// A check right away shows which ones still need a sign-in.
			await Promise.allSettled(
				imported.map((integration) =>
					testMcpIntegration({ definitionId: integration.definition.id }),
				),
			);
			toast.success(t("settings.connectors.import.done", { count: imported.length }));
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		} finally {
			dismiss();
			setImporting(false);
			await onImported();
		}
	};

	const names = joinNames(
		candidates.map((candidate) => candidate.displayName),
		i18n.language,
	);
	const apps = joinNames(
		[...new Set(candidates.map((candidate) => NATIVE_APP_NAMES[candidate.app]))],
		i18n.language,
	);

	return (
		<div className="flex flex-wrap items-center gap-3 rounded-xl border border-border/60 bg-muted/30 px-3 py-2.5">
			<Download className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
			<p className="min-w-0 flex-1 text-[12px] leading-relaxed text-foreground">
				{t("settings.connectors.import.offer", { names, apps })}
			</p>
			<div className="flex shrink-0 items-center gap-1.5">
				<Button
					type="button"
					variant="ghost"
					size="sm"
					disabled={importing}
					onClick={dismiss}
				>
					<span className="text-[12px]">{t("settings.connectors.import.notNow")}</span>
				</Button>
				<Button
					type="button"
					size="sm"
					className="rounded-full"
					disabled={importing}
					onClick={() => void bring()}
				>
					{importing ? <Loader2 className="size-3.5 animate-spin" /> : null}
					<span className="text-[12px]">{t("settings.connectors.import.bring")}</span>
				</Button>
			</div>
		</div>
	);
}
