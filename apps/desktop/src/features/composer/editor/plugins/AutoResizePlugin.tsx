import { useEffect } from "react";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";

export function AutoResizePlugin({
	minHeight = 64,
	maxHeight = 240,
}: {
	minHeight?: number;
	maxHeight?: number;
}) {
	const [editor] = useLexicalComposerContext();

	useEffect(() => {
		const resize = () => {
			const root = editor.getRootElement();
			if (!root) {
				return;
			}

			root.style.height = "auto";
			const nextHeight = Math.min(maxHeight, Math.max(minHeight, root.scrollHeight));
			root.style.height = `${nextHeight}px`;
			root.style.overflowY = root.scrollHeight > maxHeight ? "auto" : "hidden";
		};

		resize();
		return editor.registerUpdateListener(() => {
			requestAnimationFrame(resize);
		});
	}, [editor, maxHeight, minHeight]);

	return null;
}
