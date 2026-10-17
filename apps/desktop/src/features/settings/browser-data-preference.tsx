import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { formatDiskBytes } from "@/features/workspaces/workspace-disk-usage";
import {
	clearBrowserData,
	readBrowserStorageUsage,
	type BrowserStorageUsage,
} from "@/lib/browser-data-api";

/** Settings → General. Hidden where the browser keeps nothing on disk. */
export function BrowserDataPreference() {
	const { t, i18n } = useTranslation("common");
	const [usage, setUsage] = useState<BrowserStorageUsage | null>(null);
	const [clearing, setClearing] = useState<"cache" | "everything" | null>(null);
	const [cleared, setCleared] = useState(false);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		let disposed = false;
		readBrowserStorageUsage()
			.then((next) => {
				if (!disposed) setUsage(next);
			})
			.catch(() => {});
		return () => {
			disposed = true;
		};
	}, []);

	if (!usage?.supported) return null;

	const format = (bytes: number) => formatDiskBytes(bytes, i18n.language);
	const clear = async (everything: boolean) => {
		if (clearing) return;
		if (everything && !window.confirm(t("settings.general.browserData.confirmEverything"))) return;
		setClearing(everything ? "everything" : "cache");
		setCleared(false);
		setError(null);
		try {
			setUsage(await clearBrowserData(everything));
			setCleared(true);
		} catch (cause) {
			setError(cause instanceof Error ? cause.message : String(cause));
		} finally {
			setClearing(null);
		}
	};

	return (
		<div className="rounded-xl border border-border/60 bg-muted/15 p-4" data-testid="browser-data-preference">
			<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:gap-6">
				<div className="min-w-0">
					<h3 className="text-[14px] font-medium text-foreground">
						{t("settings.general.browserData.title")}
					</h3>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
						{t("settings.general.browserData.body")}
					</p>
					<p className="mt-3 font-mono text-[12px] text-foreground">
						{t("settings.general.browserData.usage", {
							cache: format(usage.cacheBytes),
							total: format(usage.totalBytes),
						})}
					</p>
				</div>
				<div className="flex shrink-0 flex-wrap gap-2 sm:justify-end">
					<Button type="button" variant="outline" size="sm" disabled={clearing !== null} onClick={() => void clear(false)}>
						{clearing === "cache" ? <Loader2 className="size-3.5 animate-spin" /> : null}
						{t("settings.general.browserData.clearCache")}
					</Button>
					<Button type="button" variant="outline" size="sm" disabled={clearing !== null} onClick={() => void clear(true)}>
						{clearing === "everything" ? <Loader2 className="size-3.5 animate-spin" /> : null}
						{t("settings.general.browserData.clearEverything")}
					</Button>
				</div>
			</div>
			<p className="mt-3 text-[12px] leading-relaxed text-muted-foreground">
				{t("settings.general.browserData.clearEverythingHint")}
			</p>
			{cleared ? (
				<p className="mt-1 text-[12px] text-foreground" aria-live="polite">
					{t("settings.general.browserData.cleared", { cache: format(usage.cacheBytes) })}
				</p>
			) : null}
			{error ? <p className="mt-1 text-[12px] text-destructive">{error}</p> : null}
		</div>
	);
}
