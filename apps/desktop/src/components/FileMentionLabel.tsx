import { FileCode2, FileHeadphone, FileText } from "lucide-react";
import { isAudioFilePath } from "@/lib/audio-attachment";
import { cn } from "@/lib/utils";
import { isCodeFileReference } from "./streamdown-link-presentation";

/** The folder that holds the file, so `collections/page.tsx` and `orders/page.tsx` differ. */
export function fileMentionParts(path: string) {
	const segments = path.replace(/\\/g, "/").split("/").filter(Boolean);
	const name = segments.pop() ?? path;
	return { name, folder: segments.pop() ?? null };
}

/**
 * How a mentioned workspace file reads, in the composer and in the timeline:
 * its type icon, the parent folder muted and the file name in front. The full
 * path stays in the tooltip.
 */
export function FileMentionLabel({ path, className }: { path: string; className?: string }) {
	const { name, folder } = fileMentionParts(path);
	const Icon = isAudioFilePath(name) ? FileHeadphone : isCodeFileReference(name) ? FileCode2 : FileText;
	return (
		<span className={cn("inline-flex min-w-0 items-center gap-1", className)}>
			<Icon className="size-3.5 shrink-0 opacity-70" strokeWidth={1.8} aria-hidden />
			<span className="min-w-0 truncate">
				{folder ? <span className="opacity-55">{folder}/</span> : null}
				<span className="font-medium">{name}</span>
			</span>
		</span>
	);
}
