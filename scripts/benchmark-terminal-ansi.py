#!/usr/bin/env python3
"""Compare the actual ANSI function in a Git revision and the working tree.

Runs an isolated, optimized Rust harness (not the desktop app). No app database,
provider, network, or user terminal is accessed. Requires cached Cargo crates.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SOURCE = "src-tauri/src/main.rs"
BASELINE = sys.argv[1] if len(sys.argv) > 1 else "7d4fecf"


def extract(source, name):
    start = source.index("fn strip_ansi_codes(input: &str) -> String {")
    end = source.index("\n}", start) + 2
    return source[start:end].replace("fn strip_ansi_codes(", "fn " + name + "(", 1)


before = subprocess.check_output(["git", "show", BASELINE + ":" + SOURCE], cwd=ROOT, text=True)
after = (ROOT / SOURCE).read_text()
harness = r'''
use regex::Regex;
use std::{hint::black_box, time::Instant};

fn measure(f: fn(&str) -> String, input: &str, count: usize) -> f64 {
    let started = Instant::now();
    for _ in 0..count { black_box(f(black_box(input))); }
    started.elapsed().as_secs_f64() * 1_000_000.0 / count as f64
}

fn main() {
    let fragments = ["", "plain log\n", "ação 🦀", "\r\n", "\x1b[31mred\x1b[0m",
        "\x1b[?25l", "\x1b]2;title\x07", "\x1b]2;title\x1b\\", "\x1b]unterminated",
        "\x1b[", "\x07", "\\", "\x1b[1;32m", "\x1b]9;9;notification\x07"];
    let mut checked = 0;
    for a in fragments { for b in fragments { for c in fragments {
        let input = format!("{a}{b}{c}");
        assert_eq!(before(&input), after(&input));
        checked += 1;
    } } }
    println!("{{\"equivalence_cases\":{checked}}}");
    let cases = [
        ("plain_1k", "build complete without errors\n".repeat(36), 2000),
        ("ansi_1k", "\x1b[32mbuild complete\x1b[0m\r\n".repeat(36), 2000),
        ("ansi_64k", "\x1b[32mbuild complete\x1b[0m\r\n".repeat(2300), 500),
    ];
    for (name, input, count) in cases {
        assert_eq!(before(&input), after(&input));
        for _ in 0..50 { black_box(before(&input)); black_box(after(&input)); }
        let mut old = Vec::new();
        let mut new = Vec::new();
        for round in 0..7 {
            if round % 2 == 0 {
                old.push(measure(before, &input, count)); new.push(measure(after, &input, count));
            } else {
                new.push(measure(after, &input, count)); old.push(measure(before, &input, count));
            }
        }
        let old_samples = old.clone(); let new_samples = new.clone();
        old.sort_by(f64::total_cmp); new.sort_by(f64::total_cmp);
        println!("{{\"case\":\"{name}\",\"input_bytes\":{},\"iterations_per_round\":{count},\"before_us\":{:.3},\"after_us\":{:.3},\"speedup\":{:.2},\"before_samples_us\":{:?},\"after_samples_us\":{:?}}}",
            input.len(), old[3], new[3], old[3] / new[3], old_samples, new_samples);
    }
}
'''
with tempfile.TemporaryDirectory(prefix="dcc-ansi-bench-") as directory:
    project = Path(directory)
    (project / "src").mkdir()
    (project / "Cargo.toml").write_text(
        '[package]\nname = "dcc-ansi-audit"\nversion = "0.0.0"\nedition = "2021"\n'
        '[workspace]\n[dependencies]\nregex = "=1.12.3"\n'
    )
    (project / "src/main.rs").write_text(
        extract(before, "before") + "\n" + extract(after, "after") + "\n" + harness
    )
    print(json.dumps({"baseline": BASELINE, "source": SOURCE,
                      "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()}), flush=True)
    subprocess.run(["cargo", "run", "--release", "--offline", "--manifest-path",
                    str(project / "Cargo.toml"), "--target-dir",
                    str(Path(tempfile.gettempdir()) / "dcc-ansi-audit-target")], check=True)
