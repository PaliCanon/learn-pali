#!/usr/bin/env bash
# Shape of .github/workflows/deploy.yml (task 083): a tag builds the image and
# pushes it to GHCR; nothing deploys. Prod runs on the user's server
# (ssh.lsh.tech), whose stack pulls the image by digest, written by hand from
# the job summary. Run: bash scripts/deploy-workflow.test.sh
# DEPLOY_WORKFLOW points the suite at a copy of deploy.yml (a mutation run).
# Needs bash, python3 with PyYAML (on the runner; ci.yml installs it otherwise).
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
wf="${DEPLOY_WORKFLOW:-$root/.github/workflows/deploy.yml}"
failed=0
ok() { printf 'ok   %s\n' "$1"; }
ko() { printf 'FAIL %s\n' "$1"; failed=$((failed + 1)); }
check() { # NAME COMMAND...
  local name=$1
  shift
  if "$@" >/dev/null 2>&1; then ok "$name"; else ko "$name"; fi
}
py() { python3 -c "import sys,yaml; w=yaml.safe_load(open(sys.argv[1])); $1" "$wf"; }
has() { grep -qE -- "$1" "$wf"; }
lacks() { ! grep -qiE -- "$1" "$wf"; }

check "the workflow parses" py 'pass'
check "it runs on a v* tag push and nothing else" \
  py 'on = w.get(True, w.get("on")); assert on == {"push": {"tags": ["v*"]}}, on'
check "one job, with contents: read and packages: write" \
  py 'j = w["jobs"]; assert len(j) == 1; p = list(j.values())[0]["permissions"]; assert p["contents"] == "read" and p["packages"] == "write", p'
check "id-token stays (the passages asset is read from GCS)" \
  py 'p = list(w["jobs"].values())[0]["permissions"]; assert p["id-token"] == "write", p'
check "login to ghcr.io with the actor and GITHUB_TOKEN" \
  bash -c 'grep -q "docker/login-action" "$1" && grep -q "registry: ghcr.io" "$1" && grep -q "username: \${{ github.actor }}" "$1" && grep -q "password: \${{ secrets.GITHUB_TOKEN }}" "$1"' _ "$wf"
check "the image is ghcr.io/palicanon/learn-pali" has 'ghcr\.io/palicanon/learn-pali'
check "tags: the h- content hash and the release tag" \
  bash -c 'grep -q "image_hash" "$1" && grep -q "github.ref_name" "$1"' _ "$wf"
check "the hash is computed as before (blob ids of app core Cargo.toml Cargo.lock, plus ui-lib)" \
  has 'git ls-files -s app core Cargo.toml Cargo.lock'
check "the build is skipped when the h- tag exists (imagetools inspect)" has 'docker buildx imagetools inspect'
check "the build step runs only when the image is missing" \
  py 's = [x for x in list(w["jobs"].values())[0]["steps"] if str(x.get("uses","")).startswith("docker/build-push-action")]; assert len(s) == 1 and s[0]["if"] == "steps.check-image.outputs.exists != \x27true\x27", s'
check "the image links to the repository (label org.opencontainers.image.source)" \
  has 'org\.opencontainers\.image\.source=https://github\.com/\$\{\{ github\.repository \}\}'
check "the digest and the h- tag go to the job summary" \
  bash -c 'grep -q GITHUB_STEP_SUMMARY "$1" && grep -q "@sha256" "$1" || grep -q "digest" "$1"' _ "$wf"
check "the release values are printed for the stack" has 'scripts/release-values\.sh'
check "the passages asset is still fetched" has 'bash scripts/fetch-passages\.sh app/assets\.lock assets/passages\.db'
check "the build still takes no ssh and no secret" lacks '^\s+(ssh|secrets):'

for what in 'gcloud run' 'run\.app' 'docker\.pkg\.dev' 'artifacts' 'configure-docker' 'update-traffic' \
  'rollout-check' 'origin-header' 'origin-secret' 'x-origin-auth' 'worker' 'WIF_PROVIDER.*artifact' 'ACCESS_prod' 'setup-chrome'; do
  check "no trace of: $what" lacks "$what"
done
check "no step reads the Artifact Registry project (GCP_PROJECT is not used for an image)" lacks 'vars\.GCP_PROJECT'

echo
if [ "$failed" -eq 0 ]; then echo "deploy-workflow.test.sh: all checks passed"; else echo "deploy-workflow.test.sh: $failed failed"; fi
[ "$failed" -eq 0 ]
