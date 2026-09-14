import { useEffect } from "react";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	acknowledgeAppshots,
	activateAppshots,
	appshotErrorKey,
	getAppshotStatus,
	isAppshotsDesktop,
	pendingAppshots,
} from "@/lib/appshots-api";
import { appendAppshotsToDraft } from "../../appshots-draft";

export function AppshotsPlugin({
	draftKey,
	workspaceRoot,
	imagesSupported,
	disabled,
}: {
	draftKey: string;
	workspaceRoot: string | null;
	imagesSupported: boolean;
	disabled: boolean;
}) {
	const [editor] = useLexicalComposerContext();
	const { t } = useTranslation("common");
	useEffect(() => {
		if (!isAppshotsDesktop() || disabled) return;
		const owner = crypto.randomUUID();
		let active = true;
		let draining = false;
		let again = false;
		let notifyFailure = false;
		let activated = false;
		const unlisten: Array<() => void> = [];
		const report = (error: unknown) => {
			if (active)
				toast.error(t(appshotErrorKey(error)), { id: "appshots-error" });
		};
		const drain = async (notify = false) => {
			notifyFailure ||= notify;
			if (draining) {
				again = true;
				return;
			}
			draining = true;
			try {
				do {
					again = false;
					const shots = await pendingAppshots(draftKey);
					if (!active) return;
					const ids = appendAppshotsToDraft(
						editor,
						draftKey,
						workspaceRoot,
						imagesSupported,
						shots,
					);
					if (ids.length) {
						await acknowledgeAppshots(draftKey, ids);
						if (active) {
							if (!imagesSupported)
								toast.info(t("composer.attachments.imagesUnsupported"));
							editor.focus();
						}
					}
				} while (again && active);
			} catch (error) {
				// Mount/focus recovery is passive. Only an explicit capture should
				// interrupt the person with an error; leave failed deliveries pending.
				if (notifyFailure) report(error);
			} finally {
				draining = false;
				notifyFailure = false;
			}
		};
		// Also recover a pending capture after a transient delivery failure.
		const onFocus = () => {
			if (active) void drain();
		};
		void (async () => {
			try {
				// HMR can load this frontend against a binary predating Appshots.
				// Do not register delivery/focus hooks until the native API is ready.
				const status = await getAppshotStatus();
				if (!active || !status.supported) return;
				const ready = await listen<string>("appshots-ready", (event) => {
					if (event.payload === draftKey) void drain(true);
				});
				if (!active) {
					ready();
					return;
				}
				unlisten.push(ready);
				const error = await listen<string>("appshots-error", (event) =>
					report(event.payload),
				);
				if (!active) {
					error();
					return;
				}
				unlisten.push(error);
				await activateAppshots(owner, draftKey);
				activated = true;
				if (!active) {
					await activateAppshots(owner, null);
					return;
				}
				window.addEventListener("focus", onFocus);
				await drain();
			} catch {
				// Capability/setup failures are shown by the capture picker when
				// requested, not on every composer mount or application focus.
				unlisten.splice(0).forEach((stop) => stop());
			}
		})();
		return () => {
			active = false;
			unlisten.forEach((stop) => stop());
			window.removeEventListener("focus", onFocus);
			if (activated) void activateAppshots(owner, null).catch(() => {});
		};
	}, [draftKey, workspaceRoot, imagesSupported, disabled, editor, t]);
	return null;
}
