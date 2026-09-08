**DCC performance audit — September 7, 2026**

Five issues were reproduced and fixed: retained sidecar listeners, retained exited terminals, an unbounded publication queue when animation frames stop, incorrect session buffer accounting, and repeated regex compilation during terminal processing. The results below compare actual repository code at revision `7d4fecf` (v0.1.64) with the fixes accompanying this report.

This audit combines code review, regression tests, retained resource counts, and isolated benchmarks. It does not certify that the entire application is free of memory leaks. No comparative heap/RSS measurement of the complete application or prolonged WebKit soak test was performed.

Measurement environment: macOS ARM64, Node 22.21.1, Rust 1.94.0, regex 1.12.3.

**Confirmed issues and fixes**

| Priority | Code path and cause | Before → after evidence |
|---|---|---|
| P1 | [Session buffer](../apps/desktop/src/features/sessions/session-live-event-buffer.ts): insertion/removal used JSON size, but delta coalescing added unescaped character counts. Changed metadata was also accounted for incorrectly. | Across 30 deltas containing escapes: peak buffer size metric of 20,400,146 → 7,200,146 bytes; minimum counter of −7,200,000 → 0. The configured limit is 8,388,608. |
| P1 | [Publication queue](../apps/desktop/src/features/sessions/session-event-frame-batch.ts): `pending` grew until the next frame without its own limit. | With the scheduler withholding frames: 10,000 pending events → at most 255 after each `enqueue`. All 10,000 events were published in order after releasing the remaining frame. |
| P2 | [Terminal cleanup](../apps/desktop/src/features/terminal/terminal-store.ts): workspace removal skipped entries without an active PTY or pending spawn. | An exited terminal containing over 2 million characters remained accessible in the store → the entry is removed; no kill command is sent to the already exited PTY. |
| P2 | [Permissions](../sidecar/src/permission-bridge.mjs) and questions in [index.mjs](../sidecar/src/index.mjs): `{ once: true }` listeners remained after a normal response. | For 100, 1,000, and 10,000 completed permissions with the signal kept alive: respectively 100, 1,000, and 10,000 listeners → zero in all three cases. |
| P2 | `strip_ansi_codes` in [main.rs](../src-tauri/src/main.rs): compiled three regexes on every call, including output processing and activity queries. | The same patterns are compiled once with `LazyLock`; output checks and timings are detailed below. |

The listener test demonstrates retention while the `AbortSignal` remains alive. It does not demonstrate that every real SDK request keeps its signal alive indefinitely. The new [response helper](../sidecar/src/pending-response.mjs) releases listeners immediately on response, cancellation, and turn completion. It also handles signals that arrive already aborted; previously that case left the Promise pending. Inspection uses the public [Node getEventListeners API](https://nodejs.org/docs/latest-v22.x/api/events.html#eventsgeteventlistenersemitterortarget-eventname).

The queue now publishes early when it reaches 256 events. Frame suspension in hidden pages is documented behavior of [requestAnimationFrame](https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame). This condition was simulated deterministically here; it was not reproduced by minimizing the installed DCC application. The limit applies to event count, not individual payload size. A smaller batch may still wait for the next frame: the change bounds queue growth but does not impose a maximum background publication delay.

The session buffer retains its existing `JSON.stringify(event).length * 2` metric, now consistent across insertion, coalescing, eviction, and purging. This is a serialized representation budget, not a heap measurement. Incremental accounting handles escapes, metadata changes, and Unicode pairs split between deltas without serializing the accumulated content again. The limit remains per session and does not bound the sum of all sessions. Evicting the overlay when the limit is exceeded follows the existing policy; durable SQLite history was not changed.

**Measured CPU improvement**

The [Rust benchmark](../scripts/benchmark-terminal-ansi.py) extracts the original function with `git show` and the current function directly from the production source file. It compiles both into the same optimized executable, warms up each case, alternates the before/after order, and reports the median of seven rounds. `black_box` keeps the work observable. Output equivalence was checked across 2,744 combinations of text, ANSI controls, Unicode, and incomplete sequences, plus the three benchmark payloads.

| Actual payload | Calls per round | Before, µs/call | After, µs/call | Before/after ratio |
|---|---:|---:|---:|---:|
| Plain text, 1,080 bytes | 2,000 | 69.572 | 0.147 | 472× |
| ANSI colored text, 900 bytes | 2,000 | 75.924 | 2.758 | 27.53× |
| ANSI colored text, 57,500 bytes | 500 | 250.841 | 170.220 | 1.47× |

The large improvement on small payloads comes from eliminating repeated compilation. Text scanning accounts for more of the cost on larger payloads. This is consistent with the library's own [guidance on regex recompilation](https://docs.rs/regex/1.12.3/regex/#avoid-re-compiling-regexes-especially-in-a-loop). These numbers measure elapsed function time in a single thread harness. They do not mean the whole DCC application became 27× faster, and they do not measure contention across multiple terminals or the reduction in process CPU usage.

The accounting fix's cost was also measured: for 10,000 small deltas, median time increased from 4.602 ms to 5.629 ms in Node, approximately 0.103 µs more per delta. This change therefore bounds memory with a small CPU cost in the measured scenario; it is not a throughput optimization. The scripts record all samples to make that tradeoff visible.

Raw data: [ANSI](performance/2026-09-07-ansi.jsonl), [listeners](performance/2026-09-07-sidecar.jsonl), [buffer and its CPU cost](performance/2026-09-07-session-buffer.jsonl).

**Additional findings without an applied or quantified application improvement**

| Priority | Code evidence | Required follow-up validation |
|---|---|---|
| P1 | The PTY readers `spawn_terminal_reader_thread` in [main.rs](../src-tauri/src/main.rs) and `spawn_headless_terminal_reader_thread` in [http_api.rs](../src-tauri/src/http_api.rs) use `mpsc::channel` between reading and processing. The API has a [conceptually unlimited queue](https://doc.rust-lang.org/std/sync/mpsc/fn.channel.html). | Compare a bounded queue with the current flow when the producer outpaces the consumer; measure peak memory, throughput, latency, and shutdown without deadlock. |
| P1 | The Claude stderr reader in [claude_sdk_sidecar.rs](../crates/dcc-providers/src/claude_sdk_sidecar.rs) appends all lines to a `String` until the process exits. | Reproduce with a synthetic sidecar continuously emitting logs; define tail retention and a per-line limit while preserving final diagnostics. This test was not performed in this audit. |
| P2 | Native scrollback limits chunk count: 1,000 on desktop and 3,000 over HTTP, despite the `MAX_LINES` name. With 128 KiB chunks, text alone represents approximately 125 and 375 MiB respectively, before copies/serialization. | Test a byte budget shared with restoration and persistence. These figures are scenario calculations, not observed RSS. |
| P2 | The activity query refreshes the process list and traverses descendants per terminal; the frontend requests it every two seconds while a PTY exists. | Measure with 1/10/30 terminals. Compare a child-process index keyed by PID and snapshot reuse; preserve activity indicators for hidden terminals. |
| P3 | `codeTreeExpansionCache` in [workspace-inspector-sidebar.tsx](../apps/desktop/src/features/inspector/workspace-inspector-sidebar.tsx) retains sets per root without visible eviction. | Measure retention when visiting/removing many roots; introduce workspace cleanup or a bounded cache if the volume warrants it. |

The review also covered feed subscription lifecycle, purging after history confirmation, React Query defaults, tab cleanup, and Claude/Codex runtimes. Useful protections already exist: event batching and coalescing, invalidation rules that avoid refetching per token, a persisted cache limited to 1 million characters, garbage collection of inactive heavy payloads after two minutes, `kill_on_drop` on the examined processes, and bounded broadcast channels. These protections do not establish the absence of leaks. QueryClient listeners have application lifetime and the client is created once in `main.tsx`; they were not classified as navigation leaks solely because there is no local cleanup.

**Reproduction and validation**

From the project root, with Node 22 on PATH:

```sh
node scripts/audit-sidecar-listeners.mjs 7d4fecf
node scripts/audit-session-buffer.mjs 7d4fecf
python3 scripts/benchmark-terminal-ansi.py 7d4fecf
node --test sidecar/src/*.test.mjs
```

From `apps/desktop`:

```sh
node ../../node_modules/vitest/vitest.mjs run
node ../../node_modules/typescript/bin/tsc --noEmit
```

Results: **120 files / 693 desktop tests passed; 31 sidecar tests passed; TypeScript reported no errors; `git diff --check` reported no errors**. The new tests exposed failures in the previous implementation before the fixes. The Rust harness was compiled and executed in release mode with the same regex and transitive dependency versions as the lockfile; the complete Tauri binary was not built or tested. No real sessions were restarted, conversation contents accessed, user database changed, or deployment performed.

Measuring the remaining application-wide effect requires comparing release builds of the original and corrected revisions under the same synthetic workload: idle, streaming, hidden window, intense PTY output, and workspace open/close cycles. Record accumulated CPU time, RSS/footprint, heap after GC, processes/threads, and p95 latency; repeat the cycles and observe the plateau after resources are released. Only then can total DCC memory/CPU savings be attributed to these changes and the remaining native findings evaluated.
