// Synthetic attachment previews and real Lexical draft/editor behavior. No user files.
import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { PlainTextPlugin } from "@lexical/react/LexicalPlainTextPlugin";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	$createParagraphNode,
	$createTextNode,
	$getRoot,
	UNDO_COMMAND,
	type LexicalEditor,
} from "lexical";
import { ComposerContextReview } from "@/features/composer/ComposerContextReview";
import {
	FileBadgeNode,
	$createFileBadgeNode,
} from "@/features/composer/editor/file-badge-node";
import {
	ImageBadgeNode,
	$createImageBadgeNode,
} from "@/features/composer/editor/image-badge-node";
import {
	PastedSnippetBadgeNode,
	$createPastedSnippetBadgeNode,
} from "@/features/composer/editor/pasted-snippet-badge-node";
import { DraftPersistencePlugin } from "@/features/composer/editor/plugins/DraftPersistencePlugin";
import { SubmitPlugin } from "@/features/composer/editor/plugins/SubmitPlugin";
import { readComposerPrompt } from "@/features/composer/editorOps";
import { TooltipProvider } from "@/components/ui/tooltip";
import "@/i18n/config";
import "@/styles/app.css";

const win = window as any;
let editor: LexicalEditor;
win.__TAURI_INTERNALS__ = {
	invoke: async (command: string, args: any) => {
		if (command !== "preview_composer_attachment")
			throw Error(`Unexpected fixture IPC: ${command}`);
		if (win.previewFailure) throw Error("fixture failure");
		await new Promise((resolve) => setTimeout(resolve, 100));
		return args.filePath.endsWith(".png")
			? {
					kind: "image",
					content: null,
					reason: null,
					dataUrl:
						"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==",
				}
			: {
					kind: "text",
					content: "export const session = { persist: true, retry: 3 };",
					reason: null,
					dataUrl: null,
				};
	},
};
function BindEditor() {
	const [current] = useLexicalComposerContext();
	useEffect(() => {
		editor = current;
	}, [current]);
	return null;
}
const send = () => {
	win.contextSendCount = (win.contextSendCount ?? 0) + 1;
	win.contextPrompt = readComposerPrompt(editor);
};
function Fixture() {
	const [dark, setDark] = useState(true);
	const [draftKey, setDraftKey] = useState("fixture.context.a");
	document.documentElement.classList.toggle("dark", dark);
	const seed = () =>
		editor.update(() => {
			const root = $getRoot();
			root.clear();
			root.append(
				$createParagraphNode().append(
					$createTextNode("Revise a sessão usando "),
					$createFileBadgeNode("src/auth/config.ts"),
					$createTextNode(" e "),
					$createFileBadgeNode("src/api/config.ts"),
					$createImageBadgeNode("design/capture.png"),
					$createPastedSnippetBadgeNode(
						"Preserve o comportamento existente.\nInclua navegação por teclado e estados de carregamento.",
					),
				),
			);
		});
	return (
		<TooltipProvider>
			<main className="mx-auto flex min-h-screen max-w-3xl flex-col justify-center gap-6 px-8 py-12 text-foreground">
				<div>
					<p className="mb-3 text-xs tracking-widest text-muted-foreground">
						DCC · COMPOSIÇÃO
					</p>
					<h1 className="text-3xl tracking-tight">
						Tudo pronto para a próxima mensagem.
					</h1>
				</div>
				<div className="flex gap-4 text-xs text-muted-foreground">
					<button onClick={seed}>Adicionar contexto</button>
					<button onClick={() => setDark((value) => !value)}>
						Trocar tema
					</button>
					<button
						onClick={() =>
							setDraftKey((key) =>
								key.endsWith("a") ? "fixture.context.b" : "fixture.context.a",
							)
						}
					>
						Trocar conversa
					</button>
					<button
						onClick={() => editor.dispatchCommand(UNDO_COMMAND, undefined)}
					>
						Desfazer
					</button>
					<button
						onClick={() => {
							win.previewFailure = !win.previewFailure;
						}}
					>
						Simular falha
					</button>
				</div>
				<div className="dcc-composer-surface rounded-2xl border border-border bg-sidebar p-5">
					<LexicalComposer
						initialConfig={{
							namespace: "context-fixture",
							nodes: [FileBadgeNode, ImageBadgeNode, PastedSnippetBadgeNode],
							onError(error) {
								throw error;
							},
						}}
					>
						<ComposerContextReview
							workspaceRoot="/fixture/project"
							draftKey={draftKey}
						/>
						<PlainTextPlugin
							contentEditable={
								<ContentEditable
									className="min-h-32 text-sm leading-8 outline-none"
									aria-label="Mensagem de teste"
								/>
							}
							ErrorBoundary={LexicalErrorBoundary}
						/>
						<HistoryPlugin />
						<BindEditor />
						<DraftPersistencePlugin draftKey={draftKey} />
						<SubmitPlugin isDisabled={false} onSubmit={send} />
					</LexicalComposer>
					<div className="mt-5 flex justify-between text-xs text-muted-foreground">
						<span>Claude · Sonnet</span>
						<button
							className="rounded-lg bg-primary px-4 py-2 text-primary-foreground"
							onClick={send}
						>
							Enviar
						</button>
					</div>
				</div>
			</main>
		</TooltipProvider>
	);
}
createRoot(document.getElementById("root")!).render(<Fixture />);
