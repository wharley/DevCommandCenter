import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	appshotErrorKey,
	getAppshotStatus,
	setAppshotShortcut,
	shortcutFromEvent,
	type AppshotStatus,
} from "@/lib/appshots-api";

export function AppshotsShortcut() {
	const { t } = useTranslation("common");
	const [status, setStatus] = useState<AppshotStatus | null>(null);
	const [recording, setRecording] = useState(false);
	const [busy, setBusy] = useState(false);
	useEffect(() => {
		let active = true;
		void getAppshotStatus()
			.then((value) => {
				if (active) setStatus(value);
			})
			.catch(() => {});
		return () => {
			active = false;
		};
	}, []);
	const save = async (shortcut: string | null) => {
		setBusy(true);
		try {
			await setAppshotShortcut(shortcut);
			setStatus(await getAppshotStatus());
			setRecording(false);
		} catch (error) {
			toast.error(t(appshotErrorKey(error)));
		} finally {
			setBusy(false);
		}
	};
	if (!status?.supported) return null;
	const display = status.shortcut
		?.replace("CommandOrControl", "⌘")
		.replace("Super", "⌘")
		.replace("Control", "⌃")
		.replace("Alt", "⌥")
		.replace("Shift", "⇧")
		.replace(/Key|Digit/g, "")
		.replaceAll("+", " ");
	return (
		<div className="space-y-2 rounded-xl border border-border p-3">
			<div className="flex flex-wrap items-center justify-between gap-2">
				<label htmlFor="appshots-shortcut" className="text-sm font-medium">
					{t("appshots.shortcutTitle")}
				</label>
				<div className="flex items-center gap-2">
					<Button
						id="appshots-shortcut"
						type="button"
						variant="outline"
						size="sm"
						disabled={busy}
						onBlur={() => setRecording(false)}
						onClick={() => setRecording(true)}
						onKeyDown={(event) => {
							if (!recording) return;
							event.preventDefault();
							event.stopPropagation();
							if (event.key === "Escape") {
								setRecording(false);
								return;
							}
							const shortcut = shortcutFromEvent(event);
							if (shortcut) void save(shortcut);
						}}
					>
						{recording
							? t("appshots.pressShortcut")
							: (display ?? t("appshots.shortcutDisabled"))}
					</Button>
					{status.shortcut && (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							disabled={busy}
							onClick={() => void save(null)}
						>
							{t("appshots.disableShortcut")}
						</Button>
					)}
				</div>
			</div>
			<p className="text-xs text-muted-foreground">
				{t("appshots.shortcutHint")}
			</p>
			{status.shortcutError && (
				<p role="alert" className="text-xs text-destructive">
					{t("appshots.errors.shortcutUnavailable")}
				</p>
			)}
		</div>
	);
}
