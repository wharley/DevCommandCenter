# DCC brand identity

The Dev Command Center mark represents orchestration without turning DCC into a
literal mascot.

At first glance, the mark is a compact technology symbol: multiple execution
flows cross through a shared command core. On a second look, its four continuous
paths and eight endpoints suggest an abstract octopus coordinating several
agents at once.

The small terminal prompt inside the core preserves a connection to the original
DCC icon. It is intentionally a secondary detail; the outer silhouette must
remain recognizable when the prompt is too small to see.

## Visual principles

- **Orchestration, not a network diagram.** The paths represent coordinated work,
  not a fixed number of providers, agents, or processes.
- **Abstract, not a mascot.** The mark has no face, eyes, head, or literal animal
  anatomy.
- **Strong at small sizes.** The silhouette is the primary identifier. Gradients,
  glow, and the terminal detail are supporting layers.
- **Blue continuity.** Electric blue preserves the existing DCC identity. Cyan
  adds clarity and violet distinguishes one flow without dominating the mark.
- **Calm depth.** Dimensional effects should stay restrained so the mark remains
  suitable for a professional developer tool.

## Asset usage

| Asset | Purpose |
| --- | --- |
| [`src-tauri/icons/app-icon.svg`](../src-tauri/icons/app-icon.svg) | Master application icon with the dark squircle container |
| [`public/dcc-glyph.svg`](../public/dcc-glyph.svg) | Standalone glyph for compact product UI |
| [`public/dcc-mark.svg`](../public/dcc-mark.svg) | Web and documentation version of the complete mark |
| `src-tauri/icons/` | Generated platform assets for macOS, Linux, Windows, Android, and iOS |

Use the complete mark for application icons, launchers, favicons, and prominent
brand placement. Use the standalone glyph inside DCC when another rounded-square
container would feel redundant.

Do not add a face, separate the paths into generic node-and-connector graphics,
change the proportions, or rely on glow to make the silhouette legible.

## Regenerating platform icons

After changing the master SVG, regenerate the platform-specific assets with:

```bash
yarn tauri icon src-tauri/icons/app-icon.svg \
  --output src-tauri/icons \
  --ios-color '#05091C'
```

Review at least the 32 px icon, the macOS Dock icon, and the standalone glyph in
the sidebar before accepting a visual change.
