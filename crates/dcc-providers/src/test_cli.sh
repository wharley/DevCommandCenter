#!/bin/sh
set -eu
case "$1" in
  --version) cat "$0.version"; exit 0 ;;
  features) cat "$0.features"; exit 0 ;;
  app-server) ;;
  *) exit 1 ;;
esac
printf 'spawn %s\n' "$*" >> "$0.log"
if [ -s "$0.next-version" ]; then
  cat "$0.next-version" > "$0.version"
  : > "$0.next-version"
fi
version=$(cat "$0.version")
version=${version#codex-cli }
user_agent="dcc/$version"
if [ -f "$0.user-agent" ]; then user_agent=$(cat "$0.user-agent"); fi
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$0.log"
  id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"id":%s,"result":{"userAgent":"%s"}}\n' "$id" "$user_agent" ;;
    *'"method":"thread/start"'*)
      if [ -f "$0.thread-error" ]; then
        printf '{"id":%s,"error":{"message":"fixture thread failure"}}\n' "$id"
      else
        printf '{"id":%s,"result":{"thread":{"id":"thread-fixture"}}}\n' "$id"
      fi ;;
    *'"method":"mcpServerStatus/list"'*)
      printf '{"id":%s,"result":{"data":[],"nextCursor":null}}\n' "$id" ;;
    *'"method":"turn/start"'*)
      printf '{"id":%s,"result":{"turn":{"id":"turn-fixture"}}}\n' "$id" ;;
  esac
done
