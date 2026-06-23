import type { CodeRabbitFindingSeverity, WorkspacePrReviewComment } from "@dcc/contracts";
import { AlertCircle, FileCode2, LoaderCircle } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { useWorkspaceGitFilePreviewContent } from "./use-workspace-git-file-preview-content";

export type WorkspaceGitPreviewMachineAnnotation = {
	source: "coderabbit";
	severity: CodeRabbitFindingSeverity;
	side: "original" | "modified";
	startLine: number;
	endLine: number;
	title: string;
};

export type WorkspaceGitPreviewSelection = {
	group: "staged" | "unstaged" | "committed";
	path: string;
	name: string;
	status: string;
	baseBranch?: string | null;
	focusLine?: number | null;
	machineAnnotations?: WorkspaceGitPreviewMachineAnnotation[];
	reviewComments?: WorkspacePrReviewComment[];
};

type WorkspaceGitFilePreviewProps = {
	workspaceRoot: string | null;
	selection: WorkspaceGitPreviewSelection | null;
};

type MonacoRuntimeModule = typeof import("@/lib/monaco-runtime");
type MonacoDiffController = Awaited<
	ReturnType<MonacoRuntimeModule["createDiffEditor"]>
>;

function WorkspaceGitMonacoDiff({
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
	const [surfaceStatus, setSurfaceStatus] = useState<
		| { kind: "loading" }
		| { kind: "ready" }
		| { kind: "error"; message: string }
	>({ kind: "loading" });

	useEffect(() => {
		return () => {
			controllerRef.current?.dispose();
			controllerRef.current = null;
		};
	}, []);

	useLayoutEffect(() => {
		const host = hostRef.current;
		if (!host) {
			return;
		}

		const requestId = requestIdRef.current + 1;
		requestIdRef.current = requestId;
		let disposed = false;

		controllerRef.current?.dispose();
		controllerRef.current = null;
		host.replaceChildren();
		setSurfaceStatus({ kind: "loading" });

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
				setSurfaceStatus({ kind: "ready" });
			} catch (error) {
				if (disposed) {
					return;
				}

				setSurfaceStatus({
					kind: "error",
					message:
						error instanceof Error ? error.message : "Failed to load file preview",
				});
			}
		})();

		return () => {
			disposed = true;
		};
	}, [path]);

	useEffect(() => {
		if (!controllerRef.current) {
			return;
		}

		controllerRef.current.setTexts({
			originalText,
			modifiedText,
			inline,
		});
	}, [inline, modifiedText, originalText]);

	if (surfaceStatus.kind === "error") {
		return (
			<div className="flex min-h-[280px] flex-1 items-center justify-center rounded-md border border-destructive/30 bg-background/60 px-4 py-6 text-center text-[11px] text-destructive">
				<span className="inline-flex items-center gap-2">
					<AlertCircle className="size-3.5" />
					{surfaceStatus.message}
				</span>
			</div>
		);
	}

	return (
		<div className="relative flex min-h-[280px] flex-1 overflow-hidden rounded-md border border-border/50 bg-background/60">
			<div ref={hostRef} className="h-full min-h-0 flex-1" />
			{surfaceStatus.kind === "loading" ? (
				<div className="pointer-events-none absolute inset-0 flex items-center justify-center bg-background/70">
					<span className="inline-flex items-center gap-2 text-[11px] text-muted-foreground">
						<LoaderCircle className="size-3.5 animate-spin" />
						Loading file preview...
					</span>
				</div>
			) : null}
		</div>
	);
}

export function WorkspaceGitFilePreview({
	workspaceRoot,
	selection,
}: WorkspaceGitFilePreviewProps) {
	const query = useWorkspaceGitFilePreviewContent(
		selection && workspaceRoot
			? {
					workspaceRoot,
					relativePath: selection.path,
					status: selection.status,
					scope: selection.group,
					baseBranch: selection.baseBranch ?? null,
				}
			: null,
	);

	if (!selection) {
		return (
			<div className="flex min-h-[180px] flex-1 items-center justify-center rounded-md border border-dashed border-border/60 bg-background/40 px-4 py-6 text-center">
				<div className="max-w-[240px] space-y-2">
					<div className="mx-auto flex size-8 items-center justify-center rounded-full bg-muted/60 text-muted-foreground">
						<FileCode2 className="size-4" strokeWidth={1.8} />
					</div>
					<p className="text-[11px] leading-5 text-muted-foreground">
						Click a file in the Git tree to preview its code diff.
					</p>
				</div>
			</div>
		);
	}

	if (query.isPending) {
		return (
			<div className="flex min-h-[180px] flex-1 items-center justify-center rounded-md border border-border/50 bg-background/60 px-4 py-6 text-center text-[11px] text-muted-foreground">
				<span className="inline-flex items-center gap-2">
					<LoaderCircle className="size-3.5 animate-spin" />
					Loading file preview...
				</span>
			</div>
		);
	}

	if (query.isError) {
		return (
			<div className="flex min-h-[180px] flex-1 items-center justify-center rounded-md border border-destructive/30 bg-background/60 px-4 py-6 text-center text-[11px] text-destructive">
				<span className="inline-flex items-center gap-2">
					<AlertCircle className="size-3.5" />
					{(query.error as Error)?.message ?? "Failed to load file preview"}
				</span>
			</div>
		);
	}

	const snapshot = query.data;

	return (
		<div className="flex min-h-[180px] flex-1 flex-col overflow-hidden rounded-md border border-border/50 bg-background/60">
			<div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/45 px-3 py-2">
				<div className="min-w-0">
					<div className="truncate text-[11.5px] font-medium text-foreground">
						{selection.name}
					</div>
					<div className="truncate text-[10px] text-muted-foreground">
						{selection.path}
					</div>
				</div>
				<div className="flex flex-wrap items-center gap-1">
					<Badge variant="secondary" className="h-4 rounded-full px-1.5 text-[9.5px] font-semibold">
						{selection.group}
					</Badge>
					<Badge variant="outline" className="h-4 rounded-full px-1.5 text-[9.5px] font-semibold">
						{selection.status}
					</Badge>
					{selection.baseBranch ? (
						<Badge variant="outline" className="h-4 rounded-full px-1.5 text-[9.5px] font-semibold">
							{selection.baseBranch.replace(/^origin\//, "")}
						</Badge>
					) : null}
				</div>
			</div>
			<div className="border-b border-border/35 px-3 py-1.5 text-[10px] text-muted-foreground">
				Showing read-only code diff for the selected file.
			</div>
			{snapshot ? (
				<WorkspaceGitMonacoDiff
					key={`${selection.group}:${selection.path}:${selection.baseBranch ?? ""}`}
					path={selection.path}
					originalText={snapshot.originalText}
					modifiedText={snapshot.modifiedText}
					inline={snapshot.inline}
				/>
			) : (
				<div className="flex min-h-[180px] flex-1 items-center justify-center px-4 py-6 text-center text-[11px] text-muted-foreground">
					<span className="inline-flex items-center gap-2">
						<LoaderCircle className="size-3.5 animate-spin" />
						Loading file preview...
					</span>
				</div>
			)}
		</div>
	);
}
