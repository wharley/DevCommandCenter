import { useEffect } from "react";
import { AutoLinkNode, LinkNode } from "@lexical/link";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import type { MutationListener } from "lexical";
import { loadSiteFavicon } from "@/lib/site-favicon";

/** Decorate existing anchors without adding content to selection, drafts or Markdown. */
export function ComposerLinkIconsPlugin() {
	const [editor] = useLexicalComposerContext();
	useEffect(() => {
		let active = true;
		const decorate: MutationListener = (mutations) => {
			for (const [key, mutation] of mutations) {
				if (mutation === "destroyed") continue;
				const element = editor.getElementByKey(key);
				if (!(element instanceof HTMLAnchorElement)) continue;
				const href = element.href;
				delete element.dataset.faviconSrc;
				element.style.removeProperty("--composer-link-favicon");
				void loadSiteFavicon(href).then((src) => {
					if (
						!active ||
						!src ||
						editor.getElementByKey(key) !== element ||
						element.href !== href
					)
						return;
					element.style.setProperty("--composer-link-favicon", `url("${src}")`);
					element.dataset.faviconSrc = src;
				});
			}
		};
		const cleanups = [LinkNode, AutoLinkNode].map((node) =>
			editor.registerMutationListener(node, decorate, {
				skipInitialization: false,
			}),
		);
		return () => {
			active = false;
			for (const cleanup of cleanups) cleanup();
		};
	}, [editor]);
	return null;
}
