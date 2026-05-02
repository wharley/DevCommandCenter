# DCC UX/UI Blueprint — Helmor visual clone, t3code organization

> **Status**: source of truth for the UX/UI evolution of `apps/desktop`.
> Branch: `evolution-dcc-desing`.
>
> **Read this entire document before changing any visual file in `apps/desktop/src/`.** It is self-contained: any agent (or human) following it should produce output indistinguishable from the helmor reference, organized like t3code, without re-reading the helmor or t3code repos.

---

## 0. How to use this blueprint

1. **Visual reference** = `helmor-main` (project at `~/projetos/helmor-main`). All colors, paddings, classNames, JSX skeletons here are **copied verbatim** from helmor — do not invent new tokens or shapes.
2. **Organization reference** = `t3code-main`. We apply t3code's *file naming and split conventions* (logic/view split, `.browser.tsx`, store/lib placement) **inside** `apps/desktop/src/`. We do **not** create a new `apps/web` package.
3. **Phase order is non-negotiable**. Do not skip ahead. If a phase reveals a missing primitive, add it at the bottom of that phase, not the next.
4. **Verbatim policy**: when this doc gives a `className` string or a CSS variable value, paste it as-is. If you find yourself "improving" the spacing or rounding a different way, stop — the helmor look is the result of *exactly* those values together.
5. **What this blueprint is NOT**:
   - Not a license to delete existing DCC backend wiring (Tauri commands, providers, sessions). Visual surface only.
   - Not a green light to install every helmor dependency. Add deps **as each phase needs them** (Phase 1 = Radix + tailwindcss-animate; Phase 2 = Lexical + Streamdown; etc.).
   - Not a 5000-line monolithic refactor. Files split per t3code rules from day one.
6. **When in doubt**: prefer copying helmor's class string over inventing one. The "design system" *is* the class strings.

---

## 1. Stack confirmation

Both projects use the same base stack — DCC is missing only some dependencies.

| Concern | Helmor (reference) | DCC (current) | Action |
|---|---|---|---|
| Bundler | Vite | Vite | ✓ keep |
| Framework | React 19 | React 19 | ✓ keep |
| Shell | Tauri 2 | Tauri 2 | ✓ keep |
| Tailwind | v4 (`@theme` in CSS, no config file) | v4 expected | ✓ confirm v4 in `apps/desktop` |
| Fonts | `@fontsource-variable/geist` + `geist-mono` | already declared | ✓ keep |
| Icons | `lucide-react` (primary), `@primer/octicons-react`, `@lobehub/icons` | `lucide-react` only | install primer + lobehub when needed |
| Primitives | full `@radix-ui/*` set | only `@radix-ui/react-label` | install full set in Phase 1 |
| Editor | `lexical`, `@lexical/react` | none | install in Phase 2 |
| Markdown | `streamdown`, `@streamdown/code` | none | install in Phase 2 |
| Toasts | `sonner` | `sonner` ✓ | ✓ keep |
| Cmdk | `cmdk` | `cmdk` ✓ | ✓ keep |
| Motion | `motion` (used in subtle places only) | `motion` ✓ | ✓ keep |
| State | `zustand` | `zustand` ✓ | ✓ keep |
| Query | `@tanstack/react-query` | ✓ | ✓ keep |
| Variants | `class-variance-authority` | ✓ | ✓ keep |
| Util | `clsx`, `tailwind-merge` | ✓ | ✓ keep |

---

## 2. Design tokens (PASTE VERBATIM)

Tailwind v4 is configured **inside CSS** via `@theme`. There is no `tailwind.config.js`. Place the global stylesheet at `apps/desktop/src/styles/app.css` (or update the existing `styles/`). Import it once from `main.tsx`.

### 2.1 Font + base imports

```css
@import "@fontsource-variable/geist";
@import "@fontsource-variable/geist-mono";
@import "tailwindcss";
@import "tw-animate-css";
```

### 2.2 `@theme` block — fonts, radii, animations

```css
@theme {
  --font-sans: "Geist Variable", "SF Pro Display", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  --font-mono: "Geist Mono Variable", ui-monospace, "SF Mono", SFMono-Regular, Menlo, Monaco, Consolas, monospace;

  --radius-sm: calc(0.625rem * 0.6);
  --radius-md: calc(0.625rem * 0.8);
  --radius-lg: 0.625rem;
  --radius-xl: calc(0.625rem * 1.4);
  --radius-2xl: calc(0.625rem * 1.8);
  --radius-3xl: calc(0.625rem * 2.2);
  --radius-4xl: calc(0.625rem * 2.6);

  --animate-shine: shine var(--duration) infinite linear;
  --animate-shiny-text: shiny-text 8s infinite;
}

@keyframes shine {
  0% { background-position: 0% 0%; }
  50% { background-position: 100% 100%; }
  to  { background-position: 0% 0%; }
}

@keyframes shiny-text {
  0%, 90%, 100% { background-position: calc(-100% - var(--shiny-width)) 0; }
  30%, 60%      { background-position: calc(100% + var(--shiny-width)) 0; }
}
```

### 2.3 Color theme — `:root` (light) and `.dark`

Place in `apps/desktop/src/styles/color-theme.css` and import from `app.css`.

```css
:root {
  --background: oklch(1 0 0);
  --foreground: oklch(0.145 0 0);
  --card: oklch(1 0 0);
  --card-foreground: oklch(0.145 0 0);
  --popover: oklch(1 0 0);
  --popover-foreground: oklch(0.145 0 0);
  --primary: oklch(0.205 0 0);
  --primary-foreground: oklch(0.985 0 0);
  --secondary: oklch(0.97 0 0);
  --secondary-foreground: oklch(0.205 0 0);
  --muted: oklch(0.97 0 0);
  --muted-foreground: oklch(0.556 0 0);
  --accent: oklch(0.97 0 0);
  --accent-foreground: oklch(0.205 0 0);
  --destructive: oklch(0.577 0.245 27.325);
  --border: oklch(0.922 0 0);
  --input: oklch(0.922 0 0);
  --ring: oklch(0.708 0 0);
  --sidebar: oklch(0.985 0 0);
  --sidebar-foreground: oklch(0.145 0 0);
  --sidebar-primary: oklch(0.205 0 0);
  --sidebar-accent: oklch(0.97 0 0);
  --chart-1: oklch(0.646 0.222 41.116);
  --chart-2: oklch(0.6 0.118 184.704);
  --chart-3: oklch(0.398 0.07 227.392);
  --chart-4: oklch(0.828 0.189 84.429);
  --chart-5: oklch(0.769 0.188 70.08);

  /* Workspace / PR semantic accents (same in light + dark) */
  --workspace-pr-merged-accent: #8957e5;
  --workspace-pr-open-accent: #238636;
  --workspace-pr-conflicts-accent: rgb(210, 153, 34);
  --workspace-pr-closed-accent: #da3633;

  --workspace-sidebar-status-done:     color-mix(in srgb, #8957e5 72%, white);
  --workspace-sidebar-status-review:   #a09040;
  --workspace-sidebar-status-progress: #508a5a;
  --workspace-sidebar-status-backlog:  #848f92;
  --workspace-sidebar-status-canceled: #a86868;

  --plan: #48968c;

  --radius: 0.625rem;
}

.dark {
  --background: oklch(0.165 0 0);
  --foreground: oklch(0.985 0 0);
  --card: oklch(0.205 0 0);
  --card-foreground: oklch(0.985 0 0);
  --popover: oklch(0.205 0 0);
  --popover-foreground: oklch(0.985 0 0);
  --primary: oklch(0.922 0 0);
  --primary-foreground: oklch(0.205 0 0);
  --secondary: oklch(0.269 0 0);
  --secondary-foreground: oklch(0.985 0 0);
  --muted: oklch(0.269 0 0);
  --muted-foreground: oklch(0.708 0 0);
  --accent: oklch(0.269 0 0);
  --accent-foreground: oklch(0.985 0 0);
  --destructive: oklch(0.704 0.191 22.216);
  --border: oklch(1 0 0 / 10%);
  --input: oklch(1 0 0 / 15%);
  --ring: oklch(0.556 0 0);
  --sidebar: oklch(0.205 0 0);
  --sidebar-foreground: oklch(0.985 0 0);
  --sidebar-primary: oklch(0.488 0.243 264.376);
  --sidebar-accent: oklch(0.269 0 0);
  --chart-1: oklch(0.488 0.243 264.376);
  --chart-2: oklch(0.696 0.17 162.48);
  --chart-3: oklch(0.769 0.188 70.08);
  --chart-4: oklch(0.627 0.265 303.9);
  --chart-5: oklch(0.645 0.246 16.439);

  --workspace-sidebar-status-done:     color-mix(in srgb, #8957e5 82%, white);
  --workspace-sidebar-status-review:   #f0ca52;
  --workspace-sidebar-status-progress: #7fe08f;
  --workspace-sidebar-status-backlog:  #b6b2ae;
  --workspace-sidebar-status-canceled: #ee9b9b;
}
```

### 2.4 Base layer + scrollbars

```css
@layer base {
  * { border-color: var(--border); }
  html { font-family: var(--font-sans); font-synthesis: none; text-rendering: optimizeLegibility; -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; }
  body { background: var(--background); color: var(--foreground); }
}

@layer utilities {
  * {
    scrollbar-width: thin;
    scrollbar-color: color-mix(in oklch, var(--foreground) 18%, transparent) transparent;
  }
  *::-webkit-scrollbar { width: 8px; height: 8px; }
  *::-webkit-scrollbar-thumb {
    border: 2px solid transparent;
    border-radius: 999px;
    background: color-mix(in oklch, var(--foreground) 18%, transparent);
    background-clip: padding-box;
  }
  *::-webkit-scrollbar-thumb:hover {
    background: color-mix(in oklch, var(--foreground) 30%, transparent);
  }

  .no-scrollbar::-webkit-scrollbar { display: none; }
  .no-scrollbar { scrollbar-width: none; }

  @media (prefers-reduced-motion: reduce) {
    *, *::before, *::after {
      animation-duration: 0.01ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.01ms !important;
      scroll-behavior: auto !important;
    }
  }
}
```

### 2.5 Markdown scaling for assistant text

```css
.assistant-markdown-scale p { line-height: 1.82; }
.assistant-markdown-scale h1 { font-size: 1.35em; line-height: 1.3; }
.assistant-markdown-scale h2 { font-size: 1.2em;  line-height: 1.32; }
.assistant-markdown-scale h3 { font-size: 1.1em;  line-height: 1.36; }
.assistant-markdown-scale h4, .assistant-markdown-scale h5, .assistant-markdown-scale h6 {
  font-size: 1em; line-height: 1.4;
}
.assistant-markdown-scale code {
  font-size: 0.88em;
  border: 1px solid color-mix(in oklch, var(--muted) 87%, var(--muted-foreground) 13%);
}
.assistant-markdown-scale pre code { font-size: 0.86em; }
.assistant-markdown-scale blockquote { line-height: 1.45; color: var(--muted-foreground); }
```

---

## 3. UI primitives (shadcn-style, helmor variants)

Place in `apps/desktop/src/components/ui/` (already exists). Each primitive wraps the corresponding Radix component except where noted. **Use these exact base + variant strings** — they encode helmor's identity.

### 3.1 Button (`button.tsx`)

Base:
```
group/button inline-flex shrink-0 cursor-pointer items-center justify-center rounded-lg border border-transparent bg-clip-padding in-data-[slot=button-group]:bg-clip-border text-sm font-medium whitespace-nowrap transition-all outline-none select-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 active:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4
```

Variants (cva):
- `default`: `bg-primary text-primary-foreground [a]:hover:bg-primary/80`
- `outline`: `border-border bg-background hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:border-input dark:bg-input/30 dark:hover:bg-input/50`
- `secondary`: `bg-secondary text-secondary-foreground hover:bg-secondary/80`
- `ghost`: `hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:hover:bg-muted/50`
- `destructive`: `bg-destructive/10 text-destructive hover:bg-destructive/20 focus-visible:border-destructive/40 focus-visible:ring-destructive/20 dark:bg-destructive/20 dark:hover:bg-destructive/30 dark:focus-visible:ring-destructive/40`
- `link`: `text-primary underline-offset-4 hover:underline`

Sizes: `default` (h-8 px-3), `xs` (h-6 px-2 text-[12px]), `sm` (h-7 px-2.5), `lg` (h-9 px-4), `icon` (size-8), `icon-xs` (size-6), `icon-sm` (size-7), `icon-lg` (size-9).

### 3.2 Input

```
h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 py-1 text-base transition-colors outline-none file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-input/50 disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 md:text-sm dark:bg-input/30 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40
```

### 3.3 Dialog (Radix)

Overlay: `fixed inset-0 isolate z-50 bg-black/10 duration-100 supports-backdrop-filter:backdrop-blur-xs data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=closed]:animate-out data-[state=closed]:fade-out-0`

Content: `fixed top-1/2 left-1/2 z-50 grid w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 gap-4 rounded-xl bg-popover p-4 text-sm text-popover-foreground ring-1 ring-foreground/10 duration-100 outline-none sm:max-w-sm data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95`

### 3.4 Tooltip (Radix), `delayDuration={0}`

Content: `z-50 inline-flex w-fit max-w-xs origin-(--radix-tooltip-content-transform-origin) items-center gap-1.5 rounded-md bg-foreground px-3 py-1.5 text-xs text-background data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2`

### 3.5 Tabs (Radix)

List variants: `default` → `bg-muted rounded-lg p-1`, `line` → `border-b bg-transparent`.

Trigger: `relative inline-flex h-[calc(100%-1px)] flex-1 cursor-pointer items-center justify-center gap-1.5 rounded-md border border-transparent px-1.5 py-0.5 text-sm font-medium text-muted-foreground transition-all hover:text-foreground data-[state=active]:bg-background data-[state=active]:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50`

### 3.6 Toggle / ToggleGroup

Base: `group/toggle inline-flex cursor-pointer items-center justify-center gap-1 rounded-lg text-sm font-medium whitespace-nowrap transition-all outline-none hover:bg-muted hover:text-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 aria-pressed:bg-muted data-[state=on]:bg-muted`

### 3.7 Switch

Base: `peer group/switch relative inline-flex shrink-0 cursor-pointer items-center rounded-full border border-transparent transition-all outline-none data-[state=checked]:bg-primary data-[state=unchecked]:bg-input`. Sizes: `sm` (h-3.5 w-6), `default` (h-[18.4px] w-8). Thumb: `pointer-events-none block rounded-full bg-background ring-0 transition-transform`.

### 3.8 Separator (Radix)

Horizontal: `h-px w-full bg-border`. Vertical: `w-px self-stretch bg-border`.

### 3.9 ScrollArea (Radix)

Viewport: `size-full rounded-[inherit] transition-[color,box-shadow] outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`. Bar: `flex touch-none p-px transition-colors select-none data-[orientation=horizontal]:h-2.5 data-[orientation=vertical]:w-2.5`. Thumb: `relative flex-1 rounded-full bg-border`.

### 3.10 DropdownMenu (Radix)

Content: `z-50 max-h-(--radix-dropdown-menu-content-available-height) min-w-24 origin-(--radix-dropdown-menu-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-lg bg-popover p-1 text-popover-foreground shadow-md ring-1 ring-foreground/10 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2`

Item: `group/dropdown-menu-item relative flex cursor-pointer items-center rounded-md px-1.5 outline-hidden select-none focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50`. Sizes: `default` (gap-1.5 py-1 text-sm), `sm` (gap-1 py-1 text-xs).

Separator: `-mx-1 my-1 h-px bg-border`. Label: `px-1.5 py-1 text-[11px] font-medium tracking-[0.06em] uppercase text-muted-foreground`.

### 3.11 Popover (Radix)

Content: `z-50 flex w-72 origin-(--radix-popover-content-transform-origin) flex-col gap-2.5 rounded-lg bg-popover p-2.5 text-sm text-popover-foreground shadow-md ring-1 ring-foreground/10` + same `data-state` animations as DropdownMenu.

### 3.12 Checkbox

`size-4 rounded-[4px] border border-input data-[state=checked]:border-primary data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground`. Indicator: `grid place-content-center text-current [&>svg]:size-3.5`.

### 3.13 Badge (cva)

Base: `group/badge inline-flex h-5 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-4xl border border-transparent px-2 py-0.5 text-xs font-medium whitespace-nowrap transition-all focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50`. Variants: default / secondary / destructive / outline / ghost / link.

### 3.14 Avatar (Radix)

Sizes: `sm` (size-6), `default` (size-8), `lg` (size-10). Image: `aspect-square size-full rounded-[inherit] object-cover`. Fallback: `flex size-full items-center justify-center rounded-[inherit] bg-muted`.

### 3.15 Command (cmdk)

Root: `flex size-full flex-col overflow-hidden rounded-xl bg-popover p-1 text-popover-foreground`.

Input wrapper: `p-1` containing an `InputGroup` styled `h-8 rounded-lg border-input/30 bg-input/30 shadow-none` with `<SearchIcon className="size-4 shrink-0 opacity-50" />` as left addon.

List: `no-scrollbar max-h-72 scroll-py-1 overflow-x-hidden overflow-y-auto outline-none`.

Item: `group/command-item relative flex cursor-pointer items-center gap-2 rounded-sm px-2 py-1.5 text-sm data-[selected=true]:bg-muted data-[selected=true]:text-foreground`.

Group heading: `px-2 py-1.5 text-xs font-medium text-muted-foreground`. Separator: `-mx-1 h-px bg-border`.

### 3.16 InputGroup helper (composite)

Wrap inputs that need a leading icon: outer `flex h-8 items-center gap-1 rounded-lg border border-input px-2.5` containing `[data-slot=input-group-addon]` (the icon, `size-4 shrink-0 text-muted-foreground`) and the bare input (`flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground`).

---

## 4. App shell (3-column layout)

> **Goal**: clone helmor's chrome exactly. The middle pane stays our existing `SessionWorkbench` for now; we only re-skin its container in Phase 1.

### 4.1 Constants

`apps/desktop/src/shell/layout.ts` (REPLACE existing):

```ts
export const DEFAULT_SIDEBAR_WIDTH = 336;
export const MIN_SIDEBAR_WIDTH = 220;
export const MAX_SIDEBAR_WIDTH = 520;
export const DEFAULT_INSPECTOR_WIDTH = 360;
export const MIN_INSPECTOR_WIDTH = 280;
export const MAX_INSPECTOR_WIDTH = 560;
export const SIDEBAR_RESIZE_STEP = 16;
export const SIDEBAR_RESIZE_HIT_AREA = 20;

export const SIDEBAR_WIDTH_STORAGE_KEY = "dcc.workspaceSidebarWidth";
export const INSPECTOR_WIDTH_STORAGE_KEY = "dcc.workspaceInspectorWidth";
export const SIDEBAR_COLLAPSED_STORAGE_KEY = "dcc.workspaceSidebarCollapsed";
export const INSPECTOR_COLLAPSED_STORAGE_KEY = "dcc.workspaceInspectorCollapsed";

export const INSPECTOR_SECTION_HEADER_CLASS =
  "flex h-8 min-w-0 shrink-0 items-center justify-between border-b border-border/60 bg-muted/25 px-3";
export const INSPECTOR_SECTION_TITLE_CLASS =
  "text-[13px] leading-8 font-medium tracking-[-0.01em] text-muted-foreground";
export const INSPECTOR_TAB_BUTTON_CLASS =
  "relative inline-flex h-full cursor-pointer items-center justify-center gap-1.5 px-3 text-[12px] font-medium text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-0";
```

### 4.2 Shell JSX skeleton

```tsx
<main
  aria-label="Application shell"
  className="relative h-screen overflow-hidden bg-background font-sans text-foreground antialiased"
>
  <div className="relative flex h-full min-h-0 bg-background">
    {/* LEFT SIDEBAR */}
    {!sidebarCollapsed && (
      <aside
        aria-label="Workspace sidebar"
        data-dcc-sidebar-root
        className="relative flex h-full shrink-0 flex-col overflow-hidden bg-sidebar"
        style={{ width: `${sidebarWidth}px` }}
      >
        <WorkspacesSidebar /* ... */ />
      </aside>
    )}

    {!sidebarCollapsed && (
      <ResizeSeparator
        side="left"
        widthAt={sidebarWidth}
        ariaLabel="Resize sidebar"
        ariaMin={MIN_SIDEBAR_WIDTH}
        ariaMax={MAX_SIDEBAR_WIDTH}
        ariaNow={sidebarWidth}
        isActive={isSidebarResizing}
        onMouseDown={handleResizeStart("sidebar")}
        onKeyDown={handleResizeKeyDown("sidebar")}
      />
    )}

    {/* MIDDLE */}
    <section
      aria-label="Workspace panel"
      className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-background"
    >
      <div
        aria-label="Workspace panel drag region"
        data-tauri-drag-region
        className="absolute inset-x-0 top-0 z-10 h-9 bg-transparent"
      />
      <div aria-label="Workspace viewport" className="flex min-h-0 flex-1 flex-col bg-background">
        {/* SessionWorkbench / WorkspacePanel */}
      </div>
    </section>

    {/* RIGHT INSPECTOR */}
    {!inspectorCollapsed && (
      <ResizeSeparator
        side="right"
        widthAt={inspectorWidth}
        ariaLabel="Resize inspector sidebar"
        ariaMin={MIN_INSPECTOR_WIDTH}
        ariaMax={MAX_INSPECTOR_WIDTH}
        ariaNow={inspectorWidth}
        isActive={isInspectorResizing}
        onMouseDown={handleResizeStart("inspector")}
        onKeyDown={handleResizeKeyDown("inspector")}
      />
    )}
    {!inspectorCollapsed && (
      <aside
        aria-label="Inspector sidebar"
        className="relative h-full shrink-0 overflow-hidden bg-sidebar has-[[data-tabs-zoomed=true]]:overflow-visible"
        style={{ width: `${inspectorWidth}px` }}
      >
        <WorkspaceInspectorSidebar /* ... */ />
      </aside>
    )}
  </div>
</main>

<Toaster theme={resolvedTheme} position="bottom-right" visibleToasts={6} />
```

### 4.3 ResizeSeparator visual states

- **Idle**: inner span `w-px bg-border`
- **Hover** (group-hover): `w-[2px] bg-muted-foreground/75`
- **Active drag**: `w-[2px] bg-foreground/80 shadow-[0_0_12px_rgba(0,0,0,0.12)] dark:shadow-[0_0_12px_rgba(255,255,255,0.16)]`
- Outer hit area: `SIDEBAR_RESIZE_HIT_AREA` (20px) wide, `cursor-ew-resize touch-none outline-none`, `transition-[width,background-color,box-shadow]`.

### 4.4 Tauri drag region

A 9px (`h-9`) transparent strip at the top of the middle pane has `data-tauri-drag-region`. The left sidebar header has its own 9px drag strip + a `TrafficLightSpacer` (94px) to clear macOS traffic lights. The inspector usually does not need its own drag region.

### 4.5 `useShellPanels` hook

Lives in `apps/desktop/src/shell/hooks/useShellPanels.ts` (rename from current `use-panels.ts`):

- Reads / writes width and collapsed flags to `localStorage` (keys above).
- `handleResizeStart(side)` → mousedown → window mousemove (RAF-debounced) → mouseup, clamped to min/max.
- `handleResizeKeyDown(side)` → arrow keys move by `SIDEBAR_RESIZE_STEP`, Home/End jump to min/max, with proper `aria-valuenow` updates.
- Exposes `isSidebarResizing`, `isInspectorResizing` flags (used to drive separator active state).

---

## 5. Left sidebar — workspaces navigation

`apps/desktop/src/features/workspaces/` (already exists; rebuild presentation around helmor structure).

### 5.1 Outer container

```tsx
<div className="flex h-full min-h-0 flex-col overflow-hidden">
  {/* Window safe-top + drag region */}
  <div data-slot="window-safe-top" className="flex h-9 shrink-0 items-center pr-3">
    <TrafficLightSpacer side="left" width={94} />
    <div data-tauri-drag-region className="h-full flex-1" />
  </div>

  {/* Header row */}
  <div className="flex items-center justify-between px-3">
    <h2 className="text-[14px] font-medium tracking-[-0.01em] text-muted-foreground">Workspaces</h2>
    <div className="flex items-center gap-1 text-muted-foreground">
      {/* DropdownMenu: "Open project" / "Clone from URL" with FolderPlus icon */}
      {/* Popover: "+" button → repo picker with cmdk Command */}
    </div>
  </div>

  {/* Virtualized list */}
  <div ref={scrollContainerRef} className="min-h-0 flex-1 overflow-hidden">
    {virtualizer.getVirtualItems().map(...)}
  </div>

  {/* Footer */}
  <div className="flex shrink-0 items-center justify-between px-3 pb-3 pt-1">
    <SettingsButton onClick={openSettings} />
    {githubConnected && <GithubStatusMenu />}
  </div>
</div>
```

### 5.2 Group header (collapsible status section)

```tsx
<button className="group/trigger flex w-full select-none items-center justify-between rounded-lg px-2 py-1 text-[13px] font-semibold tracking-[-0.01em] text-foreground hover:bg-accent/60 cursor-pointer">
  <span className="flex items-center gap-2">
    <GroupIcon tone={group.tone} />
    <span>{group.label}</span>
  </span>
  {group.rows.length > 0 && (
    <Badge variant="secondary" className="h-4 min-w-[16px] rounded-full px-1 text-[9.5px]">
      {group.rows.length}
    </Badge>
  )}
</button>
```

Group tones map to CSS vars `--workspace-sidebar-status-{done,review,progress,backlog,canceled}`.

### 5.3 Workspace row (`row-item.tsx`)

```ts
const rowVariants = cva(
  "group/row relative flex h-7.5 select-none items-center gap-2 rounded-md px-2.5 text-[13px] cursor-pointer",
  {
    variants: {
      active: {
        true:  "workspace-row-selected text-foreground",
        false: "text-foreground/80 hover:bg-accent/60",
      },
    },
  },
);
```

CSS for selected: `.workspace-row-selected { background: var(--workspace-sidebar-selected-bg); }` — derive that var from `--accent` at 65% opacity, or copy helmor's exact value if discovered later.

Row body:
```tsx
<div className={cn(rowVariants({ active: selected }), !selected && row.state === "archived" && "opacity-50")}>
  <div className="flex min-w-0 flex-1 items-center gap-2">
    <WorkspaceAvatar repoIconSrc={...} repoInitials={...} badgeClassName={statusDotClassName} isRunning={isRunScriptRunning} />
    <div className="row-content-fade flex min-w-0 flex-1 items-center gap-2">
      {isSending ? <DccThinkingIndicator size={13} /> : <GitBranch className={cn("size-[13px] shrink-0", branchToneClasses[branchTone])} />}
      <span className="truncate leading-tight font-medium">{displayTitle}</span>
    </div>
  </div>
  <div className="group/actions flex shrink-0 items-center gap-0.5 pr-2.5">
    {/* Archive/Restore + Delete (revealed on hover via opacity) */}
  </div>
</div>
```

`.row-content-fade` uses a CSS mask:
```css
.row-content-fade { -webkit-mask-image: linear-gradient(to left, transparent var(--row-fade-transparent, 1.2rem), black var(--row-fade-solid, 2rem)); mask-image: linear-gradient(to left, transparent var(--row-fade-transparent, 1.2rem), black var(--row-fade-solid, 2rem)); }
```

### 5.4 Footer buttons

Settings + GitHub status menu — both use `Button variant="ghost" size="icon-xs"`. Tooltip with shortcut display on hover.

---

## 6. Middle panel — workspace + conversation

`apps/desktop/src/features/panel/` (NEW; promotes part of existing `features/sessions/`).

### 6.1 Container

```tsx
<div className="flex min-h-0 flex-1 flex-col bg-transparent">
  <WorkspacePanelHeader workspace={...} sessions={...} selectedSessionId={...} />
  <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
    {hasLoaded ? (
      <ActiveThreadViewport ... />
    ) : (
      <div className="flex min-h-full flex-1 items-center justify-center px-8">
        <EmptyState ... />
      </div>
    )}
  </div>
</div>
```

### 6.2 Header

```tsx
<div className="flex items-center border-b border-border/60 px-4 py-3">
  {/* session tabs (cmdk-style chips) + workspace title + new-session button */}
</div>
```

### 6.3 Thread viewport

- `relative flex min-h-0 flex-1 flex-col overflow-hidden overflow-y-auto`
- Scroll-anchor logic: keep scrolled to bottom when streaming new content unless user scrolled up.
- Animate scrollbar in: `animation: conversation-scrollbar-fade-in 300ms ease-out 400ms both;`

```css
@keyframes conversation-scrollbar-fade-in { from { opacity: 0; } to { opacity: 1; } }
@keyframes conversation-fade-in { from { opacity: 0; } to { opacity: 1; } }
```

### 6.4 Message components (`features/panel/message-components/`)

#### User message
```tsx
<div data-message-id={...} data-message-role="user" className="group/user flex min-w-0 justify-end">
  <div className="relative flex max-w-[75%] min-w-0 flex-col items-end pb-5">
    <div className="conversation-body-text w-full overflow-hidden rounded-md bg-accent/55 px-3 py-2 leading-7">
      <p className="whitespace-pre-wrap break-words">{/* parts */}</p>
    </div>
    <div className="pointer-events-none absolute right-1 bottom-0 flex items-center justify-end opacity-0 group-hover/user:pointer-events-auto group-hover/user:opacity-100 group-focus-within/user:pointer-events-auto group-focus-within/user:opacity-100">
      <CopyMessageButton className="size-5 shrink-0 text-muted-foreground/28 hover:text-muted-foreground" />
    </div>
  </div>
</div>
```

#### Assistant message
```tsx
<div className="conversation-markdown assistant-markdown-scale max-w-none break-words text-foreground" style={{ fontSize: `${settings.fontSize}px` }}>
  <Suspense fallback={<AssistantTextFallback text={text} />}>
    <LazyStreamdown animated={streaming ? STREAMING_ANIMATED : false} caret={undefined} className="conversation-streamdown" isAnimating={streaming} mode={mode}>
      {text}
    </LazyStreamdown>
  </Suspense>
</div>
```

`STREAMING_ANIMATED = { type: "blurIn", duration: 150, wordStagger: 30 }` (helmor default).

Message status badge (max_tokens etc.):
```tsx
<div className={cn("mt-1 inline-flex w-fit items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-medium", meta.tone)}>
  {meta.icon}<span>{meta.label}</span>
</div>
```

#### Tool call (`tool-call.tsx`)
```tsx
<details className="group/out flex flex-col" open={isOpen} onToggle={(e) => setIsOpen(e.currentTarget.open)}>
  <summary className={cn("flex max-w-full items-center gap-1.5 py-0.5 text-[12px] text-muted-foreground [&::-webkit-details-marker]:hidden", canExpand ? "cursor-pointer" : "cursor-default")}>
    <span className="shrink-0">{info.icon}</span>
    <span className="shrink-0 whitespace-nowrap font-medium">{info.action}</span>
    {info.file && /* file pill or EditDiffTrigger */}
    {info.command && <code className="inline-block min-w-0 truncate rounded bg-accent/60 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">{info.command}</code>}
    {statusIndicator /* LoaderCircle for live, AlertCircle for error */}
  </summary>
  {/* expanded body: code blocks, file lists, diff */}
</details>
```

#### Reasoning collapsible (`components/ai/reasoning.tsx`)
```tsx
<CollapsibleTrigger className="group/reasoning inline-flex max-w-full cursor-pointer items-center gap-1.5 py-0.5 text-[12px] text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden">
  <BrainIcon className="size-3 shrink-0" />
  {children ?? getThinkingMessage(isStreaming, duration)}
  <ChevronRightIcon className={cn("size-3 shrink-0", isOpen ? "rotate-90" : "rotate-0")} />
</CollapsibleTrigger>
```

#### Code block (`components/ai/code-block.tsx`)
- Shiki dual theme (light + dark blocks rendered side by side, one hidden via `dark:hidden` / `hidden dark:block`).
- Wrapper: `group relative my-4 w-full min-w-0 max-w-full overflow-hidden rounded-lg border border-border/70 bg-background/80 shadow-[inset_0_1px_0_rgba(255,255,255,0.02)]`.
- Header: language label `truncate font-mono text-[10px] leading-none tracking-wide text-muted-foreground/50 uppercase select-none` + actions that fade in `opacity-0 transition-opacity group-hover:opacity-100`.

### 6.5 Streamdown integration

- `components/streamdown-loader.tsx` (lazy import; preload on app idle):
```tsx
const LazyStreamdown = lazy(async () => {
  const [{ Streamdown }, { streamdownComponents }] = await Promise.all([
    import("streamdown"),
    import("@/components/streamdown-components"),
  ]);
  return { default: (props) => <Streamdown {...props} components={{ ...streamdownComponents, ...props.components }} /> };
});
```
- `components/streamdown-components.tsx` overrides: `table`, `pre` (→ `CodeBlock`), `a` (→ `StreamdownAnchor` that opens via Tauri or `openInEditor` for local file links).

---

## 7. Composer

`apps/desktop/src/features/composer/` (NEW). All file paths use the t3code-style flat-with-subfolders organization (see §15).

### 7.1 File layout

```
features/composer/
├── WorkspaceComposer.tsx          ← main component
├── WorkspaceComposer.logic.ts     ← pure helpers (mode label, send-disabled rules, draft key)
├── WorkspaceComposer.logic.test.ts
├── ComposerButton.tsx             ← ghost button used in toolbar
├── ContextBar.tsx                 ← horizontal chip strip for /add-dir
├── ContextBar.logic.ts
├── ContextUsageRing.tsx
├── UsageStatsIndicator.tsx
├── FastModeLottieIcon.tsx
├── draftStorage.ts                ← localStorage key + serializer
├── editorOps.ts                   ← extract text/files/images/tags from Lexical state
├── editor/
│   ├── FileBadgeNode.tsx          ← Lexical DecoratorNode
│   ├── ImageBadgeNode.tsx
│   ├── CustomTagBadgeNode.tsx
│   ├── addDir/
│   │   ├── TriggerNode.tsx
│   │   └── TypeaheadPlugin.tsx
│   └── plugins/
│       ├── SlashCommandPlugin.tsx
│       ├── FileMentionPlugin.tsx
│       ├── AutoResizePlugin.tsx
│       ├── SubmitPlugin.tsx
│       ├── DraftPersistencePlugin.tsx
│       ├── DropFilePlugin.tsx
│       ├── PasteImagePlugin.tsx
│       ├── CompositionGuardPlugin.tsx
│       └── HasContentPlugin.tsx
└── panels/
    ├── ElicitationPanel.tsx
    ├── DeferredToolPanel.tsx
    └── SubmitQueueList.tsx
```

### 7.2 Outer wrapper

```tsx
<div
  ref={composerRootRef}
  aria-label="Workspace composer"
  data-focus-scope="composer"
  className={cn(
    "relative flex flex-col rounded-2xl border border-border/40 bg-sidebar shadow-[0_-1px_8px_rgba(0,0,0,0.05),0_0_0_1px_rgba(255,255,255,0.02)]",
    hasPendingInteraction ? "p-0" : "px-4 pb-3 pt-3",
    inputDisabled && !hasPendingInteraction && "cursor-not-allowed opacity-60",
  )}
>
  {/* ... */}
</div>
```

### 7.3 Editor surface

```tsx
<LexicalComposer initialConfig={initialConfig}>
  <div className="relative">
    <PlainTextPlugin
      contentEditable={
        <ContentEditable
          id="workspace-input"
          className={cn(
            "composer-editor min-h-[64px] max-h-[240px] resize-none overflow-x-hidden overflow-y-auto whitespace-pre-wrap break-words bg-transparent text-[14px] leading-5 tracking-[-0.01em] text-foreground outline-none",
            showFocusHint && "pr-28",
          )}
        />
      }
      placeholder={
        <div className="pointer-events-none absolute left-0 top-0 text-[14px] leading-5 tracking-[-0.01em] text-muted-foreground/70">
          {planActive ? "Describe what to change, then click Request Changes" : "Ask to make changes, @mention files, run /commands"}
        </div>
      }
      ErrorBoundary={LexicalErrorBoundary}
    />
    {/* Focus hint top-right (⌘K to focus) */}
  </div>

  <HistoryPlugin />
  <SlashCommandPlugin commands={...} popupAnchorRef={composerRootRef} />
  <AddDirTypeaheadPlugin />
  <FileMentionPlugin />
  <SubmitPlugin onSubmit={handleSubmit} />
  <CompositionGuardPlugin />
  <PasteImagePlugin />
  <DropFilePlugin />
  <AutoResizePlugin minHeight={64} maxHeight={240} />
  <EditorRefPlugin editorRef={editorRef} />
  <DraftPersistencePlugin />
  <EditablePlugin disabled={inputDisabled} />
  <HasContentPlugin onChange={setHasContent} />
</LexicalComposer>
```

### 7.4 Footer (toolbar + send)

```tsx
<div className="mt-2.5 flex items-end justify-between gap-3">
  <div className="flex flex-wrap items-center gap-2">
    {/* Model selector dropdown */}
    {/* Fast mode toggle */}
    {/* Effort selector */}
    {/* Plan toggle */}
  </div>

  <div className="flex items-center gap-1">
    <UsageStatsIndicator agentType={agentType} disabled={disabled} />
    {sessionId && supportsContextUsage && <ContextUsageRing /* ... */ />}
    {/* Send / Stop / Steer cluster */}
  </div>
</div>
```

Toolbar trigger className (use `cn(composerToolbarTriggerClassName, ...)` per item):
```ts
export const composerToolbarTriggerClassName =
  "cursor-pointer rounded-[9px] px-1 py-0.5 text-[13px] font-medium transition-colors hover:bg-accent/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/50";
```

Disabled toolbar variant: `cursor-not-allowed opacity-45 hover:bg-transparent hover:text-muted-foreground`.

Send button: `<Button variant="outline" size="icon" className="ml-1.5 rounded-[9px]"><ArrowUp className="size-[15px]" /></Button>`.
Stop button: `<Button variant="destructive" size="icon"><Square className="size-3 fill-current" /></Button>`.
Steer button: `<Button variant="outline" size="icon" disabled={steerDisabled}><ArrowUp className="size-[15px]" /></Button>`.

### 7.5 Inline badges (`components/inline-badge/`)

```tsx
<span
  className={cn(
    "mx-0.5 inline-flex items-baseline rounded-sm border border-border/60 text-[14px] leading-none transition-colors hover:border-muted-foreground/40 hover:bg-accent/40",
    nonSelectable && "cursor-default select-none",
    canPreview && "cursor-pointer",
    className,
  )}
>
  <span className={cn("inline-flex min-w-0 items-baseline gap-1.5 py-[3px] pl-2", onRemove ? "pr-1" : "pr-2")}>
    <span className="inline-flex self-center">{icon}</span>
    <span className={cn("max-w-[200px] truncate text-muted-foreground", labelClassName)}>{label}</span>
  </span>
  {onRemove && (
    <button type="button" className="mr-1 inline-flex size-4 shrink-0 cursor-pointer items-center justify-center self-center rounded-sm text-muted-foreground/40 transition-colors hover:text-muted-foreground" onClick={(e) => { e.preventDefault(); e.stopPropagation(); onRemove(); }}>
      <X className="size-3" strokeWidth={1.8} />
    </button>
  )}
</span>
```

### 7.6 Context bar

```tsx
<div data-slot="context-bar" className="relative -mx-4 mb-2">
  <div className="flex items-center border-b border-dashed border-border/55 px-4 pb-2 pt-0.5">
    <span className="shrink-0 pr-2 font-mono text-[10.5px] uppercase tracking-[0.06em] text-muted-foreground">context</span>
    <div ref={scrollRef} className="relative min-w-0 flex-1" data-overflow={hasOverflow ? "true" : "false"}>
      <div aria-hidden className={cn("pointer-events-none absolute inset-y-0 right-0 w-12 bg-gradient-to-r from-transparent to-sidebar transition-opacity duration-200", hasOverflow ? "opacity-100" : "opacity-0")} />
      <div ref={barRef} role="list" className="scrollbar-none flex items-center gap-1 overflow-x-auto">
        {directories.map((d, idx) => <Chip key={d.path} directory={d} showSeparator={idx > 0} disabled={disabled} onRemove={() => handleRemove(d.path)} />)}
      </div>
    </div>
  </div>
</div>
```

Chip className: `group/chip inline-flex shrink-0 items-center gap-1.5 rounded-md px-1.5 py-0.5 text-[12px] leading-tight outline-none transition-colors hover:bg-accent/60 focus-visible:bg-accent/60 focus-visible:shadow-[0_0_0_2px_color-mix(in_srgb,var(--workspace-pr-merged-accent)_35%,transparent)]`. Close button hidden until `group-hover/chip` or `group-focus-visible/chip`.

---

## 8. Right inspector — three-tier panel

`apps/desktop/src/features/inspector/` (already exists; rebuild around three sections).

### 8.1 Container

```tsx
<div className="flex h-full min-h-0 flex-col bg-sidebar select-none">
  <ChangesSection bodyHeight={changesHeight} />
  <HorizontalResizeHandle onMouseDown={handleResizeStart("actions")} />
  <ActionsSection bodyHeight={actionsHeight} />
  {tabsOpen && <HorizontalResizeHandle onMouseDown={handleResizeStart("tabs")} />}
  <InspectorTabsSection
    open={tabsOpen}
    activeTab={activeTab}
    setupScriptState={setupScriptState}
    runScriptState={runScriptState}
    terminalInstances={terminalInstances}
  >
    <SetupTab /> <RunTab /> {terminalInstances.map(t => <TerminalInstancePanel key={t.id} instance={t} />)}
  </InspectorTabsSection>
</div>
```

### 8.2 Section header

Use `INSPECTOR_SECTION_HEADER_CLASS` from §4.1. Title uses `INSPECTOR_SECTION_TITLE_CLASS`.

### 8.3 Tabs header (Setup / Run / Terminal N / +)

```tsx
<div className={INSPECTOR_SECTION_HEADER_CLASS}>
  <div role="tablist" className="flex h-full min-w-0 flex-1 gap-0 overflow-x-auto">
    {tabs.map(tab => (
      <button
        key={tab.id}
        role="tab"
        aria-selected={activeTab === tab.id}
        className={cn(INSPECTOR_TAB_BUTTON_CLASS, activeTab === tab.id && "text-foreground")}
        onClick={() => onSelectTab(tab.id)}
      >
        <ScriptStatusIcon state={tab.state} />
        {tab.label}
        <span className={cn("absolute inset-x-0 bottom-0 h-0.5 bg-foreground transition-opacity", activeTab === tab.id ? "opacity-100" : "opacity-0")} />
      </button>
    ))}
  </div>
  <div className="ml-2 flex shrink-0 items-center gap-1">
    {tabActions}
    <Button variant="ghost" size="icon-sm" onClick={onToggle}>
      <ChevronDown style={{ transform: open ? "rotate(0deg)" : "rotate(-90deg)" }} />
    </Button>
  </div>
</div>
```

### 8.4 Tabs body + hover-zoom

```tsx
{open && (
  <div aria-label="Inspector tabs body" onMouseEnter={handleBodyMouseEnter} className="relative flex min-h-0 flex-1 flex-col bg-sidebar">
    <TabsZoomContext.Provider value={{ isZoomPresented, isHoverExpanded }}>
      {children}
    </TabsZoomContext.Provider>
  </div>
)}
```

Zoom rules:
- Hover-intent: 300ms timer.
- Target: `scale(2)`, `transform-origin: top right` (so the panel grows toward the center pane).
- Easing: `cubic-bezier(0.32, 0.72, 0, 1)`, duration 400ms.
- During the transition: `box-shadow: 0 30px 60px -20px rgba(0,0,0,0.35)`, `z-index: 50`, sets `data-tabs-zoomed="true"` on the inspector aside (the aside has `has-[[data-tabs-zoomed=true]]:overflow-visible` so the zoomed panel can spill).
- Suspend xterm `FitAddon` while transitioning; resume after settle.

### 8.5 Git section header (top of Changes)

```tsx
<div ref={headerRef} className={cn(INSPECTOR_SECTION_HEADER_CLASS, "relative gap-1.5 overflow-hidden border-b-0 shadow-[inset_0_-1px_0_color-mix(in_oklch,var(--border)_60%,transparent)] transition-[background-color,border-color,color,box-shadow] duration-300 ease-out", gitHeaderHighlightClass)}>
  {showShimmer && <div className="pointer-events-none absolute inset-x-0 bottom-0 h-px motion-safe:animate-[shine_2s_infinite_linear]" />}

  <div className="flex shrink-0 items-center gap-1.5">
    {!showChangeRequest ? (
      <span className={cn(INSPECTOR_SECTION_TITLE_CLASS, "translate-y-px")}>Git</span>
    ) : (
      <Button variant="outline" size="xs" className={cn("self-center bg-transparent font-normal tracking-[0.01em] hover:bg-transparent hover:opacity-80", prModeBorderColorClass)} onClick={onChangeRequestClick}>
        <span className="inline-flex h-4 min-w-0 items-center gap-1.5 leading-4">
          <span className="inline-flex size-4 shrink-0 items-center justify-center overflow-visible">{isMergeRequest ? <GitlabBrandIcon size={12} /> : <GithubBrandIcon size={12} />}</span>
          <span className="inline-flex h-4 min-w-0 items-center truncate leading-4 tabular-nums text-[13px] font-light">{isMergeRequest ? "!" : "#"}{changeRequest.number}</span>
        </span>
      </Button>
    )}
  </div>

  <div className="flex items-center gap-1">
    {showForgeOnboarding ? <ForgeCliTrigger workspaceId={workspaceId} /> : <WorkspaceCommitButton mode={commitButtonMode} state={commitButtonState} onCommit={onCommit} />}
  </div>
</div>
```

PR-mode highlight backgrounds (apply on the header itself):
```ts
function gitSectionHeaderHighlightClass(mode) {
  switch (mode) {
    case "fix":               return "bg-[var(--workspace-pr-closed-header-bg)]";
    case "resolve-conflicts": return "bg-[var(--workspace-pr-conflicts-header-bg)]";
    case "merge":             return "bg-[var(--workspace-pr-open-header-bg)]";
    case "merged":            return "bg-[var(--workspace-pr-merged-header-bg)]";
    case "closed":            return "bg-[var(--workspace-pr-closed-header-bg)]";
    default:                  return null;
  }
}
```

(Define `*-header-bg` CSS vars as `color-mix(in oklch, var(--workspace-pr-{mode}-accent) 12%, var(--sidebar))` — adjust to taste in Phase 3.)

### 8.6 Commit button (`features/commit/WorkspaceCommitButton.tsx`)

State machine: `idle | busy | done | error | disabled`. Modes: `create-pr | open-pr | commit-and-push | push | fix | resolve-conflicts | merge | merged | closed`. Label table:

| mode | idle | busy | done | error |
|---|---|---|---|---|
| create-pr | "Create PR" | "Creating PR..." | "PR Created" | "Retry" |
| commit-and-push | "Commit & Push" | "Committing..." | "Pushed" | "Retry" |
| push | "Push" | "Pushing..." | "Pushed" | "Retry" |
| fix | "Fix CI" | "Fixing..." | "Fixed" | "Retry" |
| resolve-conflicts | "Resolve" | "Resolving..." | "Resolved" | "Retry" |
| merge | "Merge" | "Merging..." | "Merged" | "Retry" |
| merged | "Merged" (always disabled, ghost variant) | — | — | — |
| closed | "Closed" (always disabled, ghost variant) | — | — | — |

Mode → variant + override class:
```ts
function modeClassName(mode) {
  switch (mode) {
    case "fix":
    case "closed":            return "bg-[var(--workspace-pr-closed-accent)] text-white hover:bg-[var(--workspace-pr-closed-accent)]";
    case "resolve-conflicts": return "bg-[var(--workspace-pr-conflicts-accent)] text-white hover:bg-[var(--workspace-pr-conflicts-accent)]";
    case "merge":             return "bg-[var(--workspace-pr-open-accent)] text-white hover:bg-[var(--workspace-pr-open-accent)]";
    case "merged":            return "bg-[var(--workspace-pr-merged-accent)] text-white hover:bg-[var(--workspace-pr-merged-accent)]";
    default:                  return undefined;
  }
}
```

Uncontrolled timing: `done` flashes for 900ms then returns to `idle`; `error` for 1200ms.

---

## 9. Command palette

The palette is a `Dialog` placed at `top-1/3 -translate-y-0` containing the `Command` cmdk root.

```tsx
<Dialog open={open} onOpenChange={onOpenChange}>
  <DialogHeader className="sr-only"><DialogTitle>Command Palette</DialogTitle><DialogDescription>Search workspaces and actions</DialogDescription></DialogHeader>
  <DialogContent className="top-1/3 translate-y-0 overflow-hidden rounded-xl p-0" showCloseButton={false}>
    <Command>
      <CommandInput placeholder="Type a command or search…" />
      <CommandList>
        <CommandEmpty>No results.</CommandEmpty>
        <CommandGroup heading="Workspaces">
          {workspaces.map(w => <CommandItem key={w.id} onSelect={() => onSelectWorkspace(w.id)} className="rounded-lg">{w.name}</CommandItem>)}
        </CommandGroup>
        <CommandSeparator />
        <CommandGroup heading="Actions">{/* … */}</CommandGroup>
      </CommandList>
    </Command>
  </DialogContent>
</Dialog>
```

Open shortcut: `⌘K` / `Ctrl+K`. Already wired in current DCC `App.tsx`.

---

## 10. Settings dialog

`apps/desktop/src/features/settings/SettingsDialog.tsx` (NEW). Sidebar + content pane.

Sections (fixed order): General · Appearance · Model · Shortcuts · Git · Experimental · (Import — if Conductor available) · (Developer — if dev build) · Account. Plus per-repository sections at the bottom of the sidebar.

```tsx
<Dialog open={open} onOpenChange={onClose}>
  <DialogContent className="h-[min(80vh,640px)] w-[min(80vw,860px)] max-w-[860px] overflow-hidden rounded-2xl border-border/60 bg-background p-0 shadow-2xl">
    <div className="flex h-full min-h-0 w-full min-w-0 gap-0 overflow-hidden">
      <nav className="scrollbar-stable flex w-[200px] shrink-0 flex-col overflow-x-hidden overflow-y-auto border-r border-sidebar-border bg-sidebar py-6">
        {/* Section list + repository list */}
      </nav>
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <div className="flex items-center border-b border-border/40 px-8 py-4">
          <DialogTitle className="text-[15px] font-semibold text-foreground">{titleFor(activeSection)}</DialogTitle>
        </div>
        <div className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto px-8 pt-1 pb-6">
          {renderPanel(activeSection)}
        </div>
      </div>
    </div>
  </DialogContent>
</Dialog>
```

Appearance theme toggle:
```tsx
<ToggleGroup type="single" value={settings.theme} onValueChange={updateTheme}>
  {[
    { value: "system", icon: Monitor, label: "System" },
    { value: "light",  icon: Sun,     label: "Light"  },
    { value: "dark",   icon: Moon,    label: "Dark"   },
  ].map(({ value, icon: Icon, label }) => (
    <ToggleGroupItem key={value} value={value} className="gap-1.5 rounded-lg px-3 py-1.5 text-[12px] font-medium text-muted-foreground data-[state=on]:bg-accent data-[state=on]:text-foreground">
      <Icon className="size-3.5" strokeWidth={1.8} /> {label}
    </ToggleGroupItem>
  ))}
</ToggleGroup>
```

Settings row helper (`features/settings/components/SettingsRow.tsx`): label + description on the left, control on the right; `flex items-start justify-between gap-6 py-4 border-b border-border/40 last:border-0`.

---

## 11. Onboarding

`apps/desktop/src/features/onboarding/` (NEW). Renders a full-screen wizard with absolute-positioned steps and a viewport-scaled mockup behind.

Steps (state machine): `intro → agents → corner (CLI) → skills → repoImport → completeTransition`. Optional `conductor` branch replaces `repoImport` if Conductor is available.

Key visual rules:
- Drag region top bar: `absolute inset-x-0 top-0 z-20 flex h-11 items-center` with `TrafficLightSpacer` left=94, right=140, and `data-tauri-drag-region` on the middle div.
- Grid background: `pointer-events-none absolute inset-0 opacity-[0.08]` with linear-gradient lines + radial mask.
- Step transitions: each step is `absolute inset-x-0 top-[calc(50vh-40px)] z-20 flex origin-top flex-col items-center px-8 pb-12 pt-8 transition-transform duration-1000 ease-[cubic-bezier(.22,.82,.2,1)]` with translate/scale/opacity per state.
- Mockup viewport: 1300×900 logical, scaled via `ResizeObserver`. Build minimal mock components in `features/onboarding/mockup/` (mirroring the real shell at simplified fidelity).

For Phase 4 we ship `intro → agents → repoImport → completeTransition` first, mockup behind. CLI / Skills / Conductor steps come later.

---

## 12. Auxiliary surfaces

### 12.1 Splash screen (`components/SplashScreen.tsx`)
```tsx
<div aria-hidden="true" className="fixed inset-0 z-[9999] flex items-center justify-center bg-background transition-opacity duration-400" style={{ opacity: visible ? 1 : 0 }}>
  <DccLogoAnimated size={64} className="opacity-80" />
</div>
```

### 12.2 Dock badge (`features/dock-badge/`)
- `selector.ts`: `selectUnreadSessionCount(groups)` sums `unreadSessionCount` across rows.
- `useDockUnreadBadge.ts`: `getCurrentWindow().setBadgeCount?.(count > 0 ? count : undefined)` with try/catch fallback to `setBadgeLabel`.
- Mount at app root, returns nothing.

### 12.3 Updater button (`features/updater/AppUpdateButton.tsx`)

Render only when `status?.stage === "downloaded"`.
```tsx
<Tooltip>
  <TooltipTrigger asChild>
    <Button variant="ghost" size="xs" className="h-6 gap-1 rounded-sm px-1.5 text-[11px] font-medium tracking-[0.01em] text-muted-foreground hover:bg-accent/60 hover:text-foreground dark:hover:bg-muted/45 relative overflow-hidden shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--border)_36%,transparent)] hover:shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--foreground)_12%,transparent)]" disabled={installing} onClick={installNow}>
      {installing ? <Loader2 className="size-3 animate-spin text-foreground/70" /> : <Download className="size-3 text-foreground/72" />}
      <span>Update</span>
    </Button>
  </TooltipTrigger>
  <TooltipContent side="top" sideOffset={4} className="flex h-[22px] items-center gap-1 rounded-md px-1.5 text-[11px] leading-none">
    {update.currentVersion} → {update.version}
  </TooltipContent>
</Tooltip>
```

### 12.4 Conductor onboarding

Optional, deferred. Multi-beam SVG animation between Conductor logo and workspace items. Full structure documented in source helmor file (`src/components/conductor-onboarding.tsx`); we treat this as a Phase 4 nice-to-have.

### 12.5 Shortcut display (`features/shortcuts/`)

Inline (`InlineShortcutDisplay`): `inline-flex items-center gap-px font-medium leading-none tracking-normal text-current` + each key chip `h-5 min-w-5 rounded-[4px] border-border/70 bg-background px-1.5 text-[11px] text-muted-foreground shadow-[inset_0_-1px_0_rgba(0,0,0,0.08)]`.

Used inside tooltip contents whenever an action has a binding.

### 12.6 Helmor thinking indicator (rename `DccThinkingIndicator`)

```tsx
<span aria-hidden="true" data-slot="dcc-thinking-indicator" className={cn("inline-flex shrink-0 items-center justify-center", className)} style={{ width: size, height: size }}>
  <DccLogoAnimated size={size} className="shrink-0 opacity-80" />
</span>
```

Use a lightweight Lottie or motion-based loop for the animated logo (deferred — first version can be a `Loader2` `animate-spin` placeholder).

---

## 13. Sonner toaster

Mount once outside the shell:
```tsx
<Toaster theme={resolvedTheme} position="bottom-right" visibleToasts={6} />
```

Use `toast.error("Title", { description, action: { label, onClick } })` for failures (e.g., updater install error with "Change log" action).

---

## 14. Iconography

- Primary: `lucide-react`. Default size `size-4` for inline, `size-3` for inline-small chrome (badges, tool calls), `size-5` for primary actions.
- Brand icons (GitHub, GitLab): `@primer/octicons-react` is fine, or hand-written SVG. Keep size 12 inside small badges, 14 inline.
- AI provider icons (Claude, OpenAI, Gemini): `@lobehub/icons` or curated SVG copies. Used inside `ModelIcon`.

---

## 15. Organization rules — t3code style, applied to `apps/desktop/src/`

We do **not** create `apps/web`. We apply t3code's *naming and split* conventions inside the existing `apps/desktop/src/`.

### 15.1 Top-level files allowed at `apps/desktop/src/`

Only cross-cutting things, not features. Examples:
- `main.tsx` — bootstrap
- `App.tsx` — shell composition
- Stores: `commandPaletteStore.ts`, `composerDraftStore.ts`, `inspectorLayoutStore.ts` (zustand `useXxxStore` exported)
- Bootstrap: `authBootstrap.ts`, `historyBootstrap.ts`
- API thin wrappers: `localApi.ts`, `providerApi.ts` (if not already covered by `lib/`)
- Feature-agnostic logic: `composer-logic.ts`, `session-logic.ts` (NB: feature-specific logic stays inside the feature folder)
- Routes / keybindings constants: `keybindings.ts`, `branding.ts`

### 15.2 Directories

- `apps/desktop/src/components/ui/` — shadcn-style primitives (§3). Only generic, no business logic.
- `apps/desktop/src/components/` (top-level) — page-level / layout-level cross-feature components (e.g. `BranchToolbar.tsx`, `CommandPalette.tsx`, `SplashScreen.tsx`, `DccThinkingIndicator.tsx`). Each follows the logic-split pattern below.
- `apps/desktop/src/components/<feature>/` — when a UI cluster grows, create a subfolder (`components/chat/`, `components/settings/`).
- `apps/desktop/src/features/<feature>/` — runtime-heavy features that own state + logic + UI together (workspaces, sessions, inspector, composer, panel, onboarding, updater, dock-badge, commit, shortcuts, navigation, settings).
- `apps/desktop/src/hooks/` — generic hooks (`useLocalStorage.ts`, `useMediaQuery.ts`, `useTheme.ts`).
- `apps/desktop/src/lib/` — domain helpers without React (`gitReactQuery.ts`, `terminalContext.ts`, `storage.ts`, `lruCache.ts`).
- `apps/desktop/src/shell/` — chrome / layout primitives (constants, resize hook, zoom hook, github gate).
- `apps/desktop/src/styles/` — `app.css`, `color-theme.css`, any `.css` not co-located with a single component.

### 15.3 Naming

| Kind | Casing | Example |
|---|---|---|
| React component | PascalCase `.tsx` | `WorkspaceComposer.tsx`, `BranchToolbar.tsx` |
| Component logic | PascalCase `.logic.ts` | `WorkspaceComposer.logic.ts` |
| Logic test | `.logic.test.ts` | `WorkspaceComposer.logic.test.ts` |
| Browser variant | `.browser.tsx` | only when needed (rarely; Tauri vs pure-browser fork) |
| Store | camelCase `Store.ts` | `composerDraftStore.ts` |
| Hook | `useXxx.ts` | `useShellPanels.ts`, `useDockUnreadBadge.ts` |
| Utility | camelCase `.ts` | `editorOps.ts`, `formatBranchName.ts` |
| CSS | kebab-case `.css` | `color-theme.css` |
| Tests | colocated `.test.ts(x)` | `editorOps.test.ts` |

### 15.4 The logic/view split (mandatory for components > ~150 lines)

`Component.tsx` contains JSX, hooks, refs, side effects. `Component.logic.ts` exports **pure** functions and types — no React, no DOM, no zustand. `Component.logic.test.ts` tests the logic file directly with vitest. Example:

```ts
// WorkspaceComposer.logic.ts
export type ComposerSendDecision =
  | { kind: "send" }
  | { kind: "steer" }
  | { kind: "blocked"; reason: "empty" | "disabled" | "queued" };

export function decideSend({ hasContent, sending, disabled }: {
  hasContent: boolean; sending: boolean; disabled: boolean;
}): ComposerSendDecision {
  if (disabled) return { kind: "blocked", reason: "disabled" };
  if (!hasContent) return { kind: "blocked", reason: "empty" };
  return sending ? { kind: "steer" } : { kind: "send" };
}
```

```ts
// WorkspaceComposer.logic.test.ts
import { describe, it, expect } from "vitest";
import { decideSend } from "./WorkspaceComposer.logic";

describe("decideSend", () => {
  it("blocks when disabled", () => {
    expect(decideSend({ hasContent: true, sending: false, disabled: true }))
      .toEqual({ kind: "blocked", reason: "disabled" });
  });
  // …
});
```

Any pure-data transform you find yourself writing inline in JSX is a candidate for `.logic.ts`.

### 15.5 Feature folder shape

A feature is self-contained. When building one, the canonical layout is:

```
features/<feature>/
├── index.ts                  ← public exports only
├── <Feature>.tsx             ← orchestrating component
├── <Feature>.logic.ts
├── <Feature>.logic.test.ts
├── components/               ← sub-components used only by this feature
├── hooks/                    ← feature-specific hooks (use<Feature>X.ts)
├── store.ts                  ← if feature owns global state (zustand)
└── api.ts                    ← if feature owns API calls (Tauri invokes etc.)
```

`index.ts` re-exports only what the rest of the app needs. Never import a feature's internal file from outside that feature — go through `index.ts`.

### 15.6 Imports

- `@/` aliases to `apps/desktop/src/` (already configured).
- Never import a `*.logic.ts` from the matching `.tsx` with a relative path that escapes the feature; always use the local relative import.
- Cross-feature imports go feature → feature only via the public `index.ts`.

### 15.7 Tests

- Logic tests live with code (`.logic.test.ts`).
- A `.test.tsx` (DOM render test) is reserved for components whose behavior cannot be expressed in pure logic (rare — write logic-first when possible).
- Run with `vitest`. Existing `vitest.config.ts` should already handle this.

---

## 16. Phase plan

Phases are sequential. **No phase begins until the previous one is merged-or-explicitly-acknowledged.**

### Phase 1 — Shell foundation (visual chrome only)

Goal: every existing DCC feature still works, but the shell looks like helmor.

1. Replace `apps/desktop/src/styles/` with the design tokens from §2 (`app.css`, `color-theme.css`). Wire from `main.tsx`.
2. Install Phase-1 deps: `@radix-ui/react-{dialog,dropdown-menu,popover,tooltip,tabs,scroll-area,separator,avatar,checkbox,switch,toggle,toggle-group,collapsible,context-menu,slot,label}`, `tw-animate-css`, `class-variance-authority`, `tailwind-merge`, `clsx` (some already present — confirm and `bun add` the rest in this app's `package.json`).
3. Build `components/ui/` primitives (§3) — at minimum: button, input, dialog, tooltip, tabs, dropdown-menu, popover, scroll-area, separator, badge, toggle-group, switch, command (already partly there). Verbatim class strings.
4. Replace `shell/layout.ts`, `shell/use-panels.ts` with the constants and hook contract from §4.1 / §4.5. Export `useShellPanels`.
5. Rewrite `App.tsx` shell skeleton per §4.2 — keep the existing `WorkspacesSidebar`, `SessionWorkbench`, `WorkspaceInspectorSidebar` placeholders inside, just re-skin containers and dividers.
6. Rebuild `WorkspacesSidebar` to match §5: header, group headers, row item with avatar + branch icon + truncate fade. Use existing data; no behavior changes.
7. Add `SplashScreen.tsx` + a basic visible-on-load → fade-out hook. Wire to `main.tsx`.
8. Add `Toaster` (`sonner`) to `App.tsx`.
9. Verify dark/light theme toggle works (system default if no setting yet).

**Phase 1 done = the shell visually matches helmor's silhouette: same widths, same borders, same colors, same typography. The middle panel can still be the current `SessionWorkbench`.**

**Phase 1 status as of 2026-05-01: done.** The following work is already in place in `apps/desktop`:

Implemented:

- `app.css` / `color-theme.css` wiring from `main.tsx`.
- `shell/layout.ts` and `shell/use-panels.ts` contract.
- Shell skeleton in `App.tsx` with resize separators and `Toaster`.
- Splash screen hook/component.
- Base UI primitives needed by the current shell.
- Sidebar header actions, repo picker popover, collapsed rail controls, and row styling.
- `apps/desktop/package.json` dependencies aligned with the Phase-1 Radix / utility set.

The GitHub status menu in the sidebar footer is now treated as a later chrome/account integration item, not a Phase 1 blocker. It can land with Phase 3 or any future account pass without reopening Phase 1.

### Phase 2 — Composer + conversation

1. Install: `lexical`, `@lexical/react`, `streamdown`, `@streamdown/code`.
2. Build `features/composer/` per §7 — start with: outer wrapper, Lexical editor, AutoResizePlugin, SubmitPlugin, HasContentPlugin, DraftPersistencePlugin. Defer: SlashCommandPlugin, FileMentionPlugin, Drop/Paste, AddDir, ContextBar.
3. Build `features/panel/` per §6 — `WorkspacePanel`, `ActiveThreadViewport`, `UserMessage`, `AssistantMessage`, `SystemMessage`, `EmptyState`. Wire into the existing `SessionWorkbench` data flow.
4. Build `components/streamdown-components.tsx` + `streamdown-loader.tsx` for assistant markdown.
5. Build `components/ai/code-block.tsx` (Shiki dual theme). Optional: defer Shiki integration to Phase 3 with a plain `<pre>` fallback.
6. Build `tool-call.tsx`, `reasoning.tsx` collapsibles for assistant tool/thinking display.
7. Replace existing send UI in `SessionWorkbench` with the new composer.

**Phase 2 status as of 2026-05-01: done.** The new composer/panel surface is in place in `apps/desktop`.

Implemented:

- `features/panel/` scaffold with the new thread viewport and message components.
- `features/composer/` scaffold with a Lexical-based editor, draft persistence, auto-resize, submit handling, context bar, and toolbar affordances.
- `streamdown`-based assistant rendering plus code block / link wrappers.
- `tool-call.tsx` and `reasoning.tsx` collapsible primitives for future assistant payloads.
- `SessionWorkbench` now delegates the send surface to the new composer flow.

Phase 2 closes at the composer/message-surface layer. Any deeper assistant payload enrichment can happen later without reopening this phase.

### Phase 3 — Inspector + chrome polish

**Phase 3 status as of 2026-05-01: done.** Implemented in `apps/desktop`:

- `features/inspector/` rebuilt around the three-tier chrome with section headers, resize handles, the git header shimmer, branch toolbar, commit button, and tabbed inspector body.
- `features/commit/WorkspaceCommitButton.tsx` scaffolded with state machine and mode-aware coloring.
- `features/settings/SettingsDialog.tsx` with Appearance, Model, Shortcuts, Git, Experimental, and Account sections.
- Sidebar footer now opens settings directly, and the command palette includes workspace/actions sections with settings and create/clone entry points.
- The GitHub status menu remains deferred to the later account integration pass.

### Phase 4 — Onboarding + auxiliaries

**Phase 4 status as of 2026-05-01: done.** Implemented in `apps/desktop`:

- Onboarding (§11) starts with `intro → agents → repoImport → completeTransition` and uses the simplified mockup behind the wizard.
- Dock badge integration (§12.2) is mounted at the app root.
- Updater button (§12.3) is wired to the existing Tauri updater commands.
- Shortcut display + cheatsheet overlay (§12.5) are available from the shell and command palette.
- `DccThinkingIndicator` replaced the remaining `Loader2` surfaces that mattered in the composer/tool-call chrome.
- Conductor onboarding (§12.4) stays deferred as the optional follow-up, not a Phase 4 blocker.

**Phase 4 done = full UX/UI clone shipped; remaining work is content-level (writing agents, tools, providers) and any later Conductor pass.**

---

## 17. Guardrails for any agent picking this up

- Read §0 again before you start. The "verbatim policy" is the core rule.
- Do not delete a file without checking what imports it. Keep DCC's existing backend wiring intact.
- Do not rename `App.tsx`, `main.tsx`, `package.json` paths, or anything in `crates/`, `src-tauri/`, `packages/`.
- Always finish a Phase before opening the next one. If a Phase reveals missing primitives, add them at the **end** of that Phase, not early in the next.
- When you add a new primitive to `components/ui/`, use the helmor base + variants from §3 verbatim. Do not "modernize" them.
- When you add a new feature folder, follow §15.5 layout from day one — never accumulate untyped files at the root of the feature.
- When in doubt, refer back to the helmor source paths cited inline (e.g., `helmor-main/src/features/inspector/layout.tsx`). They are stable references for this clone.
- Test only what matters: pure logic in `.logic.test.ts`. UI/render tests are optional and should be rare.
- The user (Wharley) prefers honest scoping over optimistic promises. If you discover a phase is bigger than expected, say so before coding for hours.

---

## 18. Cross-reference index

| Topic | Source path in helmor-main |
|---|---|
| Shell skeleton | `src/App.tsx` (lines ~2170–2665) |
| Shell layout constants | `src/shell/layout.ts` |
| Resize hook | `src/shell/hooks/use-panels.ts` |
| Color theme | `src/styles/color-theme.css` |
| Tailwind theme block | `src/App.css` |
| UI primitives | `src/components/ui/*` |
| Workspaces nav | `src/features/navigation/index.tsx`, `row-item.tsx` |
| Composer | `src/features/composer/index.tsx` (~1082 lines) |
| Inline badges | `src/components/inline-badge/index.tsx` |
| Conversation messages | `src/features/panel/message-components/*` |
| Streamdown overrides | `src/components/streamdown-components.tsx`, `streamdown-loader.tsx` |
| Code block | `src/components/ai/code-block.tsx` |
| Reasoning | `src/components/ai/reasoning.tsx` |
| Inspector shell | `src/features/inspector/index.tsx`, `layout.tsx` |
| Git header | `src/features/inspector/sections/git-section-header.tsx` |
| Commit button | `src/features/commit/button.tsx`, `split-button.tsx` |
| Settings | `src/features/settings/index.tsx` |
| Onboarding | `src/features/onboarding/index.tsx` |
| Dock badge | `src/features/dock-badge/*` |
| Updater button | `src/features/updater/app-update-button.tsx` |
| Shortcuts | `src/features/shortcuts/shortcut-display.tsx`, `settings-panel.tsx` |
| Splash | `src/components/splash-screen.tsx` |
| Conductor | `src/components/conductor-onboarding.tsx` |

| Topic | Source path in t3code-main |
|---|---|
| Logic/view split example | `apps/web/src/components/BranchToolbar.tsx` + `.logic.ts` + `.logic.test.ts` |
| Browser variant example | `apps/web/src/components/ChatView.browser.tsx` |
| Store example | `apps/web/src/commandPaletteStore.ts` |
| Hook example | `apps/web/src/hooks/useThreadActions.ts` |
| Lib example | `apps/web/src/lib/storage.ts` |
| Top-level loose convention | `apps/web/src/composer-logic.ts`, `localApi.ts` |
| Project rules | `AGENTS.md`, `CONTRIBUTING.md` at repo root |

---

End of blueprint. Updates to this document should be made as PRs against `evolution-dcc-desing` (or its successor) with a changelog entry below.

## Changelog

- 2026-05-01 — initial draft (Phases 1–4 planned, no code yet).
- 2026-05-01 — Phase 1 completed in `apps/desktop`: `app.css`, `color-theme.css`, shell layout/resizing, helmor-style primitives, workspace sidebar chrome, splash screen, `Toaster` wiring, sidebar repo picker/header actions, and package dependency alignment. The GitHub footer menu is deferred to Phase 3 / account integration.
- 2026-05-01 — Phase 1 follow-up: removed the legacy sidebar filter, added row hover actions, tightened the left rail toward the blueprint layout, and revalidated the desktop build/typecheck.
- 2026-05-01 — Phase 1 follow-up: removed the collapsed-rail new-workspace shortcut, tightened group header / badge spacing, and revalidated the desktop build/typecheck again.
- 2026-05-01 — Phase 2 completed in `apps/desktop`: finished the new composer/panel surface, added toolbar/context affordances, and validated the desktop build/typecheck.
- 2026-05-01 — Phase 3 completed in `apps/desktop`: rewired the inspector into the three-section chrome, added the branch toolbar, commit button scaffold, settings dialog, sidebar-footer settings entry, and command-palette actions polish; the GitHub status menu remains deferred to the later account pass.
- 2026-05-01 — Phase 4 completed in `apps/desktop`: added the onboarding wizard shell and mockup, dock badge hook, updater command wiring, shortcut cheatsheet dialog, and the DccThinkingIndicator replacement for live tool/composer loading states. Conductor onboarding remains a deferred optional pass.
- 2026-05-01 — Cursor provider clone moved off the generic stdin adapter: `cursor-agent` now uses CLI-native `create-chat`/`--resume` turns, `stream-json` parsing, and model discovery from the local CLI, with `Auto` as the safe fallback.
- 2026-05-01 — Cursor sessions now resolve the selected workspace's real working directory from the backend workspace record (`worktreePath` when present, otherwise `rootPath`) before spawning the CLI, so the imported project context is preserved instead of inheriting the DCC repo cwd.
- 2026-05-02 — Central chat viewport now uses a stable scroll area with an explicit scrollbar track, and the inspector defaults its Git section closer to half-height while translating the workspace-context labels into the active locale.
- 2026-05-02 — Git flow follow-up in `apps/desktop` and `crates/dcc-tauri`: tree rows in the inspector Git section now open a file preview panel that renders staged, unstaged, and branch-diff patches for the selected file, including new-file previews.
- 2026-05-02 — Git flow clone follow-up in `apps/desktop`, `crates/dcc-tauri`, and `src-tauri`: the selected Git file now resolves original/modified snapshots from the real workspace state and renders them in a Monaco diff surface, matching the Helmor-style code-with-changes preview instead of the old raw patch text.
- 2026-05-02 — Editor-surface clone follow-up in `apps/desktop`: the selected Git file now opens in the center panel with the Helmor-style editor chrome (traffic-light spacer, Escape shortcut, loading/error overlay) instead of feeling like a side preview embedded under the tree.
- 2026-05-02 — Commit lifecycle follow-up in `apps/desktop`, `crates/dcc-tauri`, and `src-tauri`: the inspector commit button now defaults to `Create PR`, stages all changes before commit, pushes with an upstream-aware git push, and can finish with `gh pr create --fill --web`, matching the Helmor PR/commit flow instead of the previous push-only shortcut.
- 2026-05-02 — PR action follow-up in `apps/desktop` and `crates/dcc-tauri`: the inspector now surfaces git/gh failures directly on `Criar PR`, and the `gh` browser invocation runs from the workspace directory instead of relying on a git-only `-C` flag.
- 2026-05-02 — PR action follow-up in `apps/desktop`: removed the hidden commit-message prompt from `Criar PR`, added explicit loading/success/error toasts, and switched to deterministic commit messages so the action cannot appear silent.
- 2026-05-02 — PR action follow-up in `apps/desktop` and `crates/dcc-tauri`: `Criar PR` now creates the PR directly with `gh pr create --fill --base <base> --head <branch>` instead of only opening the browser flow, and the inspector error toast now extracts richer error payloads.
- 2026-05-02 — PR action follow-up in `apps/desktop`: `Criar PR` no longer stages/commits local changes; it now behaves like the Helmor/t3code PR flow and only creates a PR from a clean, already-pushed branch, surfacing a clear message when the worktree is still dirty.
- 2026-05-02 — GitHub CLI setup parity in `apps/desktop` and `crates/dcc-tauri`: added a real `gh auth status` surface to settings/onboarding, plus a terminal handoff that mirrors the Helmor repository CLI setup flow and makes the GitHub CLI dependency explicit before PR actions.
- 2026-05-02 — GitHub CLI usability follow-up in `apps/desktop`: the account/setup card now exposes explicit connected/disconnected/checking states plus a manual re-check button so the `gh` status behaves like a first-class operational surface instead of a hidden precondition.
- 2026-05-01 — Content follow-up in `apps/desktop`: persisted the selected provider across reloads and added logic coverage for provider resolution, while keeping the phase plan unchanged.
- 2026-05-01 — Content follow-up in `apps/desktop`: the central thread viewport now projects messages by session id and renders an optimistic user prompt immediately after submit, so the chat stays visible in the center even before the backend stream catches up.
- 2026-05-01 — Content follow-up in `apps/desktop`: the center chat now merges live session events with backend thread history from Tauri, so the conversation can be reconstructed in the middle panel instead of relying only on the live feed.
- 2026-05-01 — Content follow-up in `apps/desktop`: the thread viewport now behaves more like the Helmor reference with bottom anchoring, fade-in rows, and a scroll-to-latest control when the user scrolls away from the tail.
- 2026-05-01 — Content follow-up in `apps/desktop`: thread history now comes from persisted session event records, which let the central conversation show timestamps, assistant completion state, and incomplete-turn badges instead of flattening everything into plain core events.
- 2026-05-01 — Content follow-up in `apps/desktop`: added a zero-session launch state in the center panel so the app starts from a visible conversation canvas with launch CTA and suggested prompts instead of hiding the chat area behind a blank empty-state wall.
- 2026-05-01 — Content follow-up in `apps/desktop`: added an execution state for active sessions with no rendered messages yet, so the center panel reads as "running" instead of a dead empty thread while history catches up.
- 2026-05-01 — Optional Conductor preview added in `apps/desktop` as a separate overlay from the onboarding wizard, keeping the deferred integration visually available without changing the main flow.
- 2026-05-01 — Payload enrichment in `apps/desktop`, `dcc-providers`, and `dcc-tauri`: provider stdout can now emit structured reasoning/tool-call envelopes, the session event log persists them, and the assistant bubble renders them as collapsible reasoning and tool-call annotations inside the central thread.
- 2026-05-01 — Backend integration in `apps/desktop` and `crates/dcc-tauri`: added `list_workspaces` on top of the existing SQLite-backed workspace repo, and switched the desktop shell to seed from persisted workspaces instead of the old demo list.
- 2026-05-01 — Zero-start shell follow-up in `apps/desktop`: the sidebar, inspector, and center panel now have real empty states for the "no workspace yet" path, so startup no longer depends on mock rows to look populated.
- 2026-05-01 — Workspace entry follow-up in `apps/desktop`: `Open project` now uses the native Tauri folder picker and auto-fills the project id from the chosen folder name; the clone flow moved into its own dedicated mode instead of reusing the same local-workspace modal.
- 2026-05-01 — Workspace entry follow-up in `apps/desktop` and `crates/dcc-tauri`: `Clone from URL` is now a real Rust-backed flow using `git clone` plus the existing workspace preparation pipeline, with the desktop dialog switched to a dedicated clone mode instead of the old placeholder path.
- 2026-05-01 — Clone UX follow-up in `apps/desktop` and `crates/dcc-infra`: the clone dialog now treats base branch as optional and the backend auto-detects the remote default branch via `git ls-remote --symref` when the field is left blank.
- 2026-05-01 — Open-project UX follow-up in `apps/desktop` and `crates/dcc-tauri`: the local repository picker now loads local branches from the selected git repo via `list_local_branches` and exposes them as a dropdown in the create-workspace modal instead of a free-text branch input.
- 2026-05-01 — Stream parser follow-up in `crates/dcc-providers` and `crates/dcc-tauri`: provider stdout now recognizes the real Claude (`stream_event` / `content_block_*` / `result`) and Codex (`item/started` / `item/completed` / `turn/completed`) JSONL shapes, which keeps reasoning/tool-call turns aligned with the Helmor replay model instead of relying on the temporary envelope-only bridge.
- 2026-05-01 — Workspace creation fix in `crates/dcc-tauri` and `apps/desktop`: Tauri event names for workspace/session bus emissions were switched from dotted names to slash-separated names so the emitter accepts them during workspace prepare/finalize, unblocking `create workspace` and preserving the clone flow.
- 2026-05-01 — Provider clone follow-up in `crates/dcc-core`, `crates/dcc-providers`, and `apps/desktop`: the provider catalog now carries explicit model lists, and the desktop provider picker was rewritten as a split provider/sidebar + model/details surface so the UI stops flattening provider and model into the same button row.
