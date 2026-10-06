#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
root=$(pwd -P)
name="sweep-sandbox-$(printf '%s' "$root" | shasum -a 256 | cut -c1-8)"
image="$name:latest"
build() { docker build --target sandbox -t "$image" -f scripts/sandbox.Dockerfile .; }
case "${1:-up}" in
  test) docker build --target verify -f scripts/sandbox.Dockerfile . ;;
  rebuild) build; docker rm -f "$name" >/dev/null 2>&1 || true; exec "$0" up ;;
  down) docker stop "$name" ;;
  kill) docker rm -f "$name" ;;
  up)
    docker image inspect "$image" >/dev/null 2>&1 || build
    if ! docker container inspect "$name" >/dev/null 2>&1; then
      docker run -d --init --name "$name" "$image" sleep infinity >/dev/null
    fi
    docker start "$name" >/dev/null
    if [ -t 0 ]; then exec docker exec -it "$name" bash; else exec docker exec -i "$name" bash; fi
    ;;
  *) echo 'usage: scripts/sandbox.sh [up|down|kill|rebuild|test]' >&2; exit 2 ;;
esac
