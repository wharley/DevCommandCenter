import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	appshotErrorKey,
	isAppshotsDesktop,
	shortcutFromEvent,
} from "@/lib/appshots-api";
import {
	quickStatus,
	setQuickShortcut,
	toggleQuickComposer,
	type QuickStatus,
} from "./api";

export function QuickComposerShortcut() {
	const { t } = useTranslation("common");
	const [status, setStatus] = useState<QuickStatus | null>(null);
	const [recording, setRecording] = useState(false);
	const [busy, setBusy] = useState(false);
	useEffect(() => {
		if (!isAppshotsDesktop()) return;
		let disposed = false;
		void quickStatus()
			.then((value) => {
				if (!disposed) setStatus(value);
			})
			.catch(() => {});
		return () => {
			disposed = true;
		};
	}, []);
	const save = async (shortcut: string | null) => {
		setBusy(true);
		try {
			await setQuickShortcut(shortcut);
			setStatus(await quickStatus());
			setRecording(false);
		} catch (cause) {
			toast.error(t(appshotErrorKey(cause)));
		} finally {
			setBusy(false);
		}
	};
	if (!status?.supported) return null;
	const display = status.shortcut
		?.replace(/Super|CommandOrControl/g, "⌘")
		.replace("Control", "⌃")
		.replace("Alt", "⌥")
		.replace("Shift", "⇧")
		.replace(/Key|Digit/g, "")
		.replaceAll("+", " ");
	return (
		<div className="space-y-2 rounded-xl border border-border p-3">
			<div className="flex flex-wrap items-center justify-between gap-2">
				<label
					htmlFor="quick-composer-shortcut"
					className="text-sm font-medium"
				>
					{t("quickComposer.title")}
				</label>
				<div className="flex items-center gap-2">
					<Button
						id="quick-composer-shortcut"
						type="button"
						size="sm"
						variant="outline"
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
							size="sm"
							variant="ghost"
							disabled={busy}
							onClick={() => void save(null)}
						>
							{t("appshots.disableShortcut")}
						</Button>
					)}
					<Button
						size="sm"
						variant="ghost"
						onClick={() =>
							void toggleQuickComposer().catch((cause) =>
								toast.error(String(cause)),
							)
						}
					>
						{t("quickComposer.open")}
					</Button>
				</div>
			</div>
			<p className="text-xs text-muted-foreground">
				{t("quickComposer.shortcutHint")}
			</p>
			{status.shortcutError && (
				<p role="alert" className="text-xs text-destructive">
					{t("appshots.errors.shortcutUnavailable")}
				</p>
			)}
		</div>
	);
}
