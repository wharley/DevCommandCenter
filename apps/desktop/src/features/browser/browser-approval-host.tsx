import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Globe2, ShieldAlert, Timer } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { listAllBrowserOpenRequests, resolveBrowserOpenRequest, type BrowserOpenRequest } from "./browser-api";
import { configureBrowserApprovalPanel, dismissBrowserApprovalPanel, onBrowserPanelAllow } from "./browser-approval-panel";

export type BrowserOpenApproval = BrowserOpenRequest;

type Props = {
	workspaceSessions: WorkspaceSessionSummary[];
	onApproved: (request: BrowserOpenRequest) => void;
};

const message = (error: unknown) => error instanceof Error ? error.message : String(error);
const remaining = (ms: number) => `${Math.max(0, Math.ceil(ms / 1000))}s`;

export function BrowserApprovalHost({ workspaceSessions, onApproved }: Props) {
	const { t } = useTranslation("common");
	const query = useQuery({
		queryKey: ["browser", "pending-open-requests"],
		queryFn: async () => ({ requests: await listAllBrowserOpenRequests() }),
		refetchInterval: 1000,
		refetchIntervalInBackground: true,
		staleTime: 0,
	});
	const request = query.data?.requests?.[0] ?? null;
	const [now, setNow] = useState(Date.now);
	const [busy, setBusy] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);
	const deciding = useRef<string | null>(null);
	useEffect(() => {
		void configureBrowserApprovalPanel({
			title: t("browser.approval.title"), description: t("browser.approval.description"),
			destination: t("browser.approval.destination"), conversation: t("browser.approval.conversation"),
			provider: t("browser.approval.provider"), reason: t("browser.approval.reason"),
			remaining: t("browser.approval.remaining", { time: "{{time}}" }),
			allow: t("browser.approval.allow"), deny: t("browser.approval.deny"),
		}, workspaceSessions.map(({ session, thread }) => ({
			workspaceId: session.workspaceId, sessionId: session.id, title: thread.title,
		}))).catch((err) => console.warn("Browser approval panel unavailable", err));
	}, [t, workspaceSessions]);
	const currentId = useRef<string | null>(null);
	currentId.current = request?.requestId ?? null;
	useEffect(() => {
		if (deciding.current !== request?.requestId) deciding.current = null;
		setBusy((current) => current && current !== request?.requestId ? null : current);
		if (!request) setBusy(null);
		setError(null);
	}, [request?.requestId]);
	useEffect(() => {
		if (!request) return;
		const timer = window.setInterval(() => setNow(Date.now()), 500);
		return () => window.clearInterval(timer);
	}, [request?.requestId]);
	const liveMs = request ? Math.max(0, request.expiresAtMs - Math.max(now, Date.now())) : 0;
	const session = useMemo(() => request && workspaceSessions.find((item) => item.session.id === request.sessionId && item.session.workspaceId === request.workspaceId), [request, workspaceSessions]);
	const decide = async (allowed: boolean, target = request) => {
		if (!target || deciding.current || target.expiresAtMs <= Date.now()) return;
		const id = target.requestId;
		deciding.current = id;
		setBusy(id); setError(null);
		try {
			void dismissBrowserApprovalPanel(id).catch((err) => console.warn("Could not hide Browser approval panel", err));
			if (allowed) {
				onApproved(target);
				// Keep the request in an opening state until the native surface ACKs.
				// The broker then removes it and the background poll closes the host.
				return;
			}
			await respondBrowserOpenRequest({ requestId: id, workspaceId: target.workspaceId, sessionId: target.sessionId, decision: "deny" });
			await query.refetch();
		} catch (err) {
			if (currentId.current === id) setError(message(err));
			deciding.current = null;
			setBusy((value) => value === id ? null : value);
		} finally {
			if (!allowed) {
				if (deciding.current === id) deciding.current = null;
				setBusy((value) => value === id ? null : value);
			}
		}
	};
	const decideRef = useRef(decide);
	decideRef.current = decide;
	useEffect(() => {
		let disposed = false;
		const subscription = onBrowserPanelAllow((id) => {
			// The main WebView may have been hidden. Refresh before accepting an
			// external click, and never apply a late click to the next queued request.
			void query.refetch().then((result) => {
				const fresh = result.data?.requests.find((item) => item.requestId === id);
				if (!disposed && !result.isError && fresh) void decideRef.current(true, fresh);
			});
		});
		void subscription.catch((err) => console.warn("Browser approval panel listener unavailable", err));
		return () => { disposed = true; void subscription.then((unlisten) => unlisten(), () => {}); };
	}, [query.refetch]);
	// Unmount the permission portal immediately once a decision starts. Waiting
	// for its exit animation can keep the native Browser occluded indefinitely
	// in a minimized WebView, preventing approved background agent operations.
	if (request && busy === request.requestId) return null;
	return <Dialog open={Boolean(request && liveMs > 0)} onOpenChange={(open) => { if (!open) void decide(false); }}>
		<DialogContent className="w-[min(92vw,520px)]" showCloseButton={!busy}>
			<DialogHeader>
				<DialogTitle className="flex items-center gap-2"><ShieldAlert className="size-4" />{t("browser.approval.title")}</DialogTitle>
				<DialogDescription>{t("browser.approval.description")}</DialogDescription>
			</DialogHeader>
			{request && <div className="space-y-4 text-sm">
				<div className="rounded-md border border-border/70 bg-muted/30 p-3"><div className="mb-1 text-xs text-muted-foreground">{t("browser.approval.destination")}</div><div className="break-all font-medium">{request.url}</div></div>
				<div className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs"><span className="text-muted-foreground">{t("browser.approval.conversation")}</span><span className="font-medium">{session?.thread.title ?? request.sessionId}</span><span className="text-muted-foreground">{t("browser.approval.provider")}</span><span>{request.providerId}</span></div>
				<div className="rounded-md border border-border/70 p-3"><div className="mb-1 text-xs text-muted-foreground">{t("browser.approval.reason")}</div><p className="whitespace-pre-wrap">{request.reason}</p></div>
				<div className="flex items-center gap-2 text-xs text-muted-foreground"><Globe2 className="size-3.5" /><Timer className="size-3.5" />{t("browser.approval.remaining", { time: remaining(liveMs) })}</div>
				{error && <p role="alert" className="text-xs text-destructive">{error}</p>}
			</div>}
			<DialogFooter><Button variant="outline" disabled={Boolean(busy)} onClick={() => void decide(false)}>{t("browser.approval.deny")}</Button><Button disabled={Boolean(busy)} onClick={() => void decide(true)}>{t("browser.approval.allow")}</Button></DialogFooter>
		</DialogContent>
	</Dialog>;
}

const respondBrowserOpenRequest = resolveBrowserOpenRequest;
