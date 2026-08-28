import { AlertCircle, LoaderCircle } from "lucide-react";
import {
	Component,
	lazy,
	Suspense,
	type ErrorInfo,
	type ReactNode,
} from "react";
import type { WorkspaceChangesDiffProps } from "./WorkspaceChangesDiff";
import type { WorkspacePatchDiffProps } from "./WorkspacePatchDiff";
import { workspaceDiffContentHash } from "./workspace-changes-diff.logic";

const WorkspaceChangesDiff = lazy(() => import("./WorkspaceChangesDiff"));
const WorkspacePatchDiff = lazy(() => import("./WorkspacePatchDiff"));

class DiffSurfaceErrorBoundary extends Component<
	{ children: ReactNode; resetKey: string },
	{ error: Error | null }
> {
	state = { error: null as Error | null };

	static getDerivedStateFromError(error: Error) {
		return { error };
	}

	componentDidUpdate(previous: Readonly<{ resetKey: string }>) {
		if (previous.resetKey !== this.props.resetKey && this.state.error) {
			this.setState({ error: null });
		}
	}

	componentDidCatch(_error: Error, _info: ErrorInfo) {
		// The shell remains usable and the error is rendered locally below.
	}

	render() {
		if (this.state.error) {
			return (
				<div className="flex min-h-[180px] flex-1 items-center justify-center bg-background px-4 py-6 text-center text-[11px] text-destructive">
					<span className="inline-flex items-center gap-2">
						<AlertCircle className="size-3.5" />
						{this.state.error.message || "Failed to render file diff"}
					</span>
				</div>
			);
		}
		return this.props.children;
	}
}

export function WorkspaceChangesDiffLoader(props: WorkspaceChangesDiffProps) {
	const resetKey = `${props.path}:${workspaceDiffContentHash(props.originalText)}:${workspaceDiffContentHash(props.modifiedText)}`;
	return (
		<DiffSurfaceErrorBoundary resetKey={resetKey}>
			<Suspense
				fallback={
					<div className="flex min-h-[180px] flex-1 items-center justify-center bg-background px-4 py-6 text-[11px] text-muted-foreground">
						<span className="inline-flex items-center gap-2">
							<LoaderCircle className="size-3.5 animate-spin" />
							Loading file diff...
						</span>
					</div>
				}
			>
				<WorkspaceChangesDiff {...props} />
			</Suspense>
		</DiffSurfaceErrorBoundary>
	);
}

export function WorkspacePatchDiffLoader(props: WorkspacePatchDiffProps) {
	const resetKey = `${props.path}:${workspaceDiffContentHash(props.patch)}`;
	return (
		<DiffSurfaceErrorBoundary resetKey={resetKey}>
			<Suspense
				fallback={
					<div className="flex min-h-[180px] flex-1 items-center justify-center bg-background px-4 py-6 text-[11px] text-muted-foreground">
						<span className="inline-flex items-center gap-2">
							<LoaderCircle className="size-3.5 animate-spin" />
							Loading file diff...
						</span>
					</div>
				}
			>
				<WorkspacePatchDiff {...props} />
			</Suspense>
		</DiffSurfaceErrorBoundary>
	);
}
