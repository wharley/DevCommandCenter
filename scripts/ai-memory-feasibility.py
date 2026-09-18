#!/usr/bin/env python3
"""Isolated protocol smoke for an ai-memory release; no DCC runtime or LLM calls.

Pass a previously downloaded, checksum-verified upstream executable with --binary.
Uses synthetic data, a temporary HOME/data directory, and a loopback-only server.
Does not install hooks, modify provider configuration, or preserve the test wiki.
"""

import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request


def run(binary, expected_version=None):
    checks = []

    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        checks.append(name)

    with tempfile.TemporaryDirectory(prefix="dcc-ai-memory-smoke-") as folder:
        root = Path(folder)
        home = root / "home"
        home.mkdir()
        data = root / "data"
        # Do not inherit credentials, provider settings, proxies or real HOME.
        env = {"PATH": os.defpath, "HOME": str(home), "TMPDIR": folder}
        subprocess.run([binary, "--data-dir", str(data), "init"], env=env,
                       check=True, capture_output=True)
        (data / "config.toml").write_text(
            'log_level = "warn"\n[auto_improve.scheduler]\nenabled = false\n'
        )
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = "http://127.0.0.1:" + str(port)
        process = None
        log = open(root / "server.log", "w")
        client = urllib.request.build_opener(urllib.request.ProxyHandler({}))

        def post(path, body):
            request = urllib.request.Request(base + path, json.dumps(body).encode(), {
                "Content-Type": "application/json",
                "Accept": "application/json, text/event-stream",
            })
            with client.open(request, timeout=10) as response:
                raw = response.read()
                try:
                    return json.loads(raw)
                except ValueError:
                    return {"status": response.status, "body": raw.decode()}

        def rpc(method, params):
            return post("/mcp", {"jsonrpc": "2.0", "id": 1,
                                  "method": method, "params": params})

        def call(name, **arguments):
            response = rpc("tools/call", {"name": name, "arguments": arguments})
            if "error" in response or response.get("result", {}).get("isError"):
                return {"error": response}
            blocks = response["result"]["content"]
            return json.loads(next(item["text"] for item in blocks if item["type"] == "text"))

        def start():
            nonlocal process
            process = subprocess.Popen([
                binary, "--data-dir", str(data), "serve", "--transport", "http",
                "--bind", "127.0.0.1:" + str(port), "--no-watcher",
            ], env=env, stdout=log, stderr=log)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError("server exited during startup")
                try:
                    return rpc("initialize", {"protocolVersion": "2024-11-05",
                        "capabilities": {}, "clientInfo": {"name": "dcc-spike", "version": "1"}})
                except OSError:
                    time.sleep(0.05)
            raise TimeoutError("server startup")

        def stop():
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()

        def eventually(fn):
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                result = fn()
                if result:
                    return result
                time.sleep(0.05)
            raise TimeoutError("asynchronous hook processing")

        a = {"workspace": "dcc-spike", "project": "project-a"}
        b = {"workspace": "dcc-spike", "project": "project-b"}
        cwd = str(root / "synthetic-checkout")

        def hook(event, key, **body):
            query = urllib.parse.urlencode(dict(a, event=event, agent="claude-code",
                                                extension="dcc", ingest_key=key))
            return post("/hook?" + query, dict(session_id="dcc-synthetic-session", cwd=cwd, **body))

        def batch_item(scope, event, key, session_id, **body):
            query = urllib.parse.urlencode(dict(scope, event=event, agent="claude-code",
                                                session_id=session_id, extension="dcc",
                                                ingest_key=key))
            return {
                "url": base + "/hook?" + query,
                "body": dict(session_id=session_id, cwd=cwd, **body),
            }

        try:
            init = start()["result"]
            upstream_version = init["serverInfo"]["version"]
            check("upstream_version", expected_version is None or upstream_version == expected_version)
            schemas = rpc("tools/list", {})["result"]["tools"]
            tool_count = len(schemas)
            check("mcp_discovers_memory_query", any(t["name"] == "memory_query" for t in schemas))
            query_schema = next(t for t in schemas if t["name"] == "memory_query")["inputSchema"]
            check("query_has_no_native_branch_or_task_filter",
                  not {"branch", "task_id", "worktree_id", "tags"}.intersection(query_schema["properties"]))
            for scope, sentinel in [(a, "ALPHASENTINEL"), (b, "BRAVOSENTINEL")]:
                result = call("memory_write_page", **scope, path="decisions/checkout.md",
                              body="# Checkout\n" + sentinel + " Preserve o checkout escolhido.")
                check("write_" + scope["project"], "page_id" in result)
            check("fts_recalls_written_decision", bool(call("memory_query", **a, query="ALPHASENTINEL")["hits"]))
            check("explicit_scope_excludes_other_project", not call("memory_query", **b, query="ALPHASENTINEL")["hits"])
            check("unknown_project_fails_closed", "error" in call("memory_query", workspace="dcc-spike", project="missing", query="ALPHASENTINEL"))

            def scoped_read(i):
                scope, term = (a, "BRAVOSENTINEL") if i % 2 else (b, "ALPHASENTINEL")
                return not call("memory_query", **scope, query=term)["hits"]

            with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
                check("20_concurrent_explicit_scope_reads_do_not_cross", all(pool.map(scoped_read, range(20))))
            # Tags preserve labels, but project search does not enforce branch applicability.
            for branch in ["main", "feature"]:
                call("memory_write_page", **a, path="notes/" + branch + ".md",
                     body="# Branch\nBRANCHSENTINEL " + branch, tags=["branch:" + branch])
            check("same_project_query_includes_both_branch_tagged_pages",
                  len(call("memory_query", **a, query="BRANCHSENTINEL")["hits"]) == 2)

            check("hook_start_accepted", hook("session-start", "event-start")["status"] == 202)
            fake_key = "sk-" + "synthetic" * 5
            body = {"prompt": "CAPTURESENTINEL Decisao: manter Local. Pendente: retomada. " + fake_key}
            hook("user-prompt", "event-prompt", **body)
            hook("user-prompt", "event-prompt", **body)
            hook("stop", "event-stop", last_assistant_message="ASSISTANTSENTINEL decisao apenas na resposta")
            hook("session-end", "event-end")
            rows = eventually(lambda: call("memory_handoff_list", **a).get("handoffs"))
            session_id = rows[0]["from_session_id"]
            observations = call("memory_read_session_observations", **a, session_id=session_id)
            check("retry_ingest_key_deduplicates_prompt", observations["total"] == 4)
            serialized = json.dumps(observations)
            check("synthetic_api_key_redacted_from_retrieved_observations", fake_key not in serialized and "CAPTURESENTINEL" in serialized)
            check("assistant_text_not_captured_by_default", "ASSISTANTSENTINEL" not in serialized)
            check("session_end_generates_searchable_page_without_llm",
                  bool(call("memory_query", **a, query="CAPTURESENTINEL")["hits"]))
            check("automatic_handoff_contains_prompt_context", "CAPTURESENTINEL" in rows[0]["summary"])
            check("handoff_accept_succeeds", "error" not in call("memory_handoff_accept", **a, handoff_id=rows[0]["id"], cwd=cwd))
            check("accepted_handoff_leaves_open_list", not call("memory_handoff_list", **a)["handoffs"])

            batch = post("/hook/batch", [
                batch_item(a, "session-start", "batch-start", "dcc-batch-session"),
                batch_item(a, "user-prompt", "batch-prompt", "dcc-batch-session",
                           prompt="BATCHSENTINEL decision delivered through hook batch"),
                batch_item(a, "session-end", "batch-end", "dcc-batch-session"),
            ])
            check("hook_batch_accepts_events", batch.get("accepted") == 3)
            accepted_indices = batch.get("accepted_indices")
            check("hook_batch_ack_is_contiguous_or_indexed",
                  accepted_indices in (None, [0, 1, 2]))
            check("hook_batch_event_is_searchable",
                  bool(eventually(lambda: call("memory_query", **a,
                                                query="BATCHSENTINEL")["hits"])))
            batch_retry = post("/hook/batch", [
                batch_item(a, "session-start", "batch-start", "dcc-batch-session"),
                batch_item(a, "user-prompt", "batch-prompt", "dcc-batch-session",
                           prompt="BATCHSENTINEL decision delivered through hook batch"),
                batch_item(a, "session-end", "batch-end", "dcc-batch-session"),
            ])
            check("hook_batch_retry_acknowledges_events", batch_retry.get("accepted") == 3)
            batch_hits = eventually(lambda: call("memory_query", **a,
                                                  query="BATCHSENTINEL")["hits"])
            check("hook_batch_retry_is_idempotent", len(batch_hits) == 1)

            stop()
            try:
                call("memory_query", **a, query="ALPHASENTINEL")
                unavailable = False
            except OSError:
                unavailable = True
            check("stopped_server_reports_unavailable", unavailable)
            start()
            check("decision_survives_server_restart", bool(call("memory_query", **a, query="ALPHASENTINEL")["hits"]))
            check("captured_session_survives_restart", bool(call("memory_query", **a, query="CAPTURESENTINEL")["hits"]))
            check("wiki_markdown_exists", any((data / "wiki").rglob("checkout.md")))
        finally:
            stop()
            log.close()
    return {"upstream_version": upstream_version, "mcp_tool_count": tool_count,
            "test_kind": "isolated HTTP/MCP protocol smoke",
            "provider_runtime_executed": False, "llm_calls": 0,
            "checks_passed": len(checks), "checks": checks}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--expected-version")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = run(str(args.binary.resolve(strict=True)), args.expected_version)
    rendered = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered)
    print(rendered, end="")
