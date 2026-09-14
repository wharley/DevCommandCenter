import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
	Camera,
	Check,
	ChevronLeft,
	ChevronRight,
	Loader2,
	RefreshCw,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { openExternal } from "@/lib/shell-api";
import {
	appshotErrorKey,
	appshotTargetKey,
	attachAppshots,
	getAppshotStatus,
	previewAppshot,
	requestAppshotAccess,
	type AppshotPreview,
	type AppshotStatus,
} from "@/lib/appshots-api";
import { AppshotsShortcut } from "@/features/settings/appshots-shortcut";

const PAGE_SIZE = 6;
type PreviewResult = AppshotPreview | { error: string };

export function AppshotsDialog({
	draftKey,
	onClose,
}: {
	draftKey: string;
	onClose: () => void;
}) {
	const { t } = useTranslation("common");
	const [status, setStatus] = useState<AppshotStatus | null>(null);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const [page, setPage] = useState(0);
	const [selected, setSelected] = useState<string[]>([]);
	const [previews, setPreviews] = useState<Record<string, PreviewResult>>({});
	const cache = useRef<Record<string, PreviewResult>>({});
	const alive = useRef(true);
	const refreshId = useRef(0);
	useEffect(() => {
		alive.current = true;
		return () => {
			alive.current = false;
		};
	}, []);
	const refresh = useCallback(async () => {
		const id = ++refreshId.current;
		setLoading(true);
		setError(null);
		try {
			const next = await getAppshotStatus(true);
			if (!alive.current || id !== refreshId.current) return;
			cache.current = {};
			setPreviews({});
			setSelected([]);
			setPage(0);
			setStatus(next);
		} catch (cause) {
			if (alive.current && id === refreshId.current)
				setError(appshotErrorKey(cause));
		} finally {
			if (alive.current && id === refreshId.current) setLoading(false);
		}
	}, []);
	useEffect(() => {
		void refresh();
	}, [refresh]);
	// Permission changes happen in System Settings. Recheck when returning to DCC.
	useEffect(() => {
		if (!status || status.screenRecording) return;
		const onFocus = () => void refresh();
		window.addEventListener("focus", onFocus);
		return () => window.removeEventListener("focus", onFocus);
	}, [status, refresh]);
	const visible = useMemo(
		() => status?.targets.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE) ?? [],
		[status, page],
	);
	useEffect(() => {
		if (loading) return;
		let active = true;
		// Bound native work: only this page, one capture at a time, no polling.
		void (async () => {
			for (const target of visible) {
				if (!active) break;
				const key = appshotTargetKey(target);
				if (cache.current[key]) continue;
				let result: PreviewResult;
				try {
					result = await previewAppshot(draftKey, target);
				} catch (cause) {
					result = { error: appshotErrorKey(cause) };
				}
				if (!active) break;
				cache.current[key] = result;
				setPreviews((previous) => ({ ...previous, [key]: result }));
			}
		})();
		return () => {
			active = false;
		};
	}, [visible, draftKey, loading]);
	const attach = async () => {
		const ids = selected
			.map((key) => previews[key])
			.flatMap((preview) => (preview && "id" in preview ? [preview.id] : []));
		if (!ids.length || ids.length !== selected.length) return;
		setBusy(true);
		setError(null);
		try {
			await attachAppshots(draftKey, ids);
			if (alive.current) onClose();
		} catch (cause) {
			if (alive.current) {
				setError(appshotErrorKey(cause));
				setBusy(false);
			}
		}
	};
	const requestAccess = async () => {
		setBusy(true);
		try {
			await requestAppshotAccess();
			await openExternal(
				"x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
			);
			await refresh();
		} catch (cause) {
			toast.error(t(appshotErrorKey(cause)));
		} finally {
			if (alive.current) setBusy(false);
		}
	};
	const canAttach =
		selected.length > 0 &&
		selected.every((key) => previews[key] && "id" in previews[key]!);
	return (
		<Dialog
			open
			onOpenChange={(open) => {
				if (!open && !busy) onClose();
			}}
		>
			<DialogContent
				className="flex max-h-[min(90vh,850px)] flex-col overflow-hidden sm:max-w-2xl"
				showCloseButton={!busy}
			>
				<DialogHeader className="shrink-0">
					<DialogTitle className="flex items-center gap-2">
						<Camera className="size-4" />
						{t("appshots.title")}
					</DialogTitle>
					<DialogDescription>{t("appshots.description")}</DialogDescription>
				</DialogHeader>
				<div className="-mx-1 min-h-0 space-y-4 overflow-y-auto px-1">
					{error && (
						<p role="alert" className="text-sm text-destructive">
							{t(error)}
						</p>
					)}
					{loading ? (
						<p
							role="status"
							className="flex items-center gap-2 py-8 text-muted-foreground"
						>
							<Loader2 className="size-4 animate-spin" />
							{t("appshots.loading")}
						</p>
					) : !status ? (
						<Button variant="outline" onClick={() => void refresh()}>
							{t("appshots.refresh")}
						</Button>
					) : !status.supported ? (
						<p className="py-6 text-sm text-muted-foreground">
							{t("appshots.errors.unsupported")}
						</p>
					) : !status.screenRecording ? (
						<div className="space-y-3 rounded-xl border border-border p-5">
							<p className="font-medium">{t("appshots.permissionTitle")}</p>
							<p className="text-sm text-muted-foreground">
								{t("appshots.permissionHint")}
							</p>
							<Button disabled={busy} onClick={() => void requestAccess()}>
								{t("appshots.grantPermission")}
							</Button>
						</div>
					) : (
						<>
							<div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
								<span>
									{t("appshots.selectionCount", {
										count: selected.length,
										max: PAGE_SIZE,
									})}
								</span>
								<Button
									variant="ghost"
									size="sm"
									disabled={busy}
									onClick={() => void refresh()}
								>
									<RefreshCw className="size-3.5" />
									{t("appshots.refresh")}
								</Button>
							</div>
							{!visible.length && (
								<p className="py-8 text-center text-sm text-muted-foreground">
									{t("appshots.empty")}
								</p>
							)}
							<div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
								{visible.map((target) => {
									const key = appshotTargetKey(target);
									const preview = previews[key];
									const checked = selected.includes(key);
									return (
										<button
											key={key}
											type="button"
											aria-pressed={checked}
											disabled={
												busy ||
												(!!preview && "error" in preview) ||
												(!checked && selected.length >= PAGE_SIZE)
											}
											onClick={() =>
												setSelected((previous) =>
													checked
														? previous.filter((item) => item !== key)
														: [...previous, key],
												)
											}
											className={cn(
												"group relative overflow-hidden rounded-xl border bg-muted/30 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
												checked
													? "border-primary ring-1 ring-primary"
													: "border-border hover:border-muted-foreground/50",
											)}
										>
											<div className="flex aspect-video items-center justify-center bg-black/5">
												{preview && "dataUrl" in preview ? (
													<img
														src={preview.dataUrl}
														alt={target.title || target.name}
														className="h-full w-full object-contain"
													/>
												) : preview && "error" in preview ? (
													<span className="p-3 text-xs text-muted-foreground">
														{t(preview.error)}
													</span>
												) : (
													<Loader2
														aria-label={t("appshots.loadingPreview")}
														className="size-5 animate-spin text-muted-foreground"
													/>
												)}
											</div>
											<div className="space-y-0.5 px-3 py-2">
												<p className="truncate text-xs font-medium">
													{target.name}
												</p>
												<p
													className="truncate text-[11px] text-muted-foreground"
													title={target.title}
												>
													{target.title || t("appshots.untitled")}
												</p>
											</div>
											{checked && (
												<span className="absolute right-2 top-2 rounded-full bg-primary p-1 text-primary-foreground">
													<Check className="size-3" />
												</span>
											)}
										</button>
									);
								})}
							</div>
							{status.targets.length > PAGE_SIZE && (
								<div className="flex items-center justify-center gap-3">
									<Button
										variant="ghost"
										size="icon-sm"
										aria-label={t("appshots.previous")}
										disabled={busy || page === 0}
										onClick={() => setPage(page - 1)}
									>
										<ChevronLeft className="size-4" />
									</Button>
									<span className="text-xs text-muted-foreground">
										{page + 1} / {Math.ceil(status.targets.length / PAGE_SIZE)}
									</span>
									<Button
										variant="ghost"
										size="icon-sm"
										aria-label={t("appshots.next")}
										disabled={
											busy || (page + 1) * PAGE_SIZE >= status.targets.length
										}
										onClick={() => setPage(page + 1)}
									>
										<ChevronRight className="size-4" />
									</Button>
								</div>
							)}
						</>
					)}
					{status?.supported && <AppshotsShortcut />}
				</div>
				<DialogFooter className="shrink-0">
					<Button variant="outline" disabled={busy} onClick={onClose}>
						{t("appshots.cancel")}
					</Button>
					<Button
						disabled={busy || loading || !canAttach}
						onClick={() => void attach()}
					>
						{busy && <Loader2 className="size-4 animate-spin" />}
						{t("appshots.attach", { count: selected.length })}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
