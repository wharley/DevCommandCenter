import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
	KEEP_AWAKE_CHANGED_EVENT,
	readKeepAwake,
	setKeepAwakeMode,
	type KeepAwakeStatus,
} from "@/lib/keep-awake-api";
import {
	readUsageLimitSettings,
	saveUsageLimitSettings,
	USAGE_LIMIT_SETTINGS_QUERY_KEY,
} from "@/lib/usage-limit-api";

/** Keep-awake status, refreshed when the hold changes. `null` while unknown. */
export function useKeepAwakeStatus(): [KeepAwakeStatus | null, (next: KeepAwakeStatus) => void] {
	const [status, setStatus] = useState<KeepAwakeStatus | null>(null);
	useEffect(() => {
		let disposed = false;
		let stop: (() => void) | undefined;
		const refresh = () =>
			readKeepAwake()
				.then((next) => {
					if (!disposed) setStatus(next);
				})
				.catch(() => {});
		void refresh();
		void listen(KEEP_AWAKE_CHANGED_EVENT, () => void refresh())
			.then((unlisten) => {
				if (disposed) unlisten();
				else stop = unlisten;
			})
			.catch(() => {});
		return () => {
			disposed = true;
			stop?.();
		};
	}, []);
	return [status, setStatus];
}

/**
 * The Mac may sleep before the reset when it is not kept awake: then the
 * automatic resume never runs. Offers to turn on "Durante agentes".
 */
export function KeepAwakeForLimitWarning({
	message,
	className,
}: {
	message: string;
	className?: string;
}) {
	const { t } = useTranslation("common");
	const [status, setStatus] = useKeepAwakeStatus();
	const [saving, setSaving] = useState(false);
	if (!status?.supported || status.mode !== "off") return null;
	return (
		<div
			role="status"
			data-testid="usage-limit-keep-awake-warning"
			className={
				className ??
				"mt-3 flex flex-wrap items-center gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-[12px] text-amber-800 dark:text-amber-200"
			}
		>
			<span className="min-w-0 flex-1">{message}</span>
			<Button
				type="button"
				size="sm"
				variant="outline"
				className="h-7 text-[12px]"
				disabled={saving}
				onClick={async () => {
					setSaving(true);
					try {
						setStatus(await setKeepAwakeMode("during_agents"));
					} finally {
						setSaving(false);
					}
				}}
			>
				{t("settings.general.usageLimit.enableKeepAwake")}
			</Button>
		</div>
	);
}

/** Settings → General: the default for "Continuar sozinho se o limite acabar". */
export function UsageLimitPreference() {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [error, setError] = useState<string | null>(null);
	const query = useQuery({
		queryKey: [USAGE_LIMIT_SETTINGS_QUERY_KEY],
		queryFn: readUsageLimitSettings,
	});
	const enabled = query.data?.autoResume ?? false;

	const choose = async (autoResume: boolean) => {
		setError(null);
		try {
			const next = await saveUsageLimitSettings({ autoResume });
			queryClient.setQueryData([USAGE_LIMIT_SETTINGS_QUERY_KEY], next);
		} catch (cause) {
			setError(cause instanceof Error ? cause.message : String(cause));
		}
	};

	return (
		<div
			className="rounded-xl border border-border/60 bg-muted/15 p-4"
			data-testid="usage-limit-preference"
		>
			<div className="flex items-start justify-between gap-6">
				<div className="min-w-0">
					<h3 className="text-[14px] font-medium text-foreground">
						{t("settings.general.usageLimit.title")}
					</h3>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
						{t("settings.general.usageLimit.body")}
					</p>
				</div>
				<Switch
					checked={enabled}
					disabled={query.isLoading}
					onCheckedChange={(next) => void choose(next)}
					aria-label={t("settings.general.usageLimit.toggle")}
				/>
			</div>
			<p className="mt-3 text-[12px] text-foreground">
				{t("settings.general.usageLimit.toggle")}
				<span className="text-muted-foreground"> · {t("settings.general.usageLimit.hint")}</span>
			</p>
			{enabled ? (
				<KeepAwakeForLimitWarning message={t("settings.general.usageLimit.keepAwakeWarning")} />
			) : null}
			{error ? (
				<p role="alert" className="mt-2 text-[12px] text-destructive">
					{t("settings.general.usageLimit.saveFailed", { message: error })}
				</p>
			) : null}
		</div>
	);
}
