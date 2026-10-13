import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Coffee } from "lucide-react";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
	KEEP_AWAKE_CHANGED_EVENT,
	readKeepAwake,
	setKeepAwakeMode,
	keepAwakeReasonKey,
	type KeepAwakeMode,
	type KeepAwakeStatus,
} from "@/lib/keep-awake-api";

const MODES: Array<{ value: KeepAwakeMode; key: "off" | "always" | "duringAgents" }> = [
	{ value: "off", key: "off" },
	{ value: "always", key: "always" },
	{ value: "during_agents", key: "duringAgents" },
];

/** Settings → General. Hidden where the platform cannot hold the Mac awake. */
export function KeepAwakePreference() {
	const { t } = useTranslation("common");
	const [status, setStatus] = useState<KeepAwakeStatus | null>(null);
	const [saving, setSaving] = useState(false);
	const [error, setError] = useState<string | null>(null);

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

	if (!status?.supported) return null;

	const choose = async (mode: KeepAwakeMode) => {
		if (mode === status.mode || saving) return;
		setSaving(true);
		setError(null);
		try {
			setStatus(await setKeepAwakeMode(mode));
		} catch (cause) {
			setError(cause instanceof Error ? cause.message : String(cause));
		} finally {
			setSaving(false);
		}
	};
	const selected = MODES.find((mode) => mode.value === status.mode) ?? MODES[0]!;
	const reason = status.active ? keepAwakeReasonKey(status.active.reason) : null;

	return (
		<div className="rounded-xl border border-border/60 bg-muted/15 p-4" data-testid="keep-awake-preference">
			<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:gap-6">
				<div className="min-w-0">
					<h3 className="text-[14px] font-medium text-foreground">
						{t("settings.general.keepAwake.title")}
					</h3>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
						{t("settings.general.keepAwake.body")}
					</p>
				</div>
				<ToggleGroup
					type="single"
					value={status.mode}
					disabled={saving}
					aria-label={t("settings.general.keepAwake.title")}
					onValueChange={(value) => {
						const mode = MODES.find((candidate) => candidate.value === value);
						if (mode) void choose(mode.value);
					}}
					className="shrink-0 gap-1 self-start"
				>
					{MODES.map((mode) => (
						<ToggleGroupItem
							key={mode.value}
							value={mode.value}
							className="h-8 rounded-lg border border-border/60 px-3 text-[12px] font-medium text-muted-foreground data-[state=on]:bg-accent data-[state=on]:text-foreground"
						>
							{t(`settings.general.keepAwake.modes.${mode.key}`)}
						</ToggleGroupItem>
					))}
				</ToggleGroup>
			</div>
			<p className="mt-3 text-[12px] text-foreground">
				{t(`settings.general.keepAwake.modeHints.${selected.key}`)}
			</p>
			{status.mode !== "off" ? (
				<p className="mt-1 flex items-center gap-1.5 text-[12px] text-muted-foreground" aria-live="polite">
					{reason ? (
						<>
							<Coffee className="size-3.5 text-foreground" aria-hidden />
							{t("settings.general.keepAwake.activeNow", {
								reason: t(reason.key, { count: reason.count, time: reason.time }),
							})}
						</>
					) : (
						t("settings.general.keepAwake.idle")
					)}
				</p>
			) : null}
			<p className="mt-3 text-[12px] leading-relaxed text-muted-foreground">
				{t("settings.general.keepAwake.lid")}
			</p>
			{error ? (
				<p role="alert" className="mt-2 text-[12px] text-destructive">
					{t("settings.general.keepAwake.saveFailed", { message: error })}
				</p>
			) : null}
		</div>
	);
}
