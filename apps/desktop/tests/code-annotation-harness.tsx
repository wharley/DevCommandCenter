// Actual editable/file/diff surfaces; synthetic snippets and local callbacks only.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider, useAppearance } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { WorkspaceFileEditor } from "@/features/editor/WorkspaceFileSurface";
import WorkspaceChangesDiff from "@/features/editor/WorkspaceChangesDiff";
import {
	DiffAnnotationPopover,
	type PendingAnnotation,
} from "@/features/editor/diff-annotation";
import type { DiffAnnotationPayload } from "@/features/editor/diff-types";
import "@/i18n/config";
import "@/styles/app.css";
const win = window as any;
win.annotationActions = [];
win.__TAURI_INTERNALS__ = {
	invoke: async (command: string) => {
		if (command === "plugin:app|set_app_theme") return;
		throw Error("Unexpected fixture IPC: " + command);
	},
};
const source =
	"export function welcome(name: string) {\n  return `Olá, ${name}!`;\n}\n\nexport const ready = true;\n";
const path = "src/features/review/components/welcome.ts";
function Fixture() {
	const [mode, setMode] = useState("diff");
	const [pending, setPending] = useState<PendingAnnotation | null>(null);
	const { theme, setTheme } = useAppearance();
	const annotate = ({ anchor, ...request }: DiffAnnotationPayload) =>
		setPending({ anchor, request: { ...request, path } });
	const record = (action: string, instruction: string, newSession = false) => {
		win.annotationActions.push({
			action,
			instruction,
			newSession,
			request: pending?.request,
		});
		setPending(null);
	};
	return (
		<div style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
			<nav style={{ display: "flex", flexWrap: "wrap", gap: 16, padding: 12 }}>
				{[
					["diff", "Diff"],
					["editor", "Editor"],
					["file", "Arquivo"],
				].map(([value, label]) => (
					<button key={value} onClick={() => setMode(value)}>
						{label}
					</button>
				))}
				<button onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
					Tema
				</button>
				<button
					onClick={() =>
						annotate({
							side: "original",
							startLine: 1,
							endLine: 200,
							snippet: source.repeat(200),
							anchor: { top: innerHeight - 5, left: innerWidth - 5 },
						})
					}
				>
					Seleção longa
				</button>
			</nav>
			<main
				style={{
					display: "flex",
					flex: 1,
					minHeight: 0,
					borderTop: "1px solid var(--border)",
				}}
			>
				{mode === "diff" ? (
					<WorkspaceChangesDiff
						path={path}
						originalText={source.replace("true", "false")}
						modifiedText={source}
						inline
						onAnnotate={annotate}
						annotateLabel="Comentar trecho"
					/>
				) : (
					<WorkspaceFileEditor
						key={mode}
						path={path}
						content={source}
						readOnly={mode === "file"}
						onAnnotate={annotate}
						annotateLabel="Comentar trecho"
					/>
				)}
			</main>
			{pending && (
				<DiffAnnotationPopover
					pending={pending}
					canEditInComposer
					canAddToReview
					onSubmit={(text, newSession) => record("submit", text, newSession)}
					onEditInComposer={(text) => record("composer", text)}
					onAddToReview={(text) => record("review", text)}
					onCancel={() => setPending(null)}
				/>
			)}
		</div>
	);
}
createRoot(document.getElementById("root")!).render(
	<ThemeProvider>
		<TooltipProvider>
			<Fixture />
		</TooltipProvider>
	</ThemeProvider>,
);
