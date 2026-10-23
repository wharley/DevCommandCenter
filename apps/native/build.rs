//! Generates code from files the desktop app also uses, so both apps read
//! one source: the project fauna from `packages/mascots/src/index.ts` and
//! the Lucide icons in `assets/icons` (see `scripts/gen-icons.mjs`).

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mascots = manifest.join("../../packages/mascots/src/index.ts");
    let icons = manifest.join("assets/icons");
    let providers = manifest.join("assets/providers");
    for path in [&mascots, &icons, &providers] {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    fs::write(out.join("mascots.rs"), mascot_frames(&mascots)).unwrap();
    let tables = svg_table(&icons, "ICONS") + &svg_table(&providers, "PROVIDER_MARKS");
    fs::write(out.join("icons.rs"), tables).unwrap();
}

/// `MASCOT_FRAMES` as `&[(name, rest_rows, move_rows)]`, in file order (the
/// order `autoProjectMascot` hashes into).
fn mascot_frames(path: &Path) -> String {
    let source = fs::read_to_string(path).expect("packages/mascots/src/index.ts");
    let start = source
        .find("const MASCOT_FRAMES = {")
        .expect("MASCOT_FRAMES");
    let end = start
        + source[start..]
            .find("} as const")
            .expect("end of MASCOT_FRAMES");
    let mut frames: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    let mut current: Option<&'static str> = None;
    for line in source[start..end].lines().skip(1) {
        let trimmed = line.trim();
        if line.starts_with('\t') && !line.starts_with("\t\t") && trimmed.ends_with(": {") {
            let name = trimmed.trim_end_matches(": {").to_string();
            frames.push((name, Vec::new(), Vec::new()));
        } else if trimmed.starts_with("rest: [") {
            current = Some("rest");
        } else if trimmed.starts_with("move: [") {
            current = Some("move");
        } else if trimmed.starts_with(']') {
            current = None;
        } else if let (Some(which), Some(row)) = (current, quoted(trimmed)) {
            let frame = frames.last_mut().expect("rows belong to a mascot");
            if which == "rest" {
                &mut frame.1
            } else {
                &mut frame.2
            }
            .push(row);
        }
    }
    assert!(!frames.is_empty(), "no mascots parsed");
    let mut code =
        String::from("pub const MASCOT_FRAMES: &[(&str, [&str; 10], [&str; 10])] = &[\n");
    for (name, rest, moving) in &frames {
        assert_eq!((rest.len(), moving.len()), (10, 10), "{name} is not 10x10");
        writeln!(code, "    ({name:?}, {rest:?}, {moving:?}),").unwrap();
    }
    code.push_str("];\n");
    code
}

fn quoted(line: &str) -> Option<String> {
    let inner = line.strip_prefix('"')?;
    Some(inner[..inner.find('"')?].to_string())
}

/// Every `*.svg` in `dir`, embedded by name as `const_name`.
fn svg_table(dir: &Path, const_name: &str) -> String {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("svg asset folder")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "svg").then(|| path.file_stem()?.to_str().map(str::to_string))?
        })
        .collect();
    names.sort();
    let mut code = format!("pub const {const_name}: &[(&str, &str)] = &[\n");
    for name in names {
        let file = dir.join(format!("{name}.svg"));
        writeln!(
            code,
            "    ({name:?}, include_str!({:?})),",
            file.display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");
    code
}
