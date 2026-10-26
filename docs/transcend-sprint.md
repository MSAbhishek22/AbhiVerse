# Transcend sprint — what GitLab Duo adds around Abhiverse

Entry for the GitLab Transcend "Life After Code" hackathon, **Path B
(Bring Your Own)**, autonomy level **Supervised**: agents propose and
open merge requests; humans merge and deploy. Nothing here merges or
deploys by itself. See `AGENTS.md` "GitLab Duo governance" — it binds
every flow below.

## Before (the original project, github.com/MSAbhishek22/AbhiVerse)

Abhiverse existed before the hackathon's submission period (v0.5.4,
MIT): an Android controller, a Windows agent, and a Rust coordination
API, with GitHub CI (Rust, Windows target, Flutter, Android). What
was manual and fragmented: planning lived in issues with no follow-up,
review depended on whoever was awake, a broken pipeline waited for a
human to read logs, security findings waited for triage, releases
were notes written by hand, production config drifted silently, and
nobody re-checked production after deploy.

## After (what this sprint adds, 26 October 2026)

| Lifecycle stage | Before | After (this sprint) |
|---|---|---|
| Plan | Issue sits, no follow-up | `abhiverse-plan` turns a new issue into a scoped plan respecting the security invariants |
| Create + verify | Review by whoever is awake | `abhiverse-review` reviews every MR against per-path rules; its one question: can this weaken a security invariant? |
| Verify | Broken pipeline waits for a human | `abhiverse-fix` reads failing jobs and opens the smallest fix MR (never by weakening a security test) |
| Package | Image built ad hoc | `.gitlab-ci.yml` builds the API image to the GitLab Container Registry on every push |
| Secure | Findings wait for triage | `abhiverse-secure` triages secret-detection, semgrep, cargo-audit, and trivy findings by reachability; dismisses nothing |
| Release | Release notes by hand | `abhiverse-release` checks the live service (`/health` + a nonsense path) and posts the release summary; deploy itself stays a manual job |
| Configure | Drift silent | `config-drift` job compares production against `deploy/service-config.json`; its deliberate failure is the configure flow's signal |
| Monitor | Nobody re-checks production | Scheduled pipeline re-checks production health every 6 hours |
| Govern | Rules in people's heads | `AGENTS.md` + `.gitlab/duo/mr-review-instructions.yaml` bind every agent; agents may not merge or deploy |

Files added for this sprint (all dated inside the submission period):

- `.gitlab/duo/flows/*.yml` (5 flows) + `.gitlab/duo/flows.json`
  (manifest with triggers and pipeline-hook conditions)
- `.gitlab/duo/agent-config.yml` (sandbox network allowlist),
  `.gitlab/duo/mr-review-instructions.yaml` (per-path review rules)
- `AGENTS.md` (Duo governance section)
- `.gitlab-ci.yml` (12 jobs, 7 stages — see below)
- `deploy/duo_sync.py` (repo-is-source-of-truth sync via GraphQL),
  `deploy/cloudrun-deploy.sh` (WIF-only Cloud Run deploy),
  `deploy/check-drift.sh`, `deploy/service-config.json`

## The pipeline (`.gitlab-ci.yml`)

Verify: Rust (fmt, clippy, tests) · Flutter (analyze, tests) ·
Android APK · Duo flow validation. Secure: secret detection,
semgrep, cargo audit, trivy image scan. Package: API image to the
GitLab registry. Release: manual Cloud Run deploy, then live checks.
Configure: drift job. Monitor: scheduled health checks. The deploy
job skips cleanly when the GCP variables are absent, so the pipeline
stays green on forks and on the free fallback repo; security jobs use
free tooling only, so nothing here needs a paid tier to run.

## How to reproduce

1. Register as a GitLab Contributor (contributors.gitlab.com) and/or
   create a GitLab project; import this repository into it.
2. Set `.gitlab/duo/flows.json` `"project"` to the GitLab project
   path (or pass `--project` to the sync script).
3. Run `GITLAB_TOKEN=<api token> python3 deploy/duo_sync.py` (dry-run
   first with `--dry-run`). It creates the five flows in the AI
   Catalog and enables them with their triggers.
4. Set the CI variables the pipeline expects (all masked and
   protected): `GCP_WIF_CRED_JSON` (file), `GCP_PROJECT_ID`,
   `GCP_REGION`, `GCP_WIF_AUDIENCE`, `PRODUCTION_URL` (after the
   first deploy). See `deploy/cloudrun-deploy.sh` for the one-time
   WIF setup.
5. Push: the pipeline runs. Open an issue and watch the plan flow;
   open a merge request and assign the review flow; break a test on
   purpose and watch the fix flow; run the deploy job by hand and
   watch the release flow check production.
6. Update `.gitlab/duo/agent-config.yml` with the real production
   hostname once deployed, or the release flow will honestly report
   "not verified" instead of guessing.

## Honest limits (read before judging)

- **Demo deployment state is ephemeral.** The coordination service
  holds no database yet (PostgreSQL is on the roadmap), so a Cloud
  Run revision resets pairings. Judges and users pair fresh; the
  production VPS staging bundle (`deploy/`) keeps persistent state.
- **Duo flows need AI Catalog access.** The flows are defined and
  validated in this repo, but executing them needs the hackathon's
  provisioned group (or equivalent Duo access). On a plain free
  account the deterministic pipeline still runs end to end; the
  flows are applied the moment access exists.
- **Real-device run still pending.** Same honest gap as the main
  project: WebRTC media, screen capture, and input await physical
  hardware. Nothing in this sprint claims otherwise.
- **Agents advise; humans decide.** Every merge and every deploy in
  the demo history is a human step after reading the agent's
  verdict. That is the design, not a gap in it.
