#!/usr/bin/env bash
# Fail if docs/ layout drifts from the product vs lab vs branch-folder standard.
# Product pages stay at docs/*.md (allowlist). Lab checklists stay in docs/testing/.
# A git branch that is not product-wide gets docs/<name>/README.md + testing.md only.
# Install hook: ln -sf ../../scripts/git-hooks/pre-commit .git/hooks/pre-commit
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail=0
note() { echo "ERROR: $*" >&2; fail=1; }

# Top-level markdown that is current product, developer reference, or historical.
# New product pages: add the filename here. Do not invent a new folder.
PRODUCT_MD=(
  README.md
  architecture.md
  commands.md
  configuration.md
  deployment.md
  export-import.md
  icode-harness-inputs.md
  initializer-new-machine.md
  lolbench-jenkins.md
  new-machine.md
  prepare-wizard.md
  secrets.md
  setup.md
  testing-initializer.md
  user-guide.md
  workflow.md
)

for f in "${PRODUCT_MD[@]}"; do
  [ -f "docs/$f" ] || note "missing product doc docs/$f"
done

# Index must group by audience (stops a flat dump of every new file).
for heading in \
  "## Users" \
  "## Lab runbooks" \
  "## Developers" \
  "## Historical" \
  "## Branch process"
do
  grep -q "^${heading}" docs/README.md || note "docs/README.md missing heading: ${heading}"
done

grep -q 'user-guide.md' README.md || note "repo README.md must link docs/user-guide.md"
grep -q 'testing/cloud-eval-runbook.md' docs/user-guide.md \
  || note "docs/user-guide.md must point lab IPs at testing/cloud-eval-runbook.md"

if grep -q '43\.107\.42\.252' docs/user-guide.md; then
  note "docs/user-guide.md must not contain the lab IP 43.107.42.252 (use CONTROLLER_IP; lab IPs belong in docs/testing/)"
fi

# No nested testing/ under a branch folder (that was the old binary-initializer layout).
if [ -d docs/binary-initializer/testing ]; then
  note "docs/binary-initializer/testing/ must not exist; lab checklists live in docs/testing/"
fi

shopt -s nullglob
for dir in docs/*/; do
  name="$(basename "$dir")"
  case "$name" in
    testing | examples) continue ;;
  esac
  if [ -d "${dir}testing" ]; then
    note "do not nest ${dir}testing/; put lab checklists in docs/testing/ and branch notes in ${dir}testing.md"
  fi
  for extra in "$dir"*; do
    [ -e "$extra" ] || continue
    base="$(basename "$extra")"
    case "$base" in
      README.md | testing.md) ;;
      *)
        note "branch folder docs/${name}/ may only contain README.md and testing.md (found ${base}). Product docs go in docs/; lab runbooks in docs/testing/"
        ;;
    esac
  done
done

# Unknown markdown at docs/*.md
allow=""
for f in "${PRODUCT_MD[@]}"; do
  allow="${allow}|${f}"
done
allow="${allow#|}"
for f in docs/*.md; do
  base="$(basename "$f")"
  echo "$base" | grep -Eq "^($allow)$" || note "unknown docs/${base}: add it to PRODUCT_MD in scripts/check_docs.sh (product) or move to docs/<branch>/README.md (branch-only) or docs/testing/ (lab runbook)"
done

# Relative .md links must exist (skip http(s), mailto, anchors).
python3 - <<'PY' || fail=1
import os, re, sys
root = os.getcwd()
docs = os.path.join(root, "docs")
fail = 0
link_re = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
for dirpath, _, files in os.walk(docs):
    for name in files:
        if not name.endswith(".md"):
            continue
        path = os.path.join(dirpath, name)
        rel = os.path.relpath(path, root)
        try:
            text = open(path, encoding="utf-8").read()
        except OSError as e:
            print(f"ERROR: cannot read {rel}: {e}", file=sys.stderr)
            fail = 1
            continue
        for raw in link_re.findall(text):
            target = raw.strip()
            if not target or target.startswith(("#", "http://", "https://", "mailto:", "ftp://")):
                continue
            target = target.split("#", 1)[0].split("?", 1)[0]
            if not target.endswith(".md"):
                continue
            dest = os.path.normpath(os.path.join(dirpath, target))
            if not os.path.isfile(dest):
                print(f"ERROR: broken markdown link in {rel}: {raw}", file=sys.stderr)
                fail = 1
sys.exit(fail)
PY

if [ "$fail" -ne 0 ]; then
  echo "docs layout check failed. See scripts/check_docs.sh and docs/README.md." >&2
  exit 1
fi
echo "OK docs layout"
exit 0
