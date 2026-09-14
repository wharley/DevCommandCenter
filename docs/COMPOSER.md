# Rich text composer

Use the formatting bar to apply bold, italic, strikethrough, or inline code to selected text. Links are detected automatically when typing or pasting a URL. Click a link to open its menu, change its display text or destination, or remove the link while keeping its text. The link button also creates a link from selected text. `Cmd/Ctrl+K` remains reserved for global search.

## Lists and sending

At the beginning of a paragraph, type a marker followed by a space:

- `- `, `* `, or `+ ` starts a bullet list.
- `1. ` starts a numbered list. Other starting numbers are supported too.

Inside a list, **Enter** creates the next item. **Enter** on an empty item exits the list. **Shift+Enter** adds a line within the same item. To send from inside a list, use the Send button or **Cmd/Ctrl+Enter**.

Outside lists, **Enter** sends and **Shift+Enter** starts a new paragraph. This lets you write an introduction and then start a list on the next line. Undo also reverses automatic list creation.

## Drafts and outgoing prompts

The conversation draft preserves formatting, links, lists, and attachments. At submission, the editor converts formatting into Markdown, preserves custom link destinations, and writes attachments as `@path`. Nested items and multiline item content keep their indentation. Pasted source text remains text; only typed list markers trigger automatic list creation.
