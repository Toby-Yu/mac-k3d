# Integration log

## Weekend test queue

Token-spending manual tests deferred from finished steps. Run them on the newest commit, record that commit next to each build, then move the result into the step's row below. Builds that serve several steps are listed once.

| # | Proves | Jenkins job and parameters | What to check | Status |
|---|---|---|---|---|
| 1 | Pre-flight (all) | `deepswe_one_task`, `N_TASKS=1`, `N_ROLLOUTS=1`, `CPU_LOCK_QTY=1`, short task (for example `termenv-preserve-ansi-resets`) | Build green on the final commit before queueing the rest; one `agent/capture.json` in the trial | pending |
| 2 | P0.4 (+ P0.5 transcripts) | `lolbench_one_task`, `TASK=fastapi_1`, `N_TASKS=1`, `N_ROLLOUTS=4` | Every trial has `agent/capture.json`; no trial with `diff_stat.files > 0` has a 0-byte patch; no `capture_mismatch` / `capture_missing` in `agent/capture_flags.json` | pending |
| 3 | P0.4 (+ P0.5 transcripts) | `lolbench_one_task`, `TASK=ruff_1`, `N_TASKS=1`, `N_ROLLOUTS=4` | Same as #2; patch size vs the old ~2 MB, and `cleanup.removed` | pending |
| 4 | P0.4 | `deepswe_one_task`, `TASK=ofetch-per-origin-circuit-breaker`, `N_TASKS=1`, `N_ROLLOUTS=4` | Same as #2; any rollout with edits and an empty patch has a receipt that shows why | pending |
| 5 | P0.3 (+ P0.5 scan) | `lolbench_one_task`, `TASK=cpython_5`, `N_TASKS=1`, `N_ROLLOUTS=2` (or 3) | No transcript reads of `/opt/icode-host` + `tomllib`/`tomli`; record the scores (a drop from the contaminated 2 of 4 is the honest number) | pending |
| 6 | P0.3 | During any build above: `docker inspect <agent container> --format '{{json .Mounts}}'` | `"RW": false` on `/opt/icode-host` | pending |

Inspect receipts for a build (use `deepswe_one_task` in the path for DeepSWE):

```bash
B=<build>; R=~/jenkins-agent/workspace/lolbench_one_task/eval-runs/harness/harbor_runs/jenkins-$B
python3 - "$R" <<'PY'
import json, sys, pathlib
for agent in sorted(pathlib.Path(sys.argv[1]).glob("*/*/*/agent")):
    c, f = agent / "capture.json", agent / "capture_flags.json"
    r = json.loads(c.read_text()) if c.is_file() else {}
    flags = json.loads(f.read_text())["flags"] if f.is_file() else "no flags file"
    d = r.get("delivery", {})
    print(agent.parent.name, "receipt" if r else "NO RECEIPT",
          "bytes", r.get("patch", {}).get("bytes"), r.get("diff_stat"),
          "kept" if r.get("patch", {}).get("kept_existing") else "",
          "replaced" if d.get("replaced_submit_output") else "",
          "cleanup", r.get("cleanup", {}).get("removed"), flags, r.get("errors"))
PY
```

## Steps

| Date | Step | Branch @ commit | Automated tests | Manual test and evidence | Result | Notes / follow-ups |
|---|---|---|---|---|---|---|
| 2026-10-02 | P0.4 | int/P0.4-capture @ (this commit) | cargo test ✓ (102 unit + 16 cli); pipeline/lib unittest ✓ 115 (25 new in `test_icode_capture.py`: 6 card cases × deepswe/lolbench/swebenchpro, each replaying the suite's real collect command or `solution.patch` and applying to a fresh base clone); bash -n on changed scripts ✓; test_icode_input.sh ✓; test_p3_icode.sh ✓; check_docs.sh ✓; shellcheck not installed; docs/test-baseline-failures.txt empty | Gold through `PatchAgent` + capture (direct `harbor run`, this checkout's `pipeline/lib`): `fastapi_1` 1.0 (F2P 11/11, P2P 51/51), `ruff_1` 1.0 (19/19, 51/51), `abs-module-cache-flags` 1.0 (20/20, 3/3), `ofetch-per-origin-circuit-breaker` 1.0 (47/47, 13/13). Every receipt: grader patch sha256 = receipt sha256, `base_matches_declared` true, flags [], errors []; LoLBench `lolbench-submit` exit 0 with output identical to the standard patch (not replaced), uid 1000 `agent`; DeepSWE uid 0; no `safe.directory` complaint. Evidence: `output/evidence/P0.4-gold-runs/` (gitignored). | pass (gold); iCode rollouts in weekend queue #2–#4 | Capture moved to `pipeline/lib/icode_capture.sh` (one protocol for all suites, per-suite delivery). LoLBench submit now runs before the commit; its output is replaced by `git diff --binary <base>` when they differ. P7 `capture_receipt.py annotate` flags `capture_mismatch` / `capture_missing` / `patch_oversize`. SWE-bench Pro's generated collect (`git add -A`) keeps files untracked at base; correct for its shared-env verifier (`git clean -fd` then apply), but P7 would flag it. **Follow-up (mac-k3d, not iCode):** all 20 old DeepSWE P6 baseline trials graded a 0-byte `model.patch` (`PatchAgent` never committed; fixed here), and their inputs from `baseline_deepseek.py` are DeepSeek tool-call markup, not diffs; old LLM-only baseline scores are void. |
| 2026-10-02 | P0.3 | int/P0.3-mount-leak @ 9f9ecdc | cargo test ✓; pipeline/lib unittest ✓ 90; bash -n on changed scripts ✓; test_icode_input.sh ✓; test_p3_icode.sh ✓; check_docs.sh ✓; docs/test-baseline-failures.txt empty | Offline: dry-run deepswe+lolbench (RO mount, DEEPSEEK_API_KEY=***, no GITCODE_TOKEN); leakscan LoLBench 20/20 clean + DeepSWE 113/113 clean; zoneinfo `.pyc` only (no tomllib/test); ubuntu:24.04 amd64 `import openjiuwen_icode` via `.pth`. Jenkins: deepswe_one_task #47 SUCCESS (`termenv-preserve-ansi-resets`, flash, N=1×1, `MAC_K3D_ROOT`=this checkout); artifact `jenkins-47-20261002T062942Z` isolation sanitizer v2, sourceless true, `runtime_sha256=d0d488f5b17929b5…`, mount read_only true, leak hit_tasks []; reward.json present; no clone tokens in `.harbor-env`. Live `docker inspect` RW skipped (containers already removed). | pass (smoke) | **Deferred (weekend queue #5, #6):** card manual `cpython_5` × 2 (or × 3); live `docker inspect` `"RW": false` on agent env; transcript grep for `/opt/icode-host`+`tomllib`/`tomli` reads. Sourceless default on for all suites; `eval_protocol.isolation` records runtime fingerprint. Hub `cpython_5` image is arm64-only — use amd64 build/P5 or ubuntu for import checks. |
| 2026-10-02 | P0.2 | int/P0.2-provenance @ 35a2839 | cargo test ✓; pipeline/lib unittest ✓ 79; bash -n on pipeline/stages/*.sh and pipeline/lib/*.sh ✓; shellcheck not installed; docs/test-baseline-failures.txt empty | deepswe_one_task #43+#44 identical provenance (Harbor 0.22.0, DeepSWE 0b9fabbb…, same image id/digests); #45 DEEPSWE_REF=435ee89… appears in artifact; #46 ICODE_EXPECT_SHA=eea9d66… recorded; local P3 wrong ICODE_EXPECT_SHA → exit 1 | pass | Pin Harbor 0.22.0, DeepSWE/LoLBench SHAs, ICODE_EXPECT_SHA/OFFICIAL, provenance in eval_protocol/summary/report. VPN off required for GitCode during git-mode builds on this worker. |
| 2026-09-30 | P0.1 | int/P0.1-freeze @ 79f7165 | cargo test ✓ (102 unit + 16 cli, 0 failed); unittest ✓ 73, no failures; bash -n on pipeline/stages/*.sh and pipeline/lib/*.sh ✓; shellcheck not installed | deepswe_one_task #40: N_TASKS=1 N_ROLLOUTS=1 CPU_LOCK_QTY=1; SUCCESS; output/deepswe/jenkins-40-20261002T012401Z; no new `.cursor/debug-*.log` | pass | Tag `baseline-2026-09-30` is `5b93d7b` (tree frozen before this cleanup). Removed the hard-coded `.cursor` debug log from `eval_slots.py`, including the H3 and H4 call sites. |
