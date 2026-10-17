import { Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	compactDccStorage,
	dccStorageBytes,
	readDccStorageUsage,
	type DccStorageUsage,
} from "@/lib/dcc-storage-api";
import { formatDiskBytes } from "./workspace-disk-usage";

/**
 * DCC's own data next to the completed tasks: history, free pages and the
 * browser's cache. "Compactar" runs now what the background passes do.
 */
export function DccDataSection({ open, disabled }: { open: boolean; disabled: boolean }) {
	const { t, i18n } = useTranslation("common");
	const [usage, setUsage] = useState<DccStorageUsage | null>(null);
	const [compacting, setCompacting] = useState(false);
	const format = (bytes: number) => formatDiskBytes(bytes, i18n.resolvedLanguage);

	useEffect(() => {
		if (!open) return;
		let cancelled = false;
		readDccStorageUsage()
			.then((next) => {
				if (!cancelled) setUsage(next);
			})
			.catch(() => {});
		return () => {
			cancelled = true;
		};
	}, [open]);

	if (!usage) return null;

	const compact = async () => {
		setCompacting(true);
		try {
			const after = await compactDccStorage();
			const freed = Math.max(0, dccStorageBytes(usage) - dccStorageBytes(after));
			setUsage(after);
			toast.success(
				after.reclaimsOnRestart
					? t("sidebar.cleanup.dccData.compactedOnRestart", {
							size: format(freed),
							pending: format(after.reclaimableBytes),
						})
					: t("sidebar.cleanup.dccData.compacted", { size: format(freed) }),
			);
		} catch (error) {
			console.warn("[dcc] failed to compact DCC data", error);
			toast.error(t("sidebar.cleanup.dccData.failed"));
		} finally {
			setCompacting(false);
		}
	};

	return (
		<div
			className="flex min-w-0 items-center justify-between gap-3 rounded-lg border border-border/70 px-3 py-2.5"
			data-testid="dcc-data-section"
		>
			<span className="min-w-0">
				<span className="block text-sm font-medium text-foreground">
					{t("sidebar.cleanup.dccData.label")}
				</span>
				<span className="block text-xs tabular-nums text-muted-foreground">
					{t("sidebar.cleanup.dccData.summary", {
						database: format(usage.databaseBytes),
						reclaimable: format(usage.reclaimableBytes),
						cache: format(usage.browserCacheBytes),
						webview: format(usage.appWebviewBytes),
					})}
				</span>
			</span>
			<Button
				type="button"
				variant="outline"
				size="sm"
				className="shrink-0"
				disabled={disabled || compacting}
				onClick={() => void compact()}
			>
				{compacting ? <Loader2 className="size-3.5 animate-spin" aria-hidden /> : null}
				{compacting ? t("sidebar.cleanup.dccData.compacting") : t("sidebar.cleanup.dccData.compact")}
			</Button>
		</div>
	);
}
