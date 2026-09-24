import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { ArrowUpRight, Check, CirclePause, Cpu, LoaderCircle, MessageCircle, Plus, Power, RefreshCw, ShieldCheck } from "lucide-react";
import { LOCALE_STORAGE_KEY } from "@/i18n/config";
import { composeFromMenuBar, hideMenuBar, isMenuBarVisible, openFromMenuBar, quitFromMenuBar, readMenuBar, type MenuBarSnapshot, type MenuBarTask } from "./api";
import "./menu-bar.css";

const icons = { running: LoaderCircle, permission: ShieldCheck, input: MessageCircle, completed: Check, aborted: CirclePause };
const activeTask = (task: MenuBarTask) => ["running", "permission", "input"].includes(task.status);

export function MenuBar() {
	const { t, i18n } = useTranslation("common");
	const [snapshot, setSnapshot] = useState<MenuBarSnapshot | null>(null);
	const [error, setError] = useState(false);
	const [actionError, setActionError] = useState(false);
	const [busy, setBusy] = useState(false);
	const actionPending = useRef(false);
	const refresh = useRef<() => void>(() => {});

	useEffect(() => {
		let disposed = false;
		let visible = false;
		let generation = 0;
		let timer: ReturnType<typeof setTimeout> | undefined;
		let stop: (() => void) | undefined;
		let inFlight = false;
		const poll = async () => {
			if (disposed || !visible || inFlight) return;
			clearTimeout(timer);
			inFlight = true;
			const current = generation;
			try {
				const next = await readMenuBar();
				if (!disposed && visible && current === generation) { setSnapshot(next); setError(false); }
			} catch {
				if (!disposed && visible && current === generation) setError(true);
			} finally {
				inFlight = false;
				if (!disposed && visible) timer = setTimeout(() => void poll(), current === generation ? 3000 : 0);
			}
		};
		const setVisible = (next: boolean) => {
			visible = next;
			generation += 1;
			clearTimeout(timer);
			if (next) {
				setSnapshot(null); setError(false); setActionError(false);
				const locale = localStorage.getItem(LOCALE_STORAGE_KEY);
				if (locale === "en" || locale === "pt-BR") void i18n.changeLanguage(locale);
				void poll();
			}
		};
		// Subscribe before reading visibility: the first click may precede React.
		void listen<boolean>("menu-bar-visibility", (event) => setVisible(event.payload)).then(async (unlisten) => {
			if (disposed) { unlisten(); return; }
			stop = unlisten;
			const current = generation;
			const initial = await isMenuBarVisible();
			if (!disposed && current === generation) setVisible(initial);
		}).catch(() => { if (!disposed) setError(true); });
		refresh.current = () => void poll();
		const keydown = (event: KeyboardEvent) => {
			if (event.key === "Escape") void hideMenuBar().catch(() => setActionError(true));
		};
		window.addEventListener("keydown", keydown);
		return () => { disposed = true; clearTimeout(timer); stop?.(); window.removeEventListener("keydown", keydown); };
	}, [i18n]);

	const act = async (action: () => Promise<void>) => {
		if (actionPending.current) return;
		actionPending.current = true; setBusy(true); setActionError(false);
		try { await action(); } catch { setActionError(true); }
		finally { actionPending.current = false; setBusy(false); }
	};
	const tasks = snapshot?.tasks ?? [];
	const running = tasks.filter((task) => task.status === "running").length;
	const waiting = tasks.filter((task) => task.status === "permission" || task.status === "input").length;
	const metrics = snapshot?.metrics;
	const format = (value: number, digits = 0) => new Intl.NumberFormat(i18n.language, { maximumFractionDigits: digits }).format(value);
	const memory = metrics ? metrics.memoryBytes / 1024 ** 2 : null;
	const renderTask = (task: MenuBarTask) => {
		const Icon = icons[task.status];
		return <button className="menu-bar-task" key={task.workspaceId} disabled={busy} onClick={() => void act(() => openFromMenuBar(task))}>
			<span className={`menu-bar-status menu-bar-status-${task.status}`}><Icon size={16} className={task.status === "running" ? "animate-spin" : undefined} /></span>
			<span className="menu-bar-task-copy"><strong title={task.title}>{task.title}</strong><span title={task.project}>{task.project} · {t(`menuBar.status.${task.status}`)}</span></span>
			<ArrowUpRight size={14} className="menu-bar-task-arrow" />
		</button>;
	};
	return <main className="menu-bar" aria-label={t("menuBar.title")}>
		<header className="menu-bar-header"><div className="menu-bar-wordmark">DCC<span>{t("menuBar.title")}</span></div>
			<button className="menu-bar-icon-button" aria-label={t("menuBar.refresh")} onClick={() => refresh.current()}><RefreshCw size={14} /></button>
		</header>
		<div className="menu-bar-summary" aria-live="polite">
			{snapshot ? <><span><i className="menu-bar-dot" />{t("menuBar.running", { count: running })}</span><span className={waiting ? "menu-bar-attention" : ""}>{t("menuBar.waiting", { count: waiting })}</span></> : <span>{t(error ? "menuBar.unavailable" : "menuBar.loading")}</span>}
		</div>
		{(error || actionError) && <div role="alert" className="menu-bar-error">{t(actionError ? "menuBar.actionError" : "menuBar.stale")}</div>}
		<section className="menu-bar-tasks" aria-label={t("menuBar.tasks")}>
			{!snapshot && !error && <div className="menu-bar-empty"><LoaderCircle size={22} className="animate-spin" /><p>{t("menuBar.loading")}</p></div>}
			{snapshot && !tasks.some(activeTask) && <div className="menu-bar-empty"><Check size={24} /><strong>{t("menuBar.empty")}</strong><p>{t("menuBar.emptyHint")}</p></div>}
			{tasks.filter(activeTask).map(renderTask)}
			{tasks.some((task) => !activeTask(task)) && <h2>{t("menuBar.recent")}</h2>}
			{tasks.filter((task) => !activeTask(task)).map(renderTask)}
		</section>
		<section className="menu-bar-resources" aria-label={t("menuBar.resources")}>
			<div className="menu-bar-resource-title"><Cpu size={13} /><span>{t("menuBar.resources")}</span></div>
			<div className="menu-bar-resource-values"><span>CPU <strong>{metrics?.cpuPercent != null ? `${format(metrics.cpuPercent, 1)}%` : "—"}</strong></span><span>{t("menuBar.memory")} <strong>{memory == null ? "—" : memory >= 1024 ? `${format(memory / 1024, 1)} GiB` : `${format(memory)} MiB`}</strong></span></div>
			<p title={t("menuBar.metricsDetail")}>{t("menuBar.metricsScope")}</p>
		</section>
		<footer className="menu-bar-footer">
			<button className="menu-bar-compose" disabled={busy} onClick={() => void act(composeFromMenuBar)}><Plus size={16} />{t("menuBar.compose")}</button>
			<div className="menu-bar-footer-links"><button disabled={busy} onClick={() => void act(() => openFromMenuBar())}>{t("menuBar.open")}<ArrowUpRight size={13} /></button><button disabled={busy} onClick={() => void act(quitFromMenuBar)}><Power size={12} />{t("menuBar.quit")}</button></div>
		</footer>
	</main>;
}
