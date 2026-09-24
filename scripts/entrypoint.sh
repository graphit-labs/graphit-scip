#!/bin/sh
set -eu

: "${GRAPHIT_SCIP_UID:?GRAPHIT_SCIP_UID is required}"
: "${GRAPHIT_SCIP_GID:?GRAPHIT_SCIP_GID is required}"
case "$GRAPHIT_SCIP_UID:$GRAPHIT_SCIP_GID" in
  *[!0-9:]*|:*|*:) echo 'UID and GID must be numeric' >&2; exit 2 ;;
esac
if [ "${GRAPHIT_SCIP_OVERLAY:-0}" = 1 ]; then
  if [ ! -d /source ] || [ ! -d /overlay ]; then
    echo 'SCIP overlay requires /source and /overlay directories' >&2
    exit 2
  fi
  mkdir -p /overlay/upper /overlay/work /workspace
  if [ -n "$(ls -A /overlay/upper)" ] || [ -n "$(ls -A /overlay/work)" ]; then
    echo 'SCIP overlay requires a fresh, empty Docker volume' >&2
    exit 2
  fi
  chown "$GRAPHIT_SCIP_UID:$GRAPHIT_SCIP_GID" /overlay/upper /overlay/work
  if ! mount -t overlay overlay -o lowerdir=/source,upperdir=/overlay/upper,workdir=/overlay/work /workspace; then
    echo 'SCIP overlay mount failed; Docker must allow OverlayFS mounts' >&2
    exit 1
  fi
fi
mkdir -p /cache/home /cache/tmp /cache/xdg-cache /cache/xdg-config /output
# Docker Desktop's file sharing may reject chown while still mapping writes to
# the invoking host user. Native Linux bind mounts accept the requested IDs.
chown "$GRAPHIT_SCIP_UID:$GRAPHIT_SCIP_GID" /cache /cache/home /cache/tmp /cache/xdg-cache /cache/xdg-config /output 2>/dev/null || true
export HOME=/cache/home
export TMPDIR=/cache/tmp XDG_CACHE_HOME=/cache/xdg-cache XDG_CONFIG_HOME=/cache/xdg-config
cd /workspace
exec setpriv --no-new-privs --bounding-set=-all --reuid "$GRAPHIT_SCIP_UID" --regid "$GRAPHIT_SCIP_GID" --clear-groups /usr/local/bin/graphit-scip-run
