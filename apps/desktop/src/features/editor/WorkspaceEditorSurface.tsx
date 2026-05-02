import { X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { TrafficLightSpacer } from "@/components/chrome/traffic-light-spacer";
import { Button } from "@/components/ui/button";
import { ShortcutDisplay } from "@/features/shortcuts/shortcut-display";
import { useWorkspaceGitFilePreviewContent } from "@/features/inspector/use-workspace-git-file-preview-content";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";

type WorkspaceEditorSurfaceProps = {
	workspaceRoot: string | null;
	selection: WorkspaceGitPreviewSelection;
	onClose: () => void;
};

type MonacoRuntimeModule = typeof import("@/lib/monaco-runtime");
type MonacoDiffController = Awaited<
	ReturnType<MonacoRuntimeModule["createDiffEditor"]>
>;

function WorkspaceEditorDiff({
	path,
	originalText,
	modifiedText,
	inline,
}: {
	path: string;
	originalText: string;
	modifiedText: string;
	inline: boolean;
}) {
	const hostRef = useRef<HTMLDivElement | null>(null);
	const controllerRef = useRef<MonacoDiffController | null>(null);
	const requestIdRef = useRef(0);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);

	useEffect(
		() => () => {
			controllerRef.current?.dispose();
			controllerRef.current = null;
		},
		[],
	);

	useLayoutEffect(() => {
		const host = hostRef.current;
		if (!host) return;

		const requestId = requestIdRef.current + 1;
		requestIdRef.current = requestId;
		let disposed = false;

		controllerRef.current?.dispose();
		controllerRef.current = null;
		host.replaceChildren();
		setLoading(true);
		setError(null);

		void (async () => {
			try {
				const { createDiffEditor } = await import("@/lib/monaco-runtime");
				const controller = await createDiffEditor({
					container: host,
					path,
					originalText,
					modifiedText,
					inline,
				});

				if (disposed || requestId !== requestIdRef.current) {
					controller.dispose();
					return;
				}

				controllerRef.current = controller;
				setLoading(false);
			} catch (cause) {
				if (disposed) return;
				setError(cause instanceof Error ? cause.message : "Failed to load editor");
				setLoading(false);
			}
		})();

		return () => {
			disposed = true;
		};
	}, [inline, modifiedText, originalText, path]);

	useEffect(() => {
		controllerRef.current?.setTexts({
			originalText,
			modifiedText,
			inline,
		});
	}, [inline, modifiedText, originalText]);

	return (
		<div className="relative flex min-h-0 flex-1 overflow-hidden bg-background">
			<div ref={hostRef} className="h-full min-h-0 flex-1" />
			{loading ? (
				<div className="pointer-events-none absolute inset-0 flex items-center justify-center bg-background/70">
					<span className="text-[11px] text-muted-foreground">Loading editor...</span>
				</div>
			) : null}
			{error ? (
				<div className="absolute inset-0 flex items-center justify-center bg-background">
					<p className="text-[11px] text-destructive">{error}</p>
				</div>
			) : null}
		</div>
	);
}

export function WorkspaceEditorSurface({
	workspaceRoot,
	selection,
	onClose,
}: WorkspaceEditorSurfaceProps) {
	const query = useWorkspaceGitFilePreviewContent(
		workspaceRoot
			? {
					workspaceRoot,
					relativePath: selection.path,
					status: selection.status,
					scope: selection.group,
					baseBranch: selection.baseBranch ?? null,
				}
			: null,
	);
	const loadErrorMessage =
		(query.error as Error | null)?.message ?? "Failed to load file";

	if (query.isPending) {
		return null;
	}

	if (query.isError) {
		return null;
	}

	const snapshot = query.data;

	return (
		<section
			aria-label="Workspace editor surface"
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
					{selection.name}
				</div>
				<div className="flex shrink-0 items-center pr-2">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={onClose}
						aria-label="Close editor view"
						className="gap-1.5 px-2 text-muted-foreground hover:text-foreground"
					>
						<ShortcutDisplay hotkey="Escape" />
						<X className="size-3.5" strokeWidth={1.8} />
					</Button>
				</div>
			</div>
			<div className="relative flex min-h-0 flex-1 bg-background">
				{query.isError ? (
					<div className="absolute inset-0 flex items-center justify-center bg-background">
						<p className="text-[11px] text-destructive">
							{loadErrorMessage}
						</p>
					</div>
				) : query.isPending || !snapshot ? (
					<div className="absolute inset-0 flex items-center justify-center bg-background">
						<p className="text-[11px] text-muted-foreground">Loading file...</p>
					</div>
				) : (
					<WorkspaceEditorDiff
						path={selection.path}
						originalText={snapshot.originalText}
						modifiedText={snapshot.modifiedText}
						inline={snapshot.inline}
					/>
				)}
			</div>
		</section>
	);
}
