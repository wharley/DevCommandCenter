import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, GitBranch, LoaderCircle, LockKeyhole, Plus, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { LocalBranchesOutput } from "@dcc/contracts";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { workspaceLocalBranches, workspaceSwitchLocalBranch } from "@/lib/workspace-api";

export function LocalBranchPicker({ workspaceId, fallbackBranch, conversationStarted, busy, onBusyChange }: {
	workspaceId: string;
	fallbackBranch: string | null;
	conversationStarted: boolean;
	busy: boolean;
	onBusyChange: (busy: boolean) => void;
}) {
	const { t } = useTranslation("common");
	const client = useQueryClient();
	const queryKey = ["workspace-local-branches", workspaceId];
	const [open, setOpen] = useState(false);
	const [search, setSearch] = useState("");
	const [creating, setCreating] = useState(false);
	const [name, setName] = useState("");
	const [switching, setSwitching] = useState(false);
	const [refreshing, setRefreshing] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [refreshError, setRefreshError] = useState<string | null>(null);
	const inFlight = useRef(false);
	const mounted = useRef(true);
	const query = useQuery({
		queryKey,
		queryFn: () => workspaceLocalBranches({ workspaceId, refresh: false }),
		refetchInterval: 5_000,
		refetchOnWindowFocus: "always",
		retry: false,
	});
	const data = query.data;
	const locked = conversationStarted || data?.hasConversation === true;
	const disabled = locked || busy || switching || data?.agentRunning === true || !data || query.isError;
	const branch = data ? data.currentBranch ?? t("localBranch.detached") : fallbackBranch;
	const mismatch = data?.hasConversation && data.currentBranch !== data.taskBranch;
	const reason = locked ? t("localBranch.locked") : data?.agentRunning ? t("localBranch.agentRunning") : t("localBranch.choose");

	useEffect(() => {
		mounted.current = true;
		return () => { mounted.current = false; onBusyChange(false); };
	}, [onBusyChange]);
	useEffect(() => {
		if (locked || busy || data?.agentRunning) setOpen(false);
	}, [locked, busy, data?.agentRunning]);
	useEffect(() => {
		void client.invalidateQueries({ queryKey: ["workspace-local-branches", workspaceId] });
	}, [conversationStarted, client, workspaceId]);

	async function refresh() {
		if (refreshing) return;
		setRefreshing(true);
		setRefreshError(null);
		try {
			const result = await workspaceLocalBranches({ workspaceId, refresh: true });
			if (mounted.current) setRefreshError(result.refreshError);
		} catch (error) {
			if (mounted.current) setRefreshError(String(error));
		} finally {
			await client.invalidateQueries({ queryKey: ["workspace-local-branches", workspaceId] });
			if (mounted.current) setRefreshing(false);
		}
	}

	async function select(reference: string | null, newBranch: string | null = null) {
		if (disabled || inFlight.current || !data) return;
		inFlight.current = true;
		setSwitching(true);
		onBusyChange(true);
		setError(null);
		try {
			await client.cancelQueries({ queryKey });
			const result: LocalBranchesOutput = await workspaceSwitchLocalBranch({
				workspaceId, expectedBranch: data.currentBranch, reference, newBranch,
			});
			client.setQueryData(queryKey, result);
			await client.invalidateQueries({ predicate: (query) =>
				["workspaces", "workspaceGitStatus", "workspaceGitBranchDiff", "workspace-local-branches"].includes(String(query.queryKey[0])),
			});
			if (mounted.current) { setOpen(false); setCreating(false); setName(""); }
		} catch (error) {
			if (mounted.current) setError(String(error));
			await client.invalidateQueries({ queryKey });
		} finally {
			inFlight.current = false;
			if (mounted.current) { setSwitching(false); onBusyChange(false); }
		}
	}

	return <div className="flex min-w-0 max-w-sm flex-col items-end gap-1">
		<Popover open={open} onOpenChange={(value) => {
			if (value && disabled) return;
			setOpen(value);
			if (value) { setSearch(""); setCreating(false); setError(null); void refresh(); }
		}}>
			<PopoverTrigger asChild>
				<button type="button" disabled={disabled} title={reason} aria-label={`${t("localBranch.choose")}: ${branch ?? ""}`}
					className="flex max-w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-[10.5px] hover:bg-muted/45 disabled:cursor-default disabled:opacity-70">
					{switching || query.isLoading ? <LoaderCircle className="size-3.5 shrink-0 animate-spin" /> : <GitBranch className="size-3.5 shrink-0" />}
					<span className="truncate">{branch}</span>
					{locked ? <LockKeyhole className="size-3 shrink-0" /> : null}
				</button>
			</PopoverTrigger>
			<PopoverContent side="top" align="end" className="w-80 max-w-[calc(100vw-2rem)] p-2">
				<div className="flex items-center gap-1">
					<Input autoFocus aria-label={t("localBranch.search")} placeholder={t("localBranch.search")} value={search} onChange={(e) => setSearch(e.target.value)} className="h-8 text-xs" />
					<Button type="button" variant="ghost" size="icon-sm" title={t("localBranch.refresh")} aria-label={t("localBranch.refresh")} disabled={refreshing || switching} onClick={() => void refresh()}>
						<RefreshCw className={`size-3.5 ${refreshing ? "animate-spin" : ""}`} />
					</Button>
				</div>
				{refreshError ? <p role="status" className="mt-2 text-xs text-amber-600" title={refreshError}>{t("localBranch.refreshFailed")}</p> : null}
				<div className="mt-2 max-h-64 overflow-y-auto">
					{[false, true].map((remote) => {
						const entries = data?.branches.filter((entry) => entry.remote === remote && entry.name.toLocaleLowerCase().includes(search.toLocaleLowerCase())) ?? [];
						return entries.length ? <div key={String(remote)} role="group" aria-label={t(remote ? "localBranch.remote" : "localBranch.local")}>
							<p className="px-2 py-1 text-[10px] text-muted-foreground">{t(remote ? "localBranch.remote" : "localBranch.local")}</p>
							{entries.map((entry) => <button type="button" key={entry.reference} disabled={disabled} title={entry.name}
								onClick={() => void select(entry.reference)} className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs hover:bg-muted disabled:opacity-50">
								<GitBranch className="size-3.5 shrink-0" /><span className="flex-1 truncate">{entry.name}</span>
								{!remote && entry.name === data?.currentBranch ? <Check className="size-3.5 shrink-0" /> : null}
							</button>)}
						</div> : null;
					})}
					{!data?.branches.some((entry) => entry.name.toLocaleLowerCase().includes(search.toLocaleLowerCase())) ? <p className="p-2 text-xs text-muted-foreground">{t("localBranch.empty")}</p> : null}
				</div>
				<div className="mt-2 border-t pt-2">
					{creating ? <form onSubmit={(event) => { event.preventDefault(); void select(null, name.trim()); }} className="space-y-2">
						<p className="text-xs text-muted-foreground">{t("localBranch.from", { branch })}</p>
						<Input autoFocus aria-label={t("localBranch.name")} placeholder="feature/minha-branch" value={name} onChange={(event) => setName(event.target.value)} disabled={switching} className="h-8 text-xs" />
						<Button type="submit" size="sm" disabled={disabled || !name.trim()} className="w-full">{t("localBranch.create")}</Button>
					</form> : <button type="button" disabled={disabled} onClick={() => { setCreating(true); setName(search); }} className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs hover:bg-muted">
						<Plus className="size-3.5" />{t("localBranch.create")}
					</button>}
				</div>
				{error ? <p role="alert" className="mt-2 break-words text-xs text-destructive">{error}</p> : null}
			</PopoverContent>
		</Popover>
		{mismatch ? <p role="alert" className="text-right text-[11px] text-amber-600">{t("localBranch.mismatch", { expected: data.taskBranch, actual: data.currentBranch ?? t("localBranch.detached") })}</p> : null}
		{query.isError ? <button type="button" onClick={() => void query.refetch()} className="text-[11px] text-destructive" title={String(query.error)}>{t("localBranch.retry")}</button> : null}
	</div>;
}
