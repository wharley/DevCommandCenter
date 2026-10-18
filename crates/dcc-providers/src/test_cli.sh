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
    *'"method":"thread/resume"'*)
      thread=$(printf '%s\n' "$line" | sed -n 's/.*"threadId":"\([^"]*\)".*/\1/p')
      if [ "$thread" = "thread-gone" ]; then
        printf '{"id":%s,"error":{"message":"no rollout found for thread id %s"}}\n' "$id" "$thread"
      else
        printf '{"id":%s,"result":{"thread":{"id":"%s"}}}\n' "$id" "$thread"
      fi ;;
    *'"method":"mcpServerStatus/list"'*)
      printf '{"id":%s,"result":{"data":[],"nextCursor":null}}\n' "$id" ;;
    *'"method":"turn/start"'*)
      if [ -f "$0.goal-run" ]; then
        printf '{"id":%s,"result":{"turn":{"id":"turn-1"}}}\n' "$id"
        printf '{"method":"turn/started","params":{"threadId":"thread-fixture","turn":{"id":"turn-1","status":"inProgress"}}}\n'
      else
        printf '{"id":%s,"result":{"turn":{"id":"turn-fixture"}}}\n' "$id"
      fi ;;
    *'"method":"thread/goal/get"'*)
      if [ -s "$0.goal-status" ]; then
        printf '{"id":%s,"result":{"goal":{"threadId":"thread-fixture","objective":"fixture goal","status":"%s","tokensUsed":10,"tokenBudget":null,"timeUsedSeconds":1}}}\n' "$id" "$(cat "$0.goal-status")"
      else
        printf '{"id":%s,"result":{"goal":null}}\n' "$id"
      fi ;;
    *'"method":"thread/goal/clear"'*)
      : > "$0.goal-status"
      printf '{"id":%s,"result":{"cleared":true}}\n' "$id"
      printf '{"method":"thread/goal/cleared","params":{"threadId":"thread-fixture"}}\n' ;;
    *'"method":"thread/goal/set"'*)
      status=$(printf '%s\n' "$line" | sed -n 's/.*"status":"\([a-zA-Z]*\)".*/\1/p')
      printf '%s' "$status" > "$0.goal-status"
      goal=$(printf '{"threadId":"thread-fixture","objective":"fixture goal","status":"%s","tokensUsed":10,"tokenBudget":null,"timeUsedSeconds":1}' "$status")
      printf '{"id":%s,"result":{"goal":%s}}\n' "$id" "$goal"
      printf '{"method":"thread/goal/updated","params":{"threadId":"thread-fixture","turnId":null,"goal":%s}}\n' "$goal"
      if [ "$status" = "active" ] && [ -f "$0.goal-run" ]; then
        if [ "$(cat "$0.goal-run")" = "fail" ]; then
          printf '{"method":"turn/completed","params":{"threadId":"thread-fixture","turn":{"id":"turn-1","status":"failed","error":{"message":"fixture failure"}}}}\n'
          printf '{"method":"turn/started","params":{"threadId":"thread-fixture","turn":{"id":"turn-2","status":"inProgress"}}}\n'
        else
          printf '{"method":"turn/completed","params":{"threadId":"thread-fixture","turn":{"id":"turn-1","status":"completed"}}}\n'
          sleep 0.3
          printf '{"method":"turn/started","params":{"threadId":"thread-fixture","turn":{"id":"turn-2","status":"inProgress"}}}\n'
          printf '{"method":"turn/completed","params":{"threadId":"thread-fixture","turn":{"id":"turn-2","status":"completed"}}}\n'
          printf '{"method":"thread/goal/updated","params":{"threadId":"thread-fixture","turnId":"turn-2","goal":{"threadId":"thread-fixture","objective":"fixture goal","status":"complete","tokensUsed":20,"tokenBudget":null,"timeUsedSeconds":2}}}\n'
        fi
      fi ;;
    *'"method":"turn/interrupt"'*)
      printf '{"id":%s,"result":{}}\n' "$id" ;;
  esac
done
