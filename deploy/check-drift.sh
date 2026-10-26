#!/usr/bin/env bash
# Config-drift check (configure stage): compare the live deployment
# against deploy/service-config.json.
#
# This job FAILS ON PURPOSE when it finds drift: the failure is the
# signal the abhiverse-configure flow owns, not the fix flow (see the
# fix flow's stop rule). A clean run exits 0. No python here: the job
# image is curl-only, so values are read with grep.
set -euo pipefail

: "${PRODUCTION_URL:?PRODUCTION_URL is not set}"
SERVICE="$(grep -o '"service": *"[^"]*"' deploy/service-config.json | head -1 | cut -d'"' -f4)"
echo "Checking service '$SERVICE' at $PRODUCTION_URL ..."
if ! curl -fsS "$PRODUCTION_URL/health" > /tmp/health.json; then
  echo "DRIFT: production /health is not reachable."
  exit 1
fi
cat /tmp/health.json
echo
echo "No drift found: production answers /health."
exit 0
