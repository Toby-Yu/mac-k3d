# Testing the iCode eval pipeline (P0–P8 / E0–E8)

Per-stage CLI checks for Process 2 (DeepSWE + Pier + iCode vs DeepSeek V4 Pro baseline).  
Machine bootstrap first: [testing-binary-initializer.md](testing-binary-initializer.md) and [workflow.md](workflow.md).  
**This lab (cloud root + this PC):** copy-paste phases and flowcharts in [cloud-eval-runbook.md](cloud-eval-runbook.md).

**Sign-off:** pass **E0–E7** with `--n-tasks 1` (Harbor/LoLBench stay skip). E8 is optional N>1. Keep P0–P4 cheap (no LLM).

Scripts live under [`scripts/eval/`](../../scripts/eval/). The CLI wraps them:

```bash
export PATH="$HOME/.local/bin:$PATH"
export DEEPSEEK_MODEL=deepseek-v4-pro
mac-k3d eval --stage p0
mac-k3d eval --stage p5 --n-tasks 1 --model deepseek-v4-pro
mac-k3d eval --local --n-tasks 1    # full local pipeline after stages pass
mac-k3d eval                        # interactive → Jenkins job icode_eval
```

Default workdir: `./eval-work` (override with `MAC_K3D_EVAL_WORKDIR`).  
Default iCode source: `/home/Toby/Documents/Toby/iCode-main` (override with `ICODE_SOURCE`).  
Default model: `deepseek-v4-pro` (`DEEPSEEK_MODEL` or `--model`). API `model` in the response is stored as `llm_model_served` (routing to Flash is possible).

---

## Topology (cloud controller + this PC as worker)

Jobs must **not** run inside the controller’s k3d nodes. Jenkins on the cloud VM only queues; this PC (Docker + Pier + iCode) executes.

```text
Cloud VM: k3d + Jenkins :17070
    └── queue icode_eval (label lolbench)
This PC: Jenkins agent + Docker + iCode
    ├── api.deepseek.com  (deepseek-v4-pro)
    └── archive JSON back to cloud Jenkins
```

If a local lab controller is still bound to `:17070`, teardown it or leave it unused. Worker wizard default / YAML is `http://43.107.42.252:17070`, not localhost. Never `mac-k3d start -c worker.yaml`.

After changing the `icode_eval` job XML, refresh on the **cloud** controller:

```bash
mac-k3d config -c ~/.config/mac-k3d/config.yaml --skip-secrets
```

---

## Required information (collect before E4)

| Item | Notes |
|------|--------|
| Cloud VM public IP or DNS | SSH access; ~8 GB+ RAM |
| Ports | **17070** (and optionally 18080) open to this PC |
| Jenkins admin password | After cloud `setup` |
| Jenkins API token | **Secret**, not the token *name*, for worker register |
| Credential `deepseek-api-key` | Stored on the **cloud** controller |
| iCode tree on this PC | Default `/home/Toby/Documents/Toby/iCode-main` or a `-full-` tarball |
| `MAC_K3D_ROOT` on this PC | mac-k3d checkout (or Release binary + `scripts/eval`) |
| API model string | `deepseek-v4-pro` (override with `DEEPSEEK_MODEL` if docs change) |
| N for smoke | **1**; larger N later (E8) |

Do not commit API keys. Do not commit leftover Harbor-named `launch-agent.sh` directories.

---

## E0–E8 tracking

Copy-paste commands. Use `--n-tasks 1` until E4–E6 are green.

| Check | Command | Expected | Pass (y/n) | Notes |
|-------|---------|----------|------------|-------|
| **E0** Cloud controller | On the **cloud** VM: Release binary, first-run wizard, role **CI controller**, Jenkins **17070**, Harbor skip, credential `deepseek-api-key`. Open security group **17070**. Then `JENKINS_URL=http://127.0.0.1:17070 ./scripts/env_set_up/02_check_controller.sh` | Browser `http://<cloud-ip>:17070` HTTP 200; job `icode_eval` present | | Once per VM |
| **E1** This PC as worker | On this PC: `setup -c worker.yaml`. Wizard default URL is `http://43.107.42.252:17070` (Enter). User `admin`, API **secret**, distinct agent name. Never `start -c worker.yaml`. `JENKINS_URL=http://43.107.42.252:17070 ./scripts/eval/e1_reach_jenkins.sh` then `REQUIRE_WORKER=1 JENKINS_URL=http://43.107.42.252:17070 ./scripts/env_set_up/03_check_worker.sh` | `e1`: HTTP 200; start rejected; unit active; node **online** in **cloud** Jenkins → Nodes | | |
| **E2** P0–P4 (no LLM) | See commands below | Existing P0–P4 OK lines; Pier agent `icode` present | | Fast |
| **E3** Report schema (no LLM) | `./scripts/eval/check_report.sh eval/testdata/report-min.json` | `OK report schema` | | Instant |
| **E4** P5 n=1 harness (paid) | `DEEPSEEK_API_KEY=… DEEPSEEK_MODEL=deepseek-v4-pro mac-k3d eval --stage p5 --n-tasks 1` (or Jenkins bind `deepseek-api-key`) | `PROGRESS` P5; pier log; `harness/` artifacts. Clear pier/docker errors still “stage ran” | | |
| **E5** P6 baseline | `DEEPSEEK_MODEL=deepseek-v4-pro mac-k3d eval --stage p6 --n-tasks 1` | `baseline/<task>/agent.patch`; `baseline/summary.json` includes usage/time/model | | |
| **E6** P7/P8 report fields | `mac-k3d eval --stage p7` then `--stage p8 --n-tasks 1`; then `./scripts/eval/check_report.sh` | `output/eval-icode-deepseek-deepswe-n1-<utc>.json` with f2p, p2p, `pass_at_1_*`, `token_usage`, `duration_seconds`, `access_date_utc`, `llm_model_id` / `llm_name` | | |
| **E7** Jenkins `icode_eval` | From cloud or this PC with controller URL: `mac-k3d eval --n-tasks 1 --icode-mode source --yes` **without** `--local`, or UI Build with Parameters (`DEEPSEEK_MODEL=deepseek-v4-pro`, `AGENT_LABEL=lolbench`) | Build on **this** node; archived JSON; same schema | | |
| **E8** optional N>1 | Same as E6/E7 with `--n-tasks` > 1 | Same schema; `n_tasks` matches N | | After E6 green |

### E2 commands

```bash
export PATH="$HOME/.local/bin:$PATH"
export DEEPSEEK_MODEL=deepseek-v4-pro
mac-k3d eval --stage p0
mac-k3d eval --stage p1
mac-k3d eval --stage p2
ICODE_MODE=source ICODE_SOURCE=/home/Toby/Documents/Toby/iCode-main mac-k3d eval --stage p3
mac-k3d eval --stage p4
```

Schema unit test (no network): `python3 eval/test_report.py`

---

## JSON report fields

Written by [`eval/score_results.py`](../../eval/score_results.py) at P7/P8. Validate with [`scripts/eval/check_report.sh`](../../scripts/eval/check_report.sh).

| Field | Meaning |
|-------|---------|
| `harness`, `benchmark`, `n_tasks` | `icode`, `deepswe`, N |
| `tasks[].id` / `f2p` / `p2p` / `harness_resolved` / `baseline_resolved` | Per-task verdicts (lists may be empty) |
| `access_date_utc` | When the LLM was accessed (UTC) |
| `llm_name` | Display name, e.g. DeepSeek V4 Pro |
| `llm_model_id` | Requested id (`deepseek-v4-pro`) |
| `llm_model_served` | `model` from the API body (may differ if routed) |
| `llm_version` | Optional header/body version |
| `duration_seconds` | `{harness, baseline, total}` |
| `token_usage` | `{prompt, completion, total}` plus `harness` / `baseline` breakdown; per-task when available |
| `totals.pass_at_1_harness` / `pass_at_1_baseline` | `resolved_true / n` (single attempt = pass@1) |

---

## P0 — Docker, CLI, worker readiness

**Why:** Without Docker and an online agent (or local Docker for `--local`), Pier cannot run sandboxes.

```bash
mac-k3d eval --stage p0
# or: scripts/eval/p0_prereqs.sh
```

**Expected:** `docker info` shows Server; `mac-k3d --help` lists `eval`; optional note if Jenkins worker is offline (OK for `--local`).

## P1 — Pier on PATH

**Why:** DeepSWE is Pier/Harbor-format; Pier builds task images and runs agents.

```bash
mac-k3d eval --stage p1
```

**Expected:** installs via `uv tool install datacurve-pier` if missing; `pier --help` works.

## P2 — DeepSWE clone

**Why:** Tasks (Dockerfile, instruction, tests) are not in this git repo.

```bash
mac-k3d eval --stage p2
```

**Expected:** `$WORKDIR/deep-swe/tasks` exists after shallow clone of `https://github.com/datacurve-ai/deep-swe`.

## P3 — iCode binary or source

**Why:** Arm A must invoke the harness under test.

```bash
# binary mode
ICODE_MODE=binary ICODE_RELEASE=/path/to/icode-*-full-*.tar.gz mac-k3d eval --stage p3
# source mode (default path)
ICODE_MODE=source ICODE_SOURCE=/home/Toby/Documents/Toby/iCode-main mac-k3d eval --stage p3
```

**Expected:** unpacked/`uv run` `icode --help` succeeds.

## P4 — Pier sees agent `icode`

**Why:** Custom agent install + DeepSeek allowlist must register with Pier.

```bash
mac-k3d eval --stage p4
```

**Expected:** agent package under `eval/pier-agent-icode` is discoverable; install script is executable.

## P5 — One DeepSWE task through iCode (N=1)

**Why:** End-to-end harness path (expensive: image build + LLM).

```bash
export DEEPSEEK_API_KEY=…   # or rely on Jenkins credential when not --local
export DEEPSEEK_MODEL=deepseek-v4-pro
mac-k3d eval --stage p5 --n-tasks 1
```

**Expected:** `PROGRESS` lines; Pier run for one task; `harness/meta.json` (timing/model); patch or trajectory under workdir. Failures with clear pier/docker errors still count as “stage ran”.

## P6 — Same task via DeepSeek API only

**Why:** Baseline without iCode scaffolding.

```bash
DEEPSEEK_MODEL=deepseek-v4-pro mac-k3d eval --stage p6 --n-tasks 1
```

**Expected:** `instruction.md` posted to DeepSeek `deepseek-v4-pro`; `.patch` under `baseline/`; `summary.json` / `meta.json` include usage, wall time, and served model.

## P7 — Score f2p / p2p into temp JSON

**Why:** Compare harness vs baseline using verifier signals.

```bash
mac-k3d eval --stage p7
```

**Expected:** temp JSON with per-task `harness_resolved`, `baseline_resolved`, `f2p`, `p2p`, `pass_at_1_*`, tokens, timing, and model fields (lists may be empty if the task only exposes a single verdict).

## P8 — Named file under `output/`

**Why:** Stable identification of which eval ran.

```bash
mac-k3d eval --stage p8 --n-tasks 1
./scripts/eval/check_report.sh
```

**Expected:** `output/eval-icode-deepseek-deepswe-n1-<utc>.json` (or under `$WORKDIR/output/`) with the fields in **JSON report fields**. `check_report.sh` prints `OK report schema`.

---

## Full runner

After P0–P8 / E6:

```bash
mac-k3d eval --local --n-tasks 1 --model deepseek-v4-pro
# or trigger Jenkins on the cloud controller (runs on this worker):
mac-k3d eval --n-tasks 1 --icode-mode source --model deepseek-v4-pro
```

**Expected:** Jenkins `icode_eval` (or local) runs both arms, prints progress, archives JSON that passes `check_report.sh`.

---

## Troubleshooting

| Message | What to do |
|---------|------------|
| pier not found | `uv tool install datacurve-pier` or `uv tool install git+https://github.com/datacurve-ai/pier` |
| DeepSWE clone fails | Network / git; retry P2 |
| DEEPSEEK_API_KEY missing | Set env for `--local`, or store `deepseek-api-key` on the **cloud** controller ([secrets.md](../secrets.md)) |
| Docker OOM / disk | DeepSWE images are large; free disk; lower N |
| Worker offline | Finish E1; for local-only tests use `--local` |
| Job still uses `deepseek-chat` | Re-run `mac-k3d config --skip-secrets` on the **cloud** controller after pulling this tree |
| Worker points at localhost | Set `controller_url: http://43.107.42.252:17070` (wizard default) in worker YAML; restart the agent unit if it was already running |
