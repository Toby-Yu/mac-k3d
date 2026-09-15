# Environment setup checks (`scripts/env_set_up`)

Automated **verify + light smoke** scripts for the binary-initializer path. They download the Release `mac-k3d` binary and assert that this computer’s Jenkins **controller**, **worker**, and **eval-ready** state look healthy.

They do **not** replace interactive `mac-k3d setup` (wizard needs a TTY for role, secrets, and API token). Human bootstrap steps: [docs/binary-initializer/testing/clean-machine-binary-test.md](../../docs/binary-initializer/testing/clean-machine-binary-test.md).

Default Jenkins UI: **`http://localhost:17070`**.

## Quick start

```bash
# From the repo root (after you have run setup for controller + worker):
./scripts/env_set_up/run_all.sh

# Already have mac-k3d on PATH / ~/.local/bin — skip GitHub download:
SKIP_DOWNLOAD=1 ./scripts/env_set_up/run_all.sh

# Also run mac-k3d eval --stage p0:
SKIP_DOWNLOAD=1 RUN_EVAL_SMOKE=1 ./scripts/env_set_up/run_all.sh
```

Expected final line on success:

```text
OK ALL CHECKS PASSED
```

Any `FAIL` / `ERROR` → exit non-zero.

## Environment variables

| Variable | Default | Meaning |
|----------|---------|---------|
| `MAC_K3D_RELEASE_TAG` | `v0.4.0-rc.4` | GitHub Release tag to download |
| `MAC_K3D_REPO` | `Toby-Yu/mac-k3d` | GitHub `owner/repo` |
| `JENKINS_URL` | `http://localhost:17070` | Controller UI base URL |
| `CONTROLLER_CONFIG` | `~/.config/mac-k3d/config.yaml` | Controller YAML |
| `WORKER_CONFIG` | `~/.config/mac-k3d/worker.yaml` | Worker YAML |
| `INSTALL_DIR` | `~/.local/bin` | Where `01_download_binary.sh` installs `mac-k3d` |
| `MAC_K3D_BIN` | (auto) | Override path to the CLI |
| `SKIP_DOWNLOAD` | `0` | `1` = do not download; use existing binary |
| `REQUIRE_WORKER` | `1` in `run_all.sh` | `1` = fail if worker/agent missing; `0` = skip OK |
| `RUN_EVAL_SMOKE` | `0` | `1` = run `mac-k3d eval --stage p0` inside `04` |

---

## Scripts

### `01_download_binary.sh`

**Purpose:** Download the matching Release asset (`mac-k3d-linux-x86_64`, `linux-aarch64`, `darwin-aarch64`, or `darwin-x86_64`) for tag `MAC_K3D_RELEASE_TAG` and install it as `~/.local/bin/mac-k3d`.

**Expected output (success):**

```text
OK downloaded mac-k3d-linux-x86_64 → /home/…/.local/bin/mac-k3d
mac-k3d 0.4.0
OK help lists setup and eval
OK 01_download_binary complete
```

Uses `gh release download` when `gh` is available; otherwise `curl` to the GitHub download URL.

### `02_check_controller.sh`

**Purpose:** Confirm controller config exists, `mac-k3d status` shows Docker + k3d + Jenkins Running, Jenkins login returns HTTP 200, and jobs `lolbench_one_task` and `icode_eval` exist.

**Expected output (success):**

```text
OK mac-k3d=…
OK controller status healthy
OK Jenkins UI http://localhost:17070/login HTTP 200
OK job lolbench_one_task present
OK job icode_eval present
OK 02_check_controller complete
```

**Fail if:** no `config.yaml`, cluster/Jenkins down, wrong port, or jobs missing → finish `mac-k3d setup` / `start` / `config` for the controller first.

### `03_check_worker.sh`

**Purpose:** Confirm worker YAML exists (unless skipped), `mac-k3d start -c worker.yaml` is **rejected**, status reports worker role, and the agent daemon is active (Linux systemd user unit or macOS LaunchAgent).

**Expected output (success, one-PC both roles):**

```text
OK worker start rejected (exit 1)
OK worker status role=worker
OK Linux agent unit mac-k3d-jenkins-agent.service active
OK 03_check_worker complete
```

**Skip:** no `worker.yaml` and `REQUIRE_WORKER=0` → `OK 03_check_worker skipped`.

**Fail if:** agent not active and `REQUIRE_WORKER=1` → put `api_user` / `api_token` in `worker.yaml`, then `mac-k3d config -c ~/.config/mac-k3d/worker.yaml`.

### `04_check_eval_ready.sh`

**Purpose:** Confirm Docker has a Server section, CLI lists `eval`, Jenkins UI is up, and (when present) the local agent looks active. Optionally runs `mac-k3d eval --stage p0` when `RUN_EVAL_SMOKE=1`.

**Expected output (success):**

```text
OK docker Server section present
OK mac-k3d lists eval
OK Jenkins UI reachable for eval
OK eval-ready — safe to start an evaluation task …
OK 04_check_eval_ready complete
```

Does **not** prove DeepSeek credentials or full P5–P8. For those see [testing-eval-pipeline.md](../../docs/binary-initializer/testing/testing-eval-pipeline.md).

### `run_all.sh`

**Purpose:** Run `01` → `02` → `03` (`REQUIRE_WORKER` default 1) → `04` in order.

**Expected output (success):** ends with `OK ALL CHECKS PASSED`.

---

## Relationship to `pipeline/`

| Directory | Focus |
|-----------|--------|
| `scripts/env_set_up/` | Machine bootstrap: binary, controller, worker, eval-**ready** |
| `pipeline/stages/` | Eval pipeline stages P0–P8 (Pier, DeepSWE, iCode, scoring) |
| `pipeline/lib/` | Python adapter, baseline, scoring |

After env checks pass, continue with `mac-k3d eval --stage p0` or `pipeline/stages/run_all.sh`. Operator start: [docs/binary-initializer/user-guide.md](../../docs/binary-initializer/user-guide.md).
