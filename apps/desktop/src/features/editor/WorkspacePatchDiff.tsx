import { CodeView } from "@pierre/diffs/react";
import { useMemo } from "react";
import { useAppearance } from "@/components/theme-provider";
import { workspaceDiffContentHash } from "./workspace-changes-diff.logic";
import { workspaceDiffViewOptions } from "./workspace-diff-view-options";
import { parseWorkspacePatch } from "./workspace-patch-diff.logic";

export type WorkspacePatchDiffProps = {
	path: string;
	patch: string;
	className?: string;
};

const ITEM_ID = "dcc-workspace-patch";

/** Renders one captured Git patch with the same read-only visual system as the dock. */
export default function WorkspacePatchDiff({
	path,
	patch,
	className,
}: WorkspacePatchDiffProps) {
	const { theme } = useAppearance();
	const patchHash = useMemo(() => workspaceDiffContentHash(patch), [patch]);
	const fileDiff = useMemo(() => parseWorkspacePatch(patch), [patch]);
	const items = useMemo(
		() => [
			{
				id: ITEM_ID,
				type: "diff" as const,
				fileDiff,
				version: patchHash,
			},
		],
		[fileDiff, patchHash],
	);

	return (
		<div
			className={`relative min-h-0 min-w-0 flex-1 overflow-hidden bg-background ${className ?? ""}`}
			data-turn-review-diff={path}
		>
			<CodeView
				className="h-full min-h-0 min-w-0 overflow-x-hidden overflow-y-auto"
				items={items}
				disableWorkerPool
				options={workspaceDiffViewOptions(theme, true)}
			/>
		</div>
	);
}
