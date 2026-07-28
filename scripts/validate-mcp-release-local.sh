#!/usr/bin/env bash

set -euo pipefail

readonly script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_root="$(cd -- "${script_dir}/.." && pwd)"

cd "${repository_root}"

readonly authenticated_variables=(
  DCC_RUN_CLAUDE_MCP_CONFORMANCE
  DCC_RUN_CODEX_MCP_CONFORMANCE
  DCC_RUN_CURSOR_MCP_CONFORMANCE
  DCC_RUN_CLAUDE_FIGMA_MCP_SMOKE
  DCC_RUN_CODEX_FIGMA_MCP_SMOKE
  DCC_RUN_CLAUDE_GARU_MCP_SMOKE
  DCC_RUN_CODEX_GARU_MCP_SMOKE
  DCC_GARU_MCP_ALLOW_PINNED_EXECUTION
  DCC_GARU_MCP_API_KEY
)

unsafe_environment=0
for variable_name in "${authenticated_variables[@]}"; do
  if printenv "${variable_name}" >/dev/null 2>&1; then
    printf 'Refusing the local-only gate while %s is set.\n' "${variable_name}" >&2
    unsafe_environment=1
  fi
done

if ((unsafe_environment)); then
  printf 'Unset authenticated MCP variables and run the local gate again.\n' >&2
  exit 2
fi

for required_command in cargo node yarn; do
  if ! command -v "${required_command}" >/dev/null 2>&1; then
    printf 'Required command is unavailable: %s\n' "${required_command}" >&2
    exit 2
  fi
done

run_gate() {
  printf '\n==> '
  printf '%q ' "$@"
  printf '\n'
  "$@"
}

run_gate cargo fmt --all -- --check
run_gate cargo test --locked -p dcc-core mcp
run_gate cargo test --locked -p dcc-infra mcp
run_gate cargo test --locked -p dcc-providers --lib
run_gate cargo test --locked -p dcc-mcp-fixture
run_gate cargo test --locked -p dcc-tauri mcp_commands
run_gate node --test \
  sidecar/src/mcp-config.test.mjs \
  sidecar/src/permission-bridge.test.mjs
run_gate yarn workspace @dcc/contracts typecheck
run_gate yarn workspace @dcc/desktop typecheck
run_gate yarn workspace @dcc/desktop test \
  src/features/settings/mcp-integration-form.test.ts \
  src/features/settings/mcp-integration-runtime.test.ts

printf '\nLocal MCP release gate passed. No authenticated or real-service smoke was selected.\n'
