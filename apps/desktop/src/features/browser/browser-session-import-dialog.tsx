import { useEffect, useMemo, useRef, useState } from "react";
import { LoaderCircle, UserRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { BROWSER_OCCLUDER_ATTRIBUTE } from "./browser-occlusion";
import {
	importBrowserSession,
	listBrowserSessionImportProfiles,
	type BrowserSessionImportProfile,
} from "./browser-api";

type Props = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number | null;
	currentUrl: string | null;
	onImported: () => void;
};

function siteFromUrl(value: string | null): string | null {
	if (!value) return null;
	try { return new URL(value).host || null; } catch { return null; }
}

function displayBrowserName(browser: string) {
	if (browser.trim().toLowerCase() === "arc") return "Arc";
	if (browser.trim().toLowerCase() === "chrome") return "Google Chrome";
	return browser;
}

export function BrowserSessionImportDialog({ open, onOpenChange, workspaceId, sessionId, lifecycleToken, currentUrl, onImported }: Props) {
	const { t } = useTranslation("common");
	const site = useMemo(() => siteFromUrl(currentUrl), [currentUrl]);
	const [profiles, setProfiles] = useState<BrowserSessionImportProfile[]>([]);
	const [selected, setSelected] = useState<BrowserSessionImportProfile | null>(null);
	const [loading, setLoading] = useState(false);
	const [importing, setImporting] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const scopeKey = `${workspaceId}\u001f${sessionId ?? ""}\u001f${lifecycleToken ?? ""}\u001f${currentUrl ?? ""}`;
	const currentScope = useMemo(() => ({ key: scopeKey, open }), [open, scopeKey]);
	const currentScopeRef = useRef(currentScope);
	currentScopeRef.current = currentScope;
	const scopeGenerationRef = useRef(0);

	useEffect(() => {
		scopeGenerationRef.current += 1;
		if (!open || lifecycleToken === null) return;
		let cancelled = false;
		setLoading(true); setError(null); setSelected(null); setImporting(false);
		void listBrowserSessionImportProfiles({ workspaceId, sessionId, lifecycleToken })
			.then((next) => {
				if (cancelled) return;
				setProfiles(next.profiles);
				if (!next.supported && next.message) setError(next.message);
			})
			.catch((reason: unknown) => { if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason)); })
			.finally(() => { if (!cancelled) setLoading(false); });
		return () => { cancelled = true; };
	}, [currentUrl, lifecycleToken, open, sessionId, site, workspaceId]);

	const importSelected = () => {
		if (!currentUrl || !site || lifecycleToken === null) return;
		if (!selected || importing) return;
		const expectedScope = scopeKey;
		const expectedGeneration = scopeGenerationRef.current;
		setImporting(true); setError(null);
		void importBrowserSession({ workspaceId, sessionId, lifecycleToken, profileId: selected.id, currentUrl })
			.then((result) => {
				if (scopeGenerationRef.current !== expectedGeneration || currentScopeRef.current.key !== expectedScope || !currentScopeRef.current.open) return;
				if (result.imported > 0) {
					toast.success(t("browser.sessions.result", { imported: result.imported, skipped: result.skipped }));
					onOpenChange(false); onImported(); return;
				}
				setError(result.message || t("browser.sessions.notImported"));
			})
			.catch((reason: unknown) => {
				if (scopeGenerationRef.current === expectedGeneration && currentScopeRef.current.key === expectedScope && currentScopeRef.current.open) setError(reason instanceof Error ? reason.message : String(reason));
			})
			.finally(() => {
				if (scopeGenerationRef.current === expectedGeneration && currentScopeRef.current.key === expectedScope && currentScopeRef.current.open) setImporting(false);
			});
	};

	return <Dialog open={open} onOpenChange={(nextOpen) => { if (!importing) onOpenChange(nextOpen); }}>
		<DialogContent className="w-[min(92vw,520px)]" data-dcc-browser-occluder={BROWSER_OCCLUDER_ATTRIBUTE}>
			<DialogHeader>
				<DialogTitle className="flex items-center gap-2"><UserRound className="size-4" />{t("browser.sessions.title")}</DialogTitle>
				<DialogDescription>{t("browser.sessions.description")}</DialogDescription>
			</DialogHeader>
			<div className="space-y-3 text-sm">
				<div className="rounded-md border border-border/70 bg-muted/30 p-3">
					<p className="text-xs text-muted-foreground">{t("browser.sessions.site")}</p>
					<p className="mt-1 break-all font-medium">{site ?? t("browser.sessions.noSite")}</p>
				</div>
				<p className="text-xs text-muted-foreground">{t("browser.sessions.manualHint")}</p>
				{loading ? <p className="flex items-center gap-2 text-xs text-muted-foreground"><LoaderCircle className="size-3.5 animate-spin" />{t("browser.sessions.loading")}</p> : null}
				{!loading && profiles.length === 0 ? <p className="rounded-md border border-dashed p-3 text-xs text-muted-foreground">{t("browser.sessions.empty")}</p> : null}
				{profiles.map((profile) => <button
					key={profile.id}
					type="button"
					disabled={importing}
					onClick={() => { if (!importing) setSelected(profile); }}
					className={`w-full rounded-md border p-3 text-left text-sm transition-colors ${selected?.id === profile.id ? "border-cyan-500 bg-cyan-500/10" : "border-border hover:bg-muted/50"}`}
				>
					<span className="block font-medium">{displayBrowserName(profile.browser)}</span>
					<span className="block text-xs text-muted-foreground">{profile.name}</span>
				</button>)}
				{selected ? <p className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-xs">{t("browser.sessions.confirm", { source: displayBrowserName(selected.browser), site: site ?? t("browser.sessions.thisSite") })}</p> : null}
				{error ? <p role="alert" className="text-xs text-destructive">{error}</p> : null}
			</div>
			<DialogFooter>
				<Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={importing}>{t("browser.sessions.manual")}</Button>
				<Button type="button" onClick={importSelected} disabled={!selected || importing || !site || lifecycleToken === null}>{importing ? <LoaderCircle className="size-3.5 animate-spin" /> : null}{t("browser.sessions.import")}</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>;
}
