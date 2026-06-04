#!/bin/bash
#
# Stop hook: runs lint (biome --write + tsc) when Claude stops.

cd "$(dirname "$0")/../.." || exit 0

# Detach stdin so nothing in this script can read from the terminal.
# Claude Code may run hooks in a background process group — any tty read
# triggers SIGTTIN and suspends the session.
exec </dev/null

OUTPUT=$(bun run lint 2>&1)
RC=$?

if [ "$RC" -ne 0 ]; then
  echo -e "Lint failed:\n$OUTPUT" >&2
  exit 2
fi
