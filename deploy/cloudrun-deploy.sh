#!/usr/bin/env bash
# Deploy the Abhiverse API image to Google Cloud Run (Transcend sprint).
#
# Auth: Workload Identity Federation only — no service-account keys
# anywhere, matching the no-secrets rule in AGENTS.md.
#
# One-time setup (do once, in the GCP console / gcloud CLI, following
# GitLab's "Connect to GCP with OIDC" guide): create a workload
# identity pool + provider trusting https://gitlab.com as the OIDC
# issuer, then generate a credential configuration file and store it
# as a masked, protected CI FILE variable named GCP_WIF_CRED_JSON.
# The credential config references a file for the token; this script
# writes the job's fresh OIDC token there at deploy time.
#
# Required CI variables (masked, protected): GCP_WIF_CRED_JSON (file),
# GCP_PROJECT_ID, GCP_REGION. After the first deploy, set
# PRODUCTION_URL to the service URL so check-live can use it.
set -euo pipefail

: "${GCP_WIF_CRED_JSON:?GCP_WIF_CRED_JSON file variable is not set}"
: "${GCP_PROJECT_ID:?GCP_PROJECT_ID is not set}"
: "${GCP_REGION:?GCP_REGION is not set}"
: "${CI_REGISTRY_IMAGE:?CI_REGISTRY_IMAGE is not set}"
: "${CI_COMMIT_SHORT_SHA:?CI_COMMIT_SHORT_SHA is not set}"

# The job's OIDC token (declared via id_tokens in .gitlab-ci.yml).
if [ -n "${GCP_ID_TOKEN:-}" ]; then
  printf '%s' "$GCP_ID_TOKEN" > /tmp/gitlab-jwt
elif [ -n "${CI_JOB_JWT_V2:-}" ]; then
  printf '%s' "$CI_JOB_JWT_V2" > /tmp/gitlab-jwt
else
  echo "No OIDC token available (need id_tokens in the deploy job)." >&2
  exit 1
fi

IMAGE="$CI_REGISTRY_IMAGE/api:$CI_COMMIT_SHORT_SHA"
SERVICE="abhiverse-api"

gcloud auth login --cred-file "$GCP_WIF_CRED_JSON" --quiet
gcloud config set project "$GCP_PROJECT_ID" --quiet

gcloud run deploy "$SERVICE" \
  --image "$IMAGE" \
  --region "$GCP_REGION" \
  --platform managed \
  --allow-unauthenticated \
  --port 8080 \
  --set-env-vars "ABHIVERSE_BIND_ADDR=0.0.0.0:8080,ABHIVERSE_STATE_FILE=/tmp/abhiverse-state.json" \
  --quiet

URL="$(gcloud run services describe "$SERVICE" --region "$GCP_REGION" --format 'value(status.url)')"
echo "Deployed: $URL"
echo "Set PRODUCTION_URL to this URL in the project's CI variables (masked, protected)."
curl -fsS "$URL/health" && echo "Health check passed."
