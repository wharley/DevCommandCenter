import type { CodeViewReactOptions } from "@pierre/diffs/react";
import type { DccTheme } from "@/components/theme-provider";

/** Shared visual contract for every read-only DCC diff surface. */
export function workspaceDiffViewOptions<TAnnotation = undefined>(
	theme: DccTheme,
	inline: boolean,
): CodeViewReactOptions<TAnnotation> {
	return {
		disableFileHeader: true,
		diffStyle: inline ? "unified" : "split",
		diffIndicators: "classic",
		lineDiffType: "word",
		overflow: "scroll",
		theme: theme === "dark" ? "pierre-dark" : "pierre-light",
		themeType: theme,
		disableBackground: false,
		expandUnchanged: false,
		collapsedContextThreshold: 8,
		expansionLineCount: 20,
		hunkSeparators: "line-info",
		lineHoverHighlight: "both",
		tokenizeMaxLineLength: 2_000,
		maxLineDiffLength: 2_000,
		enableGutterUtility: false,
		enableLineSelection: false,
		disableErrorHandling: false,
		stickyHeaders: false,
	};
}
