# Testing the iCode eval pipeline (P0–P8 / E0–E8)

Per-stage CLI checks for Process 2 (DeepSWE + Pier + iCode vs DeepSeek V4 Pro baseline).  
Machine bootstrap first: [testing-binary-initializer.md](testing-binary-initializer.md) and [workflow.md](../workflow.md).  
**This lab (cloud root + this PC):** copy-paste phases and flowcharts in [cloud-eval-runbook.md](cloud-eval-runbook.md). **Users:** [user-guide.md](../user-guide.md).

**Sign-off:** DeepSWE **E0–E7** with `--n-tasks 1` (default Pier path). LoLBench is a **separate** job (`lolbench_one_task`); use the LoLBench track below, do not reuse Pier P4/P5 commands. E8 is optional N>1. Keep P0–P4 cheap (no LLM).

Scripts live under [`pipeline/stages/`](../../../pipeline/stages/). Helpers under [`pipeline/lib/`](../../../pipeline/lib/). The CLI wraps them:

```bash
export PATH="$HOME/.local/bin:$PATH"
export DEEPSEEK_MODEL=deepseek-v4-pro
mac-k3d eval --stage p0
mac-k3d eval --stage p5 --n-tasks 1 --model deepseek-v4-pro
mac-k3d eval --local --n-tasks 1    # full local pipeline after stages pass
mac-k3d eval                        # interactive → Jenkins job deepswe_one_task
```

Default workdir: `./eval-runs` (override with `MAC_K3D_EVAL_WORKDIR`).  
Default iCode for users: `ICODE_MODE=binary` and `~/.local/share/mac-k3d/icode` (or `/opt/mac-k3d/icode`). Developer source trees are discovered only when `ICODE_MODE=source`.  
Default model: `deepseek-v4-pro` (`DEEPSEEK_MODEL` or `--model`). API `model` in the response is stored as `llm_model_served` (routing to Flash is possible).

---

## Topology (cloud controller + this PC as worker)

Jobs must **not** run inside the controller’s k3d nodes. Jenkins on the cloud VM only queues; this PC (Docker + Pier + iCode) executes.

```text
Cloud VM: k3d + Jenkins :17070
    └── queue deepswe_one_task (label lolbench)
This PC: Jenkins agent + Docker + iCode
    ├── api.deepseek.com  (deepseek-v4-pro)
    └── archive JSON back to cloud Jenkins
```

Worker wizard default is `http://43.107.42.252:17070`. Type a new `http://<ip>:17070` or export `JENKINS_URL` for another controller. Never `mac-k3d start -c worker.yaml`.

After changing the `deepswe_one_task` job XML, refresh on the **cloud** controller:

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
| iCode tree on this PC | Discovered path (often `$HOME/Documents/iCode-main`) or a `-full-` tarball |
| `MAC_K3D_ROOT` on this PC | mac-k3d checkout (or Release share dir `~/.local/share/mac-k3d`) |
| API model string | `deepseek-v4-pro` (override with `DEEPSEEK_MODEL` if docs change) |
| N for smoke | **1**; larger N later (E8) |

Do not commit API keys. Do not commit leftover Harbor-named `launch-agent.sh` directories.

---

## E0–E8 tracking

Copy-paste commands. Use `--n-tasks 1` until E4–E6 are green.

| Check | Command | Expected | Pass (y/n) | Notes |
|-------|---------|----------|------------|-------|
| **E0** Cloud controller | On the **cloud** VM: Release binary, first-run wizard, role **CI controller**, Jenkins **17070**, Harbor skip, credential `deepseek-api-key`. Open security group **17070**. Then `JENKINS_URL=http://127.0.0.1:17070 ./scripts/env_set_up/02_check_controller.sh` | Browser `http://<cloud-ip>:17070` HTTP 200; job `deepswe_one_task` present | | Once per VM |
| **E1** This PC as worker | On this PC: `setup -c worker.yaml`. Enter keeps `http://43.107.42.252:17070`. User `admin`, API **secret**, distinct agent name. Never `start -c worker.yaml`. `JENKINS_URL=http://43.107.42.252:17070 ./pipeline/stages/e1_reach_jenkins.sh` then `REQUIRE_WORKER=1 JENKINS_URL=http://43.107.42.252:17070 ./scripts/env_set_up/03_check_worker.sh` | `e1`: HTTP 200; start rejected; unit active; node **online** in **cloud** Jenkins → Nodes | | |
| **E2** P0–P4 (no LLM) | See commands below | Existing P0–P4 OK lines; Pier agent `icode` present | | Fast |
| **E3** Report schema (no LLM) | `./pipeline/stages/check_report.sh pipeline/lib/testdata/report-min.json` | `OK report schema` | | Instant |
| **E4** P5 n=1 harness (paid) | `mac-k3d eval --stage p5 --n-tasks 1` with gitignored `.env` (or Jenkins bind `deepseek-api-key`) | `PROGRESS` P5; minutes of Pier/Docker/LLM; `harness/` artifacts. CLI usage errors fail the stage | | |
| **E5** P6 baseline | `DEEPSEEK_MODEL=deepseek-v4-pro mac-k3d eval --stage p6 --n-tasks 1` | `baseline/<task>/agent.patch`; `baseline/summary.json` includes usage/time/model | | |
| **E6** P7/P8 report fields | `mac-k3d eval --stage p7` then `--stage p8 --n-tasks 1`; then `./pipeline/stages/check_report.sh` | `eval-runs/reports/eval-icode-deepseek-deepswe-n1-<utc>.json` with `suite`, `pass_at_1`, `macro.f2p`/`p2p`/`reward`, `tokens`, `wall_seconds`/`wall_minutes`, `model` | | |
| **E7** Jenkins `deepswe_one_task` | From this PC: `mac-k3d eval --n-tasks 1 --icode-mode binary --yes` **without** `--local`, or UI Build with Parameters (`DEEPSEEK_MODEL=deepseek-v4-pro`, `AGENT_LABEL=lolbench`, `ICODE_MODE=binary`) | Build on **this** node; archived JSON; same schema | | |
| **E8** optional N>1 | Same as E6/E7 with `--n-tasks` > 1 | Same schema; `n_tasks` matches N | | After E6 green |

### E2 commands

```bash
export PATH="$HOME/.local/bin:$PATH"
export DEEPSEEK_MODEL=deepseek-v4-pro
mac-k3d eval --stage p0
mac-k3d eval --stage p1
mac-k3d eval --stage p2
mac-k3d eval --stage p3
mac-k3d eval --stage p4
```

Schema unit test (no network): `python3 pipeline/lib/test_report.py`

---

## LoLBench track (do not reuse DeepSWE P4/P5)

`lolbench_one_task` is Harbor + iCode, not Pier. Jenkins **SUCCESS** means `reward.json` exists (the pipeline ran). It does **not** mean `pass_at_1=1`. Match Harbor’s Resolved/Reward column: F2P 0.368 with Resolved 0.0 is a valid score.

Jenkins runs `~/.local/share/mac-k3d/pipeline` unless you set job param **`MAC_K3D_ROOT`** to this checkout. To test git changes: `MAC_K3D_ROOT=/home/Toby/Documents/Toby/mac-k3d` on the job, or `mac-k3d config -c ~/.config/mac-k3d/worker.yaml` to re-extract `pipeline/`.

| Check | Command | Expected | Pass (y/n) | Notes |
|-------|---------|----------|------------|-------|
| **L2** P0–P3 (no LLM) | `mac-k3d eval --benchmark lolbench --task ruff_1 --stage p0` then `p1` `p2` `p3` (not p4) | P0–P3 OK; `run_all` prints `P4 skipped: LoLBench uses Harbor` | | Fast |
| **L3** Schema | `./pipeline/stages/check_report.sh pipeline/lib/testdata/report-min.json` and `python3 pipeline/lib/test_report.py` | `OK report schema`; unit tests include nested Harbor `ruff_1` rates | | Instant |
| **L4** P5 n=1 Harbor (paid) | `mac-k3d eval --benchmark lolbench --task ruff_1 --icode-mode binary --stage p5` | `PROGRESS` P5; `harbor_runs/…/reward.json`; local image or docker build. Missing `reward.json` fails; reward `0.0` is a score | | |
| **L5** P6 | `mac-k3d eval --benchmark lolbench --task ruff_1 --stage p6` | `baseline/ruff_1/agent.patch`; summary usage/time/model | | |
| **L6** P7/P8 | `mac-k3d eval --benchmark lolbench --task ruff_1 --stage p7` then `--stage p8`; `./pipeline/stages/check_report.sh` | Same schema as DeepSWE (`suite=lolbench`); `f2p` / `p2p` rates from Harbor; `reward` from reward.json; `wall_minutes` | | |
| **L7** Jenkins `lolbench_one_task` | UI Build with Parameters (`TASK=ruff_1`, `ICODE_MODE=binary`, `DEEPSEEK_MODEL=deepseek-v4-pro`, `AGENT_LABEL=lolbench`) or `mac-k3d eval --benchmark lolbench --task ruff_1 --icode-mode binary --yes` **without** `--local` | Build on **this** node (not a k3d agent); P4 skipped; archived JSON; SUCCESS ≠ resolved | | |

L2 commands:

```bash
export PATH="$HOME/.local/bin:$PATH"
export DEEPSEEK_MODEL=deepseek-v4-pro
mac-k3d eval --benchmark lolbench --task ruff_1 --stage p0
mac-k3d eval --benchmark lolbench --task ruff_1 --stage p1
mac-k3d eval --benchmark lolbench --task ruff_1 --stage p2
mac-k3d eval --benchmark lolbench --task ruff_1 --icode-mode binary --stage p3
```

Do **not** run `--stage p4` for LoLBench.

---

## JSON report fields

Written by [`pipeline/lib/score_results.py`](../../../pipeline/lib/score_results.py) at P7/P8. Same schema for `deepswe_one_task` and `lolbench_one_task` (`suite` differs). Validate with [`pipeline/stages/check_report.sh`](../../../pipeline/stages/check_report.sh).

| Field | Meaning |
|-------|---------|
| `suite`, `harness`, `n_tasks`, `n_rollouts` | `deepswe` or `lolbench`; `icode`; N questions; **1** attempt per question |
| `model` / `model_served` / `api_base` | Requested id (`deepseek-v4-pro`), API-served id, endpoint |
| `access_date_utc` | When the run was recorded (UTC) |
| `wall_seconds` / `wall_minutes` | P5 harness + P6 baseline wall clock |
| `pass_at_1` | Mean of `c_t/n_t` (resolved attempts; n=1 → 0 or 1 per task) |
| `macro.f2p` / `p2p` / `partial` / `reward` | Mean per-task rates (`null` if the verifier has no value) |
| `tokens.in` / `out` / `total` | API prompt/completion (harness if known, else baseline). Missing → `null` |
| `tasks[].id` | Question dir name |
| `tasks[].c` / `n` / `pass_frac` / `first` | Resolved rollouts, attempts, `c/n`, first-rollout resolved |
| `tasks[].reward` | 1.0 resolved, 0.0 not (Harbor `reward.json`; Pier `resolved`) |
| `tasks[].f2p` / `p2p` | FAIL_TO_PASS / PASS_TO_PASS **rates** in `[0,1]`, never test-name lists |
| `tasks[].f2p_pass` / `f2p_total` | Optional counts when the verifier exposes them |
| `tasks[].tok_in` / `tok_out` / `dur_s` / `dur_min` | Harness API tokens and time for this question (`null` if unknown) |
| `tasks[].baseline` | Same question, DeepSeek without iCode (`reward`, tokens, `dur_s`/`dur_min`) |

---

## P0 — Docker, CLI, worker readiness

**Why:** Without Docker and an online agent (or local Docker for `--local`), Pier cannot run sandboxes.

```bash
mac-k3d eval --stage p0
# or: pipeline/stages/p0_prereqs.sh
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
mac-k3d eval --stage p3
```

**Expected:** unpacked/`uv run` `icode --help` succeeds.

## P4 — Pier sees agent `icode`

**Why:** Custom agent install + DeepSeek allowlist must register with Pier.

```bash
mac-k3d eval --stage p4
```

**Expected:** `pipeline/lib/icode_pier_agent.py` present; `pipeline/lib/pier-agent-icode` scripts executable; P4 prints `--agent-import-path icode_pier_agent:ICodeAgent`.

## P5 — One DeepSWE task through iCode (N=1)

**Why:** End-to-end harness path (expensive: image build + LLM).

```bash
# key from gitignored .env — do not export DEEPSEEK_API_KEY
mac-k3d eval --stage p5 --n-tasks 1
```

**Expected:** `PROGRESS` lines; Pier run for one task (**minutes of iCode**, not a ~20s `NonZeroAgentExitCodeError`). Pier uses the same iCode CLI as LoLBench Harbor: `icode -p … run -t <instruction> -C <repo> -a code --json`, plus `ICODE_API_BASE` / `ICODE_PROVIDER` / `ICODE_MODEL`. `harness/meta.json` (timing/model/tokens); patch under workdir. `No such option` / usage errors **fail** the stage. Jenkins `mac-k3d eval --yes` embeds `pipeline/` from the **installed binary** — rebuild (`cargo build --release` and copy to `~/.local/bin/mac-k3d`) after changing Pier `run.sh`.

## P6 — Same task via DeepSeek API only

**Why:** Baseline without iCode scaffolding.

```bash
mac-k3d eval --stage p6 --n-tasks 1
```

**Expected:** `instruction.md` posted to DeepSeek `deepseek-v4-pro`; `.patch` under `baseline/`; `summary.json` / `meta.json` include usage, wall time, and served model.

## P7 — Score f2p / p2p into temp JSON

**Why:** Compare harness vs baseline using verifier signals.

```bash
mac-k3d eval --stage p7
```

**Expected:** temp JSON with per-task `reward`, `f2p`/`p2p` rates, `pass_at_1`, tokens, `wall_minutes`, and `model` (rates may be `null` if the verifier only exposes resolved).

## P8 — Named file under `eval-runs/reports/`

**Why:** Stable identification of which eval ran.

```bash
mac-k3d eval --stage p8 --n-tasks 1
./pipeline/stages/check_report.sh
```

**Expected:** `eval-runs/reports/eval-icode-deepseek-deepswe-n1-<utc>.json` with the fields in **JSON report fields**. `check_report.sh` prints `OK report schema`.

---

## Full runner

After P0–P8 / E6:

```bash
mac-k3d eval --local --n-tasks 1 --model deepseek-v4-pro
# or trigger Jenkins on the cloud controller (runs on this worker):
mac-k3d eval --n-tasks 1 --icode-mode source --model deepseek-v4-pro
```

**Expected:** Jenkins `deepswe_one_task` (or local) runs both arms, prints progress, archives JSON that passes `check_report.sh`.

---

## Troubleshooting

| Message | What to do |
|---------|------------|
| pier not found | `uv tool install datacurve-pier` or `uv tool install git+https://github.com/datacurve-ai/pier` |
| DeepSWE clone fails | Network / git; retry P2 |
| DEEPSEEK_API_KEY missing | Copy `.env.example` → `.env` (chmod 600). Do not export the key. E7: store `deepseek-api-key` on the **cloud** controller ([secrets.md](../../secrets.md)) |
| No such option: --agent-dir | Pier 0.3.1 has no `--agent-dir`. Use this tree’s `--agent-import-path icode_pier_agent:ICodeAgent` |
| Docker OOM / disk | DeepSWE images are large; free disk; lower N |
| Worker offline | Finish E1; for local-only tests use `--local` |
| Job still uses `deepseek-chat` | Re-run `mac-k3d config --skip-secrets` on the **cloud** controller after pulling this tree |
| Worker points at localhost | For this lab set `controller_url: http://43.107.42.252:17070` (or export `JENKINS_URL` before setup); restart the agent unit if it was already running |
| No such option / docker compose unknown | P0 installs the Compose v2 user plugin; re-run `--stage p0`. P5 now fails on a 0-trial Pier job. |
