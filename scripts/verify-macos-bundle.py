#!/usr/bin/env python3
"""Reject build-machine dylibs in macOS bundles; optionally smoke-test launch."""

import argparse
import pathlib
import plistlib
import re
import subprocess
import tempfile
import time


MACH_O_MAGICS = {
    bytes.fromhex(value)
    for value in ("feedface", "cefaedfe", "feedfacf", "cffaedfe",
                  "cafebabe", "bebafeca", "cafebabf", "bfbafeca")
}


def verify_bundle(app):
    with (app / "Contents/Info.plist").open("rb") as file:
        executable_name = plistlib.load(file)["CFBundleExecutable"]
    executable = app / "Contents/MacOS" / executable_name
    if not executable.is_file():
        raise RuntimeError(f"Missing app executable: {executable}")

    checked = set()
    total_bytes = 0
    errors = []
    # Include native sidecars and vendor binaries, not just the main executable.
    for path in sorted((app / "Contents").rglob("*")):
        if not path.is_file() or path.resolve() in checked:
            continue
        total_bytes += path.stat().st_size
        with path.open("rb") as file:
            if file.read(4) not in MACH_O_MAGICS:
                continue
        checked.add(path.resolve())
        print(f"{path.relative_to(app)}: {path.stat().st_size / 1024**2:.2f} MiB", flush=True)
        output = subprocess.check_output(["otool", "-L", str(path)], text=True)
        dependencies = re.findall(r"^\s+(.+?) \(compatibility version", output, re.M)
        if not dependencies:
            raise RuntimeError(f"Could not inspect Mach-O dependencies: {path}")
        for dependency in dependencies:
            if dependency.startswith(("/System/Library/", "/usr/lib/",
                                      "@rpath/", "@loader_path/", "@executable_path/")):
                continue
            errors.append(f"{path.relative_to(app)} -> {dependency}")

    if executable.resolve() not in checked:
        raise RuntimeError(f"App executable is not Mach-O: {executable}")
    if errors:
        raise RuntimeError("Bundle links libraries outside macOS/the app:\n  "
                           + "\n  ".join(errors))
    print(f"Validated dylib paths in {len(checked)} Mach-O files: {app}", flush=True)
    print(f"Bundle file content: {total_bytes / 1024**2:.2f} MiB", flush=True)
    return executable


def smoke_test(executable):
    # Run on the macOS CI runner after signing; library validation failures
    # terminate before application code, even when notarization has passed.
    with tempfile.TemporaryFile() as log:
        process = subprocess.Popen([str(executable)], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    log.seek(0)
                    output = log.read().decode(errors="replace")
                    raise RuntimeError(f"App exited during launch (code {process.returncode}):\n{output}")
                time.sleep(0.25)
            print("App remained running for the 10-second launch smoke test.", flush=True)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=pathlib.Path)
    parser.add_argument("--smoke-test", action="store_true")
    args = parser.parse_args()
    try:
        executable = verify_bundle(args.app.resolve())
        if args.smoke_test:
            smoke_test(executable)
    except (OSError, KeyError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"macOS bundle validation failed: {error}\n")


if __name__ == "__main__":
    main()
