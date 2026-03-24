/**
 * xterm 5.3: Viewport._innerRefresh can run in a RAF after the renderer is cleared,
 * causing TypeError on this._renderService.dimensions (getter uses _renderer.value).
 * Guard with hasRenderer() — same idea as upstream defensive checks elsewhere.
 *
 * Patches node_modules/xterm/lib/xterm.js. Clears Vite's prebundle for xterm so the
 * dev server loads the fixed code (otherwise .vite/deps/xterm.js stays stale).
 */
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");
const xtermLib = path.join(root, "node_modules", "xterm", "lib", "xterm.js");

const OLD_REFRESH =
  "_innerRefresh(){if(this._charSizeService.height>0){this._currentRowHeight=this._renderService.dimensions.device.cell.height";
const NEW_REFRESH =
  "_innerRefresh(){if(!this._renderService.hasRenderer()){this._refreshAnimationFrame=null;return}if(this._charSizeService.height>0){this._currentRowHeight=this._renderService.dimensions.device.cell.height";

const OLD_GETTER = "get dimensions(){return this._renderer.value.dimensions}";
const NEW_GETTER = "get dimensions(){if(!this._renderer.value)return{device:{cell:{width:0,height:0},canvas:{width:0,height:0}},css:{cell:{width:0,height:0},canvas:{width:0,height:0}}};return this._renderer.value.dimensions}";

if (!fs.existsSync(xtermLib)) {
  console.warn("[patch-xterm] skip: node_modules/xterm/lib/xterm.js missing");
  process.exit(0);
}

let s = fs.readFileSync(xtermLib, "utf8");
let modified = false;

if (s.includes(NEW_REFRESH)) {
  console.log("[patch-xterm] lib/xterm.js: Viewport _innerRefresh already patched");
} else if (s.includes(OLD_REFRESH)) {
  s = s.replace(OLD_REFRESH, NEW_REFRESH);
  modified = true;
  console.log("[patch-xterm] lib/xterm.js: Viewport _innerRefresh guard applied");
} else {
  console.warn("[patch-xterm] lib/xterm.js: Viewport _innerRefresh snippet not found");
}

if (s.includes(NEW_GETTER)) {
  console.log("[patch-xterm] lib/xterm.js: RenderService dimensions already patched");
} else if (s.includes(OLD_GETTER)) {
  s = s.replace(OLD_GETTER, NEW_GETTER);
  modified = true;
  console.log("[patch-xterm] lib/xterm.js: RenderService dimensions guard applied");
} else {
  console.warn("[patch-xterm] lib/xterm.js: RenderService dimensions snippet not found");
}

if (modified) {
  fs.writeFileSync(xtermLib, s);
}

const viteDeps = path.join(root, "node_modules", ".vite", "deps");
for (const f of ["xterm.js", "xterm.js.map"]) {
  const p = path.join(viteDeps, f);
  try {
    fs.unlinkSync(p);
  } catch {
    /* ignore */
  }
}
