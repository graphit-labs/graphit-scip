#!/bin/sh
set -eu

: "${GRAPHIT_SCIP_UID:?GRAPHIT_SCIP_UID is required}"
: "${GRAPHIT_SCIP_GID:?GRAPHIT_SCIP_GID is required}"
case "$GRAPHIT_SCIP_UID:$GRAPHIT_SCIP_GID" in
  *[!0-9:]*|:*|*:) echo 'UID and GID must be numeric' >&2; exit 2 ;;
esac
mkdir -p /cache/home /output
# Docker Desktop's file sharing may reject chown while still mapping writes to
# the invoking host user. Native Linux bind mounts accept the requested IDs.
chown "$GRAPHIT_SCIP_UID:$GRAPHIT_SCIP_GID" /cache /cache/home /output 2>/dev/null || true
export HOME=/cache/home
cd /workspace
exec setpriv --reuid "$GRAPHIT_SCIP_UID" --regid "$GRAPHIT_SCIP_GID" --clear-groups /usr/local/bin/graphit-scip-run
