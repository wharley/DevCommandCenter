#!/usr/bin/env bash
# Notarizes and staples a Tauri-built DMG.
#
# Tauri notarizes and staples only the .app; the DMG it wraps around that app
# comes out Developer ID signed but without a notarization ticket of its own.
# The DMG is the outermost container a user downloads, so it is submitted to
# Apple here, stapled, and checked the same way Gatekeeper will check it.
#
# Usage: scripts/notarize-macos-dmg.sh <path-to.dmg>
# Requires APPLE_ID, APPLE_PASSWORD (app-specific password) and APPLE_TEAM_ID.

set -euo pipefail

if [[ $# -ne 1 || ! -f "$1" ]]; then
  echo "Usage: $0 <path-to.dmg>" >&2
  exit 64
fi

readonly dmg_path="$1"

for variable_name in APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
  if [[ -z "${!variable_name:-}" ]]; then
    echo "Missing required environment variable: ${variable_name}" >&2
    exit 1
  fi
done

echo "Checking the DMG Developer ID signature..."
codesign --verify --strict --verbose=2 "${dmg_path}"
codesign -dv --verbose=4 "${dmg_path}" 2>&1 | grep -q '^Authority=Developer ID Application:'

echo "Submitting the DMG for notarization (waits for Apple)..."
submission_json="$(
  xcrun notarytool submit "${dmg_path}" \
    --apple-id "${APPLE_ID}" \
    --password "${APPLE_PASSWORD}" \
    --team-id "${APPLE_TEAM_ID}" \
    --wait \
    --timeout 45m \
    --output-format json
)"
echo "${submission_json}"

# notarytool exits 0 after waiting even when Apple rejects the upload, so the
# verdict comes from the JSON status.
submission_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("id", ""))' <<<"${submission_json}")"
submission_status="$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("status", ""))' <<<"${submission_json}")"

if [[ "${submission_status}" != "Accepted" ]]; then
  echo "Notarization finished with status '${submission_status}'." >&2
  if [[ -n "${submission_id}" ]]; then
    xcrun notarytool log "${submission_id}" \
      --apple-id "${APPLE_ID}" \
      --password "${APPLE_PASSWORD}" \
      --team-id "${APPLE_TEAM_ID}" >&2 || true
  fi
  exit 1
fi

echo "Stapling the notarization ticket to the DMG..."
# Stapling fetches the ticket from Apple's CDN, which can drop the connection
# or not have the fresh ticket yet (v0.1.99 Intel: "network connection was
# lost", error 68). The ticket is already issued, so retrying is safe.
staple_attempt=1
until xcrun stapler staple "${dmg_path}"; do
  if [[ "${staple_attempt}" -ge 5 ]]; then
    echo "Stapling failed after ${staple_attempt} attempts." >&2
    exit 1
  fi
  echo "Stapling attempt ${staple_attempt} failed; retrying in 30s..."
  staple_attempt=$((staple_attempt + 1))
  sleep 30
done
xcrun stapler validate "${dmg_path}"

echo "Assessing the DMG with Gatekeeper..."
# Apple's documented assessment for a disk image; `spctl --help` does not list
# --context, so it is passed unconditionally rather than feature-detected.
spctl -a -vv -t open --context context:primary-signature "${dmg_path}"
