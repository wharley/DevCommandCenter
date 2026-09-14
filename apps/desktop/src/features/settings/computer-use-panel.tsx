import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Check, Clock3, Globe2, Loader2, MonitorCog, ShieldAlert } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	disarmComputerUse,
	getComputerUseStatus,
	requestComputerUseAccess,
	type ComputerUseAccessKind,
	type ComputerUsePermission,
} from "@/lib/computer-use-api";
import { toast } from "sonner";

export const COMPUTER_USE_STATUS_QUERY_KEY = ["computer-use", "status"] as const;

function errorMessage(error: unknown, fallback: string): string {
	if (error instanceof Error) return error.message;
	if (typeof error === "string") return error;
	if (error && typeof error === "object" && "message" in error) {
		const message = (error as { message?: unknown }).message;
		if (typeof message === "string" && message.trim()) return message;
	}
	return fallback;
}

function formatRemaining(milliseconds: number): string {
	const seconds = Math.max(0, Math.ceil(milliseconds / 1000));
	return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

export function ComputerUsePanel({ sessionId }: { sessionId: string | null }) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [busyAction, setBusyAction] = useState<string | null>(null);
	const [now, setNow] = useState(() => Date.now());
	const statusQuery = useQuery({
		queryKey: [...COMPUTER_USE_STATUS_QUERY_KEY, sessionId],
		queryFn: () => getComputerUseStatus(sessionId),
		staleTime: 2_000,
		refetchInterval: 5_000,
		refetchOnWindowFocus: true,
	});
	const status = statusQuery.data;
	const remainingMs = status?.grant.armed
		? Math.max(0, status.grant.remainingMs - (now - statusQuery.dataUpdatedAt))
		: 0;
	const activeGrant = Boolean(status?.grant.armed && remainingMs > 0);

	useEffect(() => {
		if (!status?.grant.armed) return;
		const timer = window.setInterval(() => setNow(Date.now()), 1_000);
		return () => window.clearInterval(timer);
	}, [status?.grant.armed]);

	const refresh = async () => {
		await queryClient.invalidateQueries({ queryKey: [...COMPUTER_USE_STATUS_QUERY_KEY, sessionId] });
	};

	const perform = async (action: string, callback: () => Promise<unknown>) => {
		setBusyAction(action);
		try {
			await callback();
			await refresh();
		} catch (error) {
			toast.error(errorMessage(error, t("settings.computerUse.errors.generic")));
		} finally {
			setBusyAction(null);
		}
	};

	const requestPermission = (kind: ComputerUseAccessKind) => {
		void perform(kind, () => requestComputerUseAccess(kind));
	};

	const permissionRows: Array<{
		kind: ComputerUseAccessKind;
		permission: ComputerUsePermission;
		label: string;
		hint: string;
	}> = status
		? [
					{
						kind: "screenRecording",
						permission: status.screenRecording,
						label: t("settings.computerUse.permissions.screenRecording"),
						hint: t("settings.computerUse.permissions.screenRecordingHint"),
					},
					{
						kind: "accessibility",
						permission: status.accessibility,
						label: t("settings.computerUse.permissions.accessibility"),
						hint: t("settings.computerUse.permissions.accessibilityHint"),
					},
				]
		: [];

	const activeAppCount = status?.grant.allowedBundleIds.length ?? 0;
	const activeAppNames = useMemo(() => {
		if (!status?.grant.armed) return [];
		return status.targets
			.filter((target) => status.grant.allowedBundleIds.includes(target.bundleId))
			.map((target) => target.name);
	}, [status?.grant.allowedBundleIds, status?.grant.armed, status?.targets]);
	const browserExplanation = (
		<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
			<div className="flex items-start gap-3">
				<div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
					<Globe2 className="size-4" aria-hidden />
				</div>
				<div className="min-w-0">
					<h3 className="text-[14px] font-medium text-foreground">{t("settings.computerUse.browserTitle")}</h3>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">{t("settings.computerUse.browserHint")}</p>
					<p className="mt-2 text-[12px] leading-relaxed text-foreground">{t("settings.computerUse.browserPermissionsHint")}</p>
				</div>
			</div>
		</div>
	);

	if (statusQuery.isPending) {
		return (
			<section className="space-y-4" aria-busy="true">
				{browserExplanation}
				<div className="rounded-xl border border-border/60 p-4 text-[12px] text-muted-foreground">
					<Loader2 className="mr-2 inline size-3.5 animate-spin" aria-hidden />
					{t("settings.computerUse.loading")}
				</div>
			</section>
		);
	}

	if (!status) {
		return (
			<section className="space-y-4">
				{browserExplanation}
				<div className="rounded-xl border border-destructive/30 bg-destructive/5 p-4">
					<p className="text-[13px] text-destructive">{t("settings.computerUse.errors.status")}</p>
					<Button className="mt-3" variant="outline" size="sm" onClick={() => void statusQuery.refetch()}>
						{t("settings.computerUse.retry")}
					</Button>
				</div>
			</section>
		);
	}

	return (
		<section className="space-y-4">
			{browserExplanation}
			{statusQuery.isError ? (
				<div className="rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-[12px] text-destructive">
					{t("settings.computerUse.errors.statusCached")}
				</div>
			) : null}
			<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
				<div className="flex items-start justify-between gap-4">
					<div className="flex min-w-0 items-start gap-3">
						<div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
							<MonitorCog className="size-4" aria-hidden />
						</div>
						<div className="min-w-0">
							<div className="flex flex-wrap items-center gap-2">
								<h3 className="text-[14px] font-medium text-foreground">{t("settings.computerUse.externalAppsTitle")}</h3>
								<Badge variant="warn">{t("settings.computerUse.experimental")}</Badge>
							</div>
							<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">{t("settings.computerUse.promptHint")}</p>
						</div>
					</div>
					<Badge variant={status.supported ? "success" : "outline"} className="h-8 px-3 text-[12px] font-normal">
						{status.supported ? t("settings.computerUse.supported") : t("settings.computerUse.unsupported")}
					</Badge>
				</div>
				{!status.supported ? (
					<p className="mt-4 rounded-lg border border-border/60 bg-background/50 p-3 text-[12px] leading-relaxed text-muted-foreground">
						{status.platform === "macos" && status.minimumMacosVersion
							? t("settings.computerUse.unsupportedMacHint", { version: status.minimumMacosVersion })
							: t("settings.computerUse.unsupportedPlatformHint")}
					</p>
				) : null}
			</div>

			<div className="rounded-xl border border-border/60 p-4">
				<div className="flex items-start gap-3">
					<ShieldAlert className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
					<div>
						<h3 className="text-[14px] font-medium text-foreground">{t("settings.computerUse.permissionsTitle")}</h3>
						<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">{t("settings.computerUse.permissionsHint")}</p>
						{import.meta.env.DEV && status.platform === "macos" ? (
							<p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">{t("settings.computerUse.devPermissionHint")}</p>
						) : null}
					</div>
				</div>
				<div className="mt-4 grid gap-2">
					{permissionRows.map(({ kind, permission, label, hint }) => {
						const actionBusy = busyAction === kind;
						return (
							<div className="flex flex-col gap-3 rounded-lg border border-border/60 p-3 sm:flex-row sm:items-center sm:justify-between" key={kind}>
								<div className="min-w-0">
									<div className="flex items-center gap-2">
										{permission.granted ? <Check className="size-3.5 text-emerald-500" aria-hidden /> : null}
										<strong className="text-[13px] font-medium text-foreground">{label}</strong>
										<Badge variant={permission.granted ? "success" : "warn"}>{permission.granted ? t("settings.computerUse.permissionState.granted") : t("settings.computerUse.permissionState.notGranted")}</Badge>
									</div>
									<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">{hint}</p>
								</div>
								{!permission.granted && permission.canRequest ? (
									<Button size="sm" variant="outline" disabled={Boolean(busyAction)} onClick={() => requestPermission(kind)}>
										{actionBusy ? <Loader2 className="animate-spin" /> : null}
										{t("settings.computerUse.requestPermission")}
									</Button>
								) : null}
							</div>
						);
					})}
				</div>
			</div>

			<p className="rounded-xl border border-border/60 p-4 text-[12px] leading-relaxed text-muted-foreground">
				{t("settings.computerUse.conversationHint")}
			</p>

			{activeGrant ? (
				<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
					<div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
						<div className="flex items-start gap-3">
							<Clock3 className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
							<div>
								<h3 className="text-[14px] font-medium text-foreground">{t("settings.computerUse.activeTitle")}</h3>
								<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">{t("settings.computerUse.activeHint", { time: formatRemaining(remainingMs) })}</p>
								<p className="mt-1 text-[11px] text-muted-foreground">
									{activeAppNames.length > 0 ? activeAppNames.join(", ") : t("settings.computerUse.activeAppsCount", { count: activeAppCount })}
								</p>
							</div>
						</div>
						<Button variant="destructive" size="sm" disabled={busyAction === "disarm"} onClick={() => sessionId && void perform("disarm", () => disarmComputerUse({ sessionId }))}>
							{busyAction === "disarm" ? <Loader2 className="animate-spin" /> : null}
							{t("settings.computerUse.disable")}
						</Button>
					</div>
				</div>
			) : null}
		</section>
	);
}
