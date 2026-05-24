import { X } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { MissionSpecEntry } from "@dcc/contracts";
import { TrafficLightSpacer } from "@/components/chrome/traffic-light-spacer";
import { Button } from "@/components/ui/button";
import { shouldIgnoreGlobalShortcutTarget } from "@/features/shortcuts/shortcut-utils";
import { ShortcutDisplay } from "@/features/shortcuts/shortcut-display";

type WorkspaceMissionSpecSurfaceProps = {
	spec: MissionSpecEntry;
	onClose: () => void;
};

export function WorkspaceMissionSpecSurface({
	spec,
	onClose,
}: WorkspaceMissionSpecSurfaceProps) {
	const { t } = useTranslation();

	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.defaultPrevented || event.key !== "Escape") {
				return;
			}
			if (shouldIgnoreGlobalShortcutTarget(event.target)) {
				return;
			}

			event.preventDefault();
			onClose();
		};

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [onClose]);

	return (
		<section
			aria-label="Mission spec surface"
			data-focus-scope="editor"
			className="flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground"
		>
			<div
				className="flex h-9 items-center border-b border-border"
				data-tauri-drag-region
			>
				<TrafficLightSpacer side="left" width={86} />
				<div className="min-w-0 flex-1" data-tauri-drag-region />
				<div className="min-w-0 px-3 text-[11px] text-muted-foreground">
					{spec.name}
				</div>
				<div className="flex shrink-0 items-center pr-2">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={onClose}
						aria-label="Close spec view"
						className="gap-1.5 px-2 text-muted-foreground hover:text-foreground"
					>
						<ShortcutDisplay hotkey="Escape" />
						<X className="size-3.5" strokeWidth={1.8} />
					</Button>
				</div>
			</div>

			<div className="flex min-h-0 flex-1 flex-col overflow-hidden">
				<div className="border-b border-border/50 px-4 py-3">
					<p className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						{t("inspector.spec.kicker")}
					</p>
					<p className="mt-1 font-mono text-[12px] text-foreground">
						{spec.relativePath}
					</p>
					<p className="mt-2 text-[11px] leading-5 text-muted-foreground">
						{t("inspector.spec.surfaceHint")}
					</p>
				</div>

				<div className="min-h-0 flex-1 overflow-auto px-4 py-4">
					<pre className="min-w-0 whitespace-pre-wrap break-words [overflow-wrap:anywhere] rounded-2xl border border-border/50 bg-muted/20 p-4 text-[12px] leading-6 text-foreground">
						{spec.content}
					</pre>
				</div>
			</div>
		</section>
	);
}
