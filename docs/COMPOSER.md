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

## Link icons

Links use blue text and a decorative site favicon, both inline and in the link menu. The shared loader queries Google S2 using only the hostname, deduplicates concurrent loads, and caches up to 256 results (24 hours for success, one minute for failure). A globe appears during loading, on failure, and for local/IP addresses. Requests time out after five seconds. Icons never enter the Lexical document, copied text, persisted draft, or outgoing Markdown. An edited URL cannot receive an icon from an earlier destination's pending request.

### Reference implementations inspected

- [Synara `LinkChipIcon`](https://github.com/Emanuele-web04/synara/blob/dd88d9272f97e4dda5735281e73ce14de388ad25/apps/web/src/components/LinkChipIcon.tsx) combines GitHub-specific presentation with a shared site favicon component. Its [server resolver](https://github.com/Emanuele-web04/synara/blob/dd88d9272f97e4dda5735281e73ce14de388ad25/apps/server/src/siteFaviconCache.ts) caches by domain and tries Google S2, DuckDuckGo and a direct favicon URL.
- The locally installed Codex app bundle inspected on 2026-09-14 (`app-initial-bbbcf8946b60.js`) uses a known-service domain registry for composer icons, with a globe for unrecognized services. Its general favicon component separately supports Google S2, but the composer icon decoration uses the service registry. These are observations of that installed build, not a claim that both apps use identical implementations.

DCC uses generic hostname-based favicon loading for all public sites, with no GitHub-specific CSS rules or service allowlist.
