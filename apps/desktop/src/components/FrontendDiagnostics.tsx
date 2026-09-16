import { useState } from "react";
import i18n from "@/i18n/config";
import { exportFrontendErrors } from "@/lib/frontend-diagnostics";

/** Also works outside the app providers, on the root recovery screen. */
export function FrontendDiagnostics() {
	const [status, setStatus] = useState<"idle" | "copied" | "failed">("idle");
	const [report, setReport] = useState<string | null>(null);
	return (
		<div className="space-y-3">
			<button
				type="button"
				className="rounded-md border border-border px-3 py-2 text-sm"
				onClick={async () => {
					const text = exportFrontendErrors();
					setReport(text);
					try {
						await navigator.clipboard.writeText(text);
						setStatus("copied");
					} catch {
						setStatus("failed");
					}
				}}
			>
				{i18n.t("frontendRecovery.copy")}
			</button>
			{status !== "idle" && (
				<p role="status" className="text-sm text-muted-foreground">
					{i18n.t(`frontendRecovery.${status}`)}
				</p>
			)}
			{status === "failed" && report && (
				<textarea
					aria-label={i18n.t("frontendRecovery.report")}
					readOnly
					value={report}
					onFocus={(event) => event.currentTarget.select()}
					className="h-40 w-full rounded-md border border-border bg-background p-2 font-mono text-xs"
				/>
			)}
		</div>
	);
}
