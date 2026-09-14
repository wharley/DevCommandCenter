import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ShieldAlert, Timer } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import {
	getComputerUseStatus,
	listComputerUsePendingRequests,
	requestComputerUseAccess,
	respondComputerUseControlRequest,
	type ComputerUseAccessKind,
} from "@/lib/computer-use-api";

type ComputerUseApprovalHostProps = {
	workspaceSessions: WorkspaceSessionSummary[];
};

function errorMessage(error: unknown): string | null {
	if (typeof error === "string") return error;
	if (error instanceof Error) return error.message;
	if (error && typeof error === "object" && "message" in error) {
		const message = (error as { message?: unknown }).message;
		return typeof message === "string" ? message : null;
	}
	return null;
}

function shortId(value: string): string {
	return value.length > 12 ? `${value.slice(0, 8)}…${value.slice(-4)}` : value;
}

function formatRemaining(ms: number): string {
	const seconds = Math.max(0, Math.ceil(ms / 1000));
	const minutes = Math.floor(seconds / 60);
	const remainder = seconds % 60;
	return minutes > 0 ? `${minutes}:${String(remainder).padStart(2, "0")}` : `${remainder}s`;
}

export function ComputerUseApprovalHost({
	workspaceSessions,
}: ComputerUseApprovalHostProps) {
	const { t } = useTranslation("common");
	const pendingQuery = useQuery({
		queryKey: ["computer-use", "pending-requests"],
		queryFn: listComputerUsePendingRequests,
		refetchInterval: 1500,
		refetchIntervalInBackground: true,
		refetchOnWindowFocus: true,
		staleTime: 0,
	});
	const request = pendingQuery.data?.[0] ?? null;
	const statusQuery = useQuery({
		queryKey: ["computer-use", "approval-status", request?.requestId, request?.sessionId],
		queryFn: () => getComputerUseStatus(request?.sessionId ?? null),
		enabled: Boolean(request),
		refetchInterval: 1500,
		refetchIntervalInBackground: true,
		refetchOnWindowFocus: true,
		staleTime: 0,
	});
	const [selectedByRequest, setSelectedByRequest] = useState<Record<string, string[]>>({});
	const [busyDecision, setBusyDecision] = useState<string | null>(null);
	const [busyPermission, setBusyPermission] = useState<{
		kind: ComputerUseAccessKind;
		requestId: string | null;
	} | null>(null);
	const [accessError, setAccessError] = useState<string | null>(null);
	const [decisionError, setDecisionError] = useState<string | null>(null);
	const [now, setNow] = useState(() => Date.now());
	const currentRequestIdRef = useRef<string | null>(null);
	const expiredRequestIdRef = useRef<string | null>(null);
	const decidedRequestIdsRef = useRef(new Set<string>());
	const previousRequestRef = useRef<{ requestId: string; deadlineAt: number } | null>(null);
	currentRequestIdRef.current = request?.requestId ?? null;

	useEffect(() => {
		if (!request) return;
		const timer = window.setInterval(() => setNow(Date.now()), 1000);
		return () => window.clearInterval(timer);
	}, [request?.requestId]);

	useEffect(() => {
		if (!request) return;
		setNow(Date.now());
		setAccessError(null);
		setDecisionError(null);
	}, [request?.requestId]);

	useEffect(() => {
		const pendingIds = new Set((pendingQuery.data ?? []).map((item) => item.requestId));
		setSelectedByRequest((current) => {
			const next = Object.fromEntries(
				Object.entries(current).filter(([requestId]) => pendingIds.has(requestId)),
			);
			return Object.keys(next).length === Object.keys(current).length ? current : next;
		});
	}, [pendingQuery.data]);

	const matchingSession = useMemo(
		() => workspaceSessions.find(
			(summary) =>
				summary.session.id === request?.sessionId &&
				summary.session.workspaceId === request?.workspaceId,
		),
		[request?.sessionId, request?.workspaceId, workspaceSessions],
	);
	const status = statusQuery.data;
	const targets = useMemo(() => {
		const seen = new Set<string>();
		return (status?.targets ?? []).filter((target) => {
			if (seen.has(target.bundleId)) return false;
			seen.add(target.bundleId);
			return true;
		});
	}, [status?.targets]);
	const selectedBundleIds = request
		? (selectedByRequest[request.requestId] ?? [])
			.filter((bundleId) => targets.some((target) => target.bundleId === bundleId))
			.slice(0, 16)
		: [];
	const permissionReady = Boolean(
		status?.accessibility.granted && status.screenRecording.granted,
	);
	const remainingMs = request
		? Math.max(0, request.remainingMs - Math.max(0, now - pendingQuery.dataUpdatedAt))
		: 0;
	const requestDeadlineAt = request ? pendingQuery.dataUpdatedAt + request.remainingMs : null;
	useEffect(() => {
		const previous = previousRequestRef.current;
		if (previous && previous.requestId !== request?.requestId &&
			Date.now() >= previous.deadlineAt &&
			expiredRequestIdRef.current !== previous.requestId &&
			!decidedRequestIdsRef.current.has(previous.requestId)) {
			expiredRequestIdRef.current = previous.requestId;
			toast.info(t("settings.computerUse.approval.expired"));
		}
		previousRequestRef.current = request && requestDeadlineAt !== null
			? { requestId: request.requestId, deadlineAt: requestDeadlineAt }
			: null;
	}, [request?.requestId, requestDeadlineAt, t]);

	useEffect(() => {
		if (!request || remainingMs > 0 || expiredRequestIdRef.current === request.requestId || decidedRequestIdsRef.current.has(request.requestId)) return;
		expiredRequestIdRef.current = request.requestId;
		toast.info(t("settings.computerUse.approval.expired"));
	}, [request?.requestId, remainingMs, t]);
	const canAllow = Boolean(
		request &&
		status?.supported &&
		status.providerSupported &&
		status.runtimeAttached &&
		permissionReady &&
		targets.length > 0 &&
		selectedBundleIds.length > 0 &&
		remainingMs > 0 &&
		!statusQuery.isError &&
		!busyDecision,
	);
	const decide = async (allowed: boolean) => {
		if (!request || busyDecision || (allowed && !canAllow)) return;
		const requestId = request.requestId;
		decidedRequestIdsRef.current.add(requestId);
		setBusyDecision(requestId);
		try {
			await respondComputerUseControlRequest({
				requestId,
				allowed,
				allowedBundleIds: allowed ? selectedBundleIds : [],
			});
			await pendingQuery.refetch();
		} catch (error) {
			if (currentRequestIdRef.current === requestId) {
				setDecisionError(errorMessage(error) ?? t("settings.computerUse.errors.generic"));
			}
		} finally {
			setBusyDecision((current) => current === requestId ? null : current);
		}
	};

	const openPermissionSettings = async (kind: ComputerUseAccessKind) => {
		if (busyPermission) return;
		const requestId = currentRequestIdRef.current;
		setBusyPermission({ kind, requestId });
		setAccessError(null);
		try {
			await requestComputerUseAccess(kind);
			await statusQuery.refetch();
		} catch (error) {
			if (currentRequestIdRef.current === requestId) {
				setAccessError(errorMessage(error) ?? t("settings.computerUse.errors.generic"));
			}
		} finally {
			setBusyPermission((current) => current?.requestId === requestId && current.kind === kind ? null : current);
		}
	};

	return (
		<>
			<Dialog
			open={Boolean(request && remainingMs > 0)}
			onOpenChange={(open) => {
				if (!open) void decide(false);
			}}
		>
			<DialogContent
				className="flex max-h-[min(88vh,760px)] w-[min(92vw,560px)] max-w-[560px] flex-col overflow-hidden"
				showCloseButton={!busyDecision}
			>
				<DialogHeader>
					<DialogTitle className="flex items-center gap-2">
						<ShieldAlert className="size-4" />
						{t("settings.computerUse.approval.title")}
					</DialogTitle>
					<DialogDescription>
						{t("settings.computerUse.approval.description")}
					</DialogDescription>
				</DialogHeader>
				{request && (
					<div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
						<div className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
							<span className="text-muted-foreground">{t("settings.computerUse.approval.provider")}</span>
							<span className="font-medium">{request.providerId}</span>
							<span className="text-muted-foreground">{t("settings.computerUse.approval.conversation")}</span>
							<span className="font-medium">
								{matchingSession?.thread.title || t("settings.computerUse.approval.unknownConversation", { id: shortId(request.sessionId) })}
							</span>
							{!matchingSession && (
								<>
									<span className="text-muted-foreground">{t("settings.computerUse.approval.workspace")}</span>
									<span className="font-medium">{t("settings.computerUse.approval.unknownWorkspace", { id: shortId(request.workspaceId) })}</span>
								</>
							)}
						</div>

						<div className="rounded-md border border-border/70 bg-muted/30 p-3 text-sm">
							<div className="mb-1 text-xs font-medium text-muted-foreground">{t("settings.computerUse.approval.reason")}</div>
							<p className="whitespace-pre-wrap">{request.reason}</p>
						</div>

						<div className="flex items-center gap-2 text-xs text-muted-foreground">
							<Timer className="size-3.5" />
							{t("settings.computerUse.approval.remaining", { time: formatRemaining(remainingMs) })}
						</div>

						<div className="space-y-2">
							<div>
								<div className="text-sm font-medium">{t("settings.computerUse.approval.appsTitle")}</div>
								<div className="text-xs text-muted-foreground">{t("settings.computerUse.approval.appsHint")}</div>
							</div>
							{targets.length > 0 ? (
								<div className="space-y-1">
									{targets.map((target) => {
										const checked = selectedBundleIds.includes(target.bundleId);
										return (
											<label key={target.bundleId} className="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-sm hover:bg-muted/50">
												<input
													type="checkbox"
												checked={checked}
												 disabled={Boolean(busyDecision)}
												 onChange={() => setSelectedByRequest((current) => ({
													...current,
													[request.requestId]: checked
														? (current[request.requestId] ?? []).filter((id) => id !== target.bundleId)
														: [...(current[request.requestId] ?? []), target.bundleId],
												}))}
												/>
												<span>{target.name}</span>
											</label>
										);
									})}
								</div>
							) : (
								<div className="rounded border border-dashed border-border p-3 text-xs text-muted-foreground">
									{statusQuery.isPending
										? t("settings.computerUse.approval.loadingStatus")
										: t("settings.computerUse.approval.appsEmpty")}
								</div>
							)}
						</div>

						{status && !permissionReady && (
							<div className="space-y-2 rounded-md border border-border/70 p-3 text-xs">
								<div className="font-medium">{t("settings.computerUse.approval.permissions")}</div>
								<div className="flex flex-wrap gap-2">
									{(["accessibility", "screenRecording"] as const).filter((kind) => !status[kind].granted).map((kind) => (
										<Button key={kind} type="button" size="sm" variant="outline" disabled={!status[kind].canRequest || Boolean(busyPermission) || Boolean(busyDecision)} onClick={() => void openPermissionSettings(kind)}>
											{busyPermission?.kind === kind
														? t("settings.computerUse.loading")
														: t(`settings.computerUse.approval.${kind === "accessibility" ? "openAccessibility" : "openScreenRecording"}`)}
										</Button>
									))}
								</div>
							</div>
						)}
						{statusQuery.isError && <p className="text-xs text-muted-foreground">{t("settings.computerUse.approval.statusError")}</p>}
						{status && !status.supported && <p className="text-xs text-muted-foreground">{t("settings.computerUse.approval.unsupported")}</p>}
						{status && status.supported && (!status.providerSupported || !status.runtimeAttached) && <p className="text-xs text-muted-foreground">{t("settings.computerUse.approval.conversationNotReady")}</p>}
						{accessError && <p role="alert" className="text-xs text-destructive">{accessError}</p>}
						{decisionError && <p role="alert" className="text-xs text-destructive">{decisionError}</p>}
					</div>
				)}
				<DialogFooter>
					<Button type="button" variant="outline" onClick={() => void decide(false)} disabled={Boolean(busyDecision)}>
						{t("settings.computerUse.approval.deny")}
					</Button>
					<Button type="button" onClick={() => void decide(true)} disabled={!canAllow}>
						{t("settings.computerUse.approval.allow")}
					</Button>
				</DialogFooter>
			</DialogContent>
			</Dialog>
		</>
	);
}
