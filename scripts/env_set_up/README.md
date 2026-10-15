# Environment setup checks (`scripts/env_set_up`)

Automated **verify + light smoke** scripts for the binary-initializer path. They download the Release `mac-k3d` binary and assert that this computer’s Jenkins **controller**, **worker**, and **eval-ready** state look healthy, on Linux and on macOS. The eval-ready checks cover what DeepSWE with iCode or JiuwenSwarm needs.

They do **not** replace interactive `mac-k3d setup` (wizard needs a TTY for role, secrets, and API token). Human bootstrap steps: [docs/testing/clean-machine-binary-test.md](../../docs/testing/clean-machine-binary-test.md). A Mac worker end to end: [docs/testing/mac-mini-worker.md](../../docs/testing/mac-mini-worker.md).

Default Jenkins UI: **`http://localhost:17070`**.

## Quick start

```bash
# From the repo root (after you have run setup for controller + worker):
./scripts/env_set_up/run_all.sh

# Already have mac-k3d on PATH / ~/.local/bin — skip GitHub download:
SKIP_DOWNLOAD=1 ./scripts/env_set_up/run_all.sh

# Also check the DeepSWE jobs, the plugin, the credentials and the node:
JENKINS_USER=admin JENKINS_API_TOKEN=… SKIP_DOWNLOAD=1 ./scripts/env_set_up/run_all.sh

# Also run mac-k3d eval --stage env (the pipeline's bare-metal phase) and
# build both harnesses for each task platform this worker runs:
SKIP_DOWNLOAD=1 RUN_EVAL_SMOKE=1 RUN_HARNESS_SMOKE=1 ./scripts/env_set_up/run_all.sh
```

Expected final line on success:

```text
OK ALL CHECKS PASSED
```

Any `FAIL` / `ERROR` → exit non-zero. A `NOTE` line is advice; it does not fail the run.

## Environment variables

| Variable | Default | Meaning |
|----------|---------|---------|
| `MAC_K3D_RELEASE_TAG` | `v<version in Cargo.toml>` | GitHub Release tag to download, this checkout's version so the binary matches the scripts and the pipeline |
| `MAC_K3D_REPO` | `Toby-Yu/mac-k3d` | GitHub `owner/repo` |
| `JENKINS_URL` | `http://localhost:17070`; on a worker-only machine (no `config.yaml`) `jenkins_agent.controller_url` from `worker.yaml` | Controller UI base URL |
| `JENKINS_USER`, `JENKINS_API_TOKEN` | unset | A Jenkins user and an API token of that user (Jenkins > your name > Security > API Token). With both, `02` and `03` also read job configs, plugins, credentials and the node; without them those checks are skipped with a `NOTE`. The token reaches `curl` on stdin, never on its command line |
| `CONTROLLER_CONFIG` | `~/.config/mac-k3d/config.yaml` | Controller YAML |
| `WORKER_CONFIG` | `~/.config/mac-k3d/worker.yaml` | Worker YAML |
| `INSTALL_DIR` | `~/.local/bin` | Where `01_download_binary.sh` installs `mac-k3d` |
| `MAC_K3D_BIN` | (auto) | Override path to the CLI |
| `MAC_K3D_SHARE` | `~/.local/share/mac-k3d` | The worker's share (harness store, env-check store), as the pipeline reads it |
| `SKIP_DOWNLOAD` | `0` | `1` = do not download; use existing binary |
| `REQUIRE_WORKER` | `1` in `run_all.sh` | `1` = fail if worker/agent missing; `0` = skip OK |
| `RUN_EVAL_SMOKE` | `0` | `1` = run `mac-k3d eval --stage env` inside `04` |
| `RUN_HARNESS_SMOKE` | `0` | `1` = run `06` (real harness builds) |
| `HARNESSES` | `icode jiuwenswarm` | Harnesses `06` prepares |
| `PLATFORMS` | this host's; on arm64 also `linux/amd64` | Task platforms `06` prepares for |

---

## Scripts

### `01_download_binary.sh`

**Purpose:** Download the matching Release asset (`mac-k3d-linux-x86_64`, `linux-aarch64`, `darwin-aarch64`, or `darwin-x86_64`) for tag `MAC_K3D_RELEASE_TAG` and install it as `~/.local/bin/mac-k3d`. Then check that the binary has what DeepSWE with iCode or JiuwenSwarm needs: `eval --harness` and `eval prepare`.

**Expected output (success):**

```text
OK downloaded mac-k3d-darwin-aarch64 → /Users/…/.local/bin/mac-k3d
mac-k3d 0.5.4
OK help lists setup and eval
OK eval lists --harness and prepare
OK 01_download_binary complete
```

Uses `gh release download` when `gh` is available; otherwise `curl` to the GitHub download URL.

**Fail if:** the tag has no asset for this OS and CPU, or the binary predates `eval --harness` (v0.5.3 and older). Build this checkout instead: `cargo build --release`, then `SKIP_DOWNLOAD=1 MAC_K3D_BIN=$PWD/target/release/mac-k3d`.

### `02_check_controller.sh`

**Purpose:** On the controller machine, `mac-k3d status` shows Docker + k3d + Jenkins Running. On a worker-only machine (a Mac beside a Linux controller) that part is skipped with a `NOTE` and the rest checks the controller at `JENKINS_URL`. Then Jenkins login returns HTTP 200, and the nine jobs (`deepswe`, `lolbench`, `swebenchpro` × `one_task`, `some_task`, `full_suite_task`) exist while the retired `build_jiuwenswarm_headless` and `eval_aggregate` do not. With `JENKINS_USER` / `JENKINS_API_TOKEN`: every DeepSWE job offers `HARNESS` `icode` and `jiuwenswarm`, the Active Choices plugin (`uno-choice`) is active, and the credential `deepseek-api-key` exists (`gitcode-pat` is a `NOTE` when missing).

**Expected output (success):**

```text
OK mac-k3d=…
OK controller status healthy
OK Jenkins UI http://localhost:17070/login HTTP 200
OK job deepswe_one_task present
…
OK no retired jobs (build_jiuwenswarm_headless, eval_aggregate)
OK deepswe_one_task HARNESS: icode jiuwenswarm
…
OK Active Choices plugin (uno-choice) active
OK credential deepseek-api-key present
OK 02_check_controller complete
```

A controller that hides itself from anonymous users answers 403 for every job, real or not, so without `JENKINS_USER` / `JENKINS_API_TOKEN` the job checks end in a `NOTE` instead of `OK job … present`.

**Fail if:** neither `config.yaml` nor `worker.yaml`, cluster/Jenkins down, wrong port, jobs missing or stale → `mac-k3d config -c ~/.config/mac-k3d/config.yaml` on the controller.

### `03_check_worker.sh`

**Purpose:** Confirm worker YAML exists (unless skipped), `mac-k3d start -c worker.yaml` is **rejected**, status reports worker role, the agent's Java is new enough, and the agent daemon runs: the Linux systemd user unit, or on macOS the LaunchAgent `com.mac-k3d.jenkins-agent` in state `running` (`launchctl print`; loaded is not enough). On macOS it also notes when automatic login is off or the Mac sleeps when idle. With `JENKINS_USER` / `JENKINS_API_TOKEN`: the controller sees node `jenkins_agent.name` online with the `fits-deepswe` label.

**Expected output (success, a Mac worker):**

```text
OK worker start rejected (exit 1)
OK worker status role=worker
OK agent java 21 at /opt/homebrew/opt/openjdk@21/bin/java (need 21+)
OK macOS LaunchAgent com.mac-k3d.jenkins-agent running
OK automatic login on
OK the Mac does not sleep when idle
OK Jenkins node mac-mini online with fits-deepswe
OK 03_check_worker complete
```

On Linux the agent line is `OK Linux agent unit mac-k3d-jenkins-agent.service active`.

**Skip:** no `worker.yaml` and `REQUIRE_WORKER=0` → `OK 03_check_worker skipped`.

**Fail if:** agent not active and `REQUIRE_WORKER=1` → put `api_user` / `api_token` in `worker.yaml`, then `mac-k3d config -c ~/.config/mac-k3d/worker.yaml`. A LaunchAgent that is loaded but not running leaves its reason in `jenkins-agent.stderr.log` under `jenkins_agent.remote_fs`.

### `04_check_eval_ready.sh`

**Purpose:** Confirm the host and Docker can run a DeepSWE evaluation with iCode or JiuwenSwarm:

- `bash` on PATH is at least `BASH_MIN` (Homebrew bash on a Mac), `python3` at least `PYTHON_MIN` with PyYAML, `curl` and `git` present.
- Docker has a Server section, `docker compose` and `buildx` are at the pinned versions (the pipeline's `env/compose` step), and Docker has at least `MIN_RAM_GB` of memory. On a Mac that is Docker Desktop's VM, which reports a little less than its Memory Limit.
- On an arm64 host (Apple silicon), Docker runs `linux/amd64` containers (Rosetta in Docker Desktop, binfmt on Linux): every DeepSWE question without a proven native build runs the published amd64 image.
- The harness build image (`ICODE_LINUX_BUILD_IMAGE`) runs, with `make`, `uv` and `git`, for each platform the harness is built for.
- Docker mounts `MAC_K3D_SHARE` and `jenkins_agent.remote_fs` (Docker Desktop's File sharing list on a Mac).
- On an arm64 host, how many DeepSWE questions `mac-k3d eval prepare` has proven to run natively (a `NOTE` when none).
- The CLI lists `eval`, Jenkins UI is up, and (when present) the local agent runs.

Optionally runs `mac-k3d eval --stage env` when `RUN_EVAL_SMOKE=1`.

**Expected output (success, a Mac):**

```text
OK bash 5.2 at /opt/homebrew/bin/bash
OK python3 3.13 at /opt/homebrew/bin/python3
OK python3 has PyYAML
OK curl and git on PATH
OK docker Server section present
OK docker compose and buildx present
OK Docker memory 11 GB (min 8)
OK Docker runs linux/amd64 containers (emulated)
OK harness build image runs for linux/amd64 (ghcr.io/astral-sh/uv:…)
OK harness build image runs for linux/arm64 (ghcr.io/astral-sh/uv:…)
OK Docker mounts /Users/…/.local/share/mac-k3d
OK Docker mounts /Users/…/jenkins-agent
OK mac-k3d lists eval
NOTE no DeepSWE question has a proven native build: …
OK Jenkins UI reachable for eval
OK macOS agent LaunchAgent running
OK eval-ready — safe to start an evaluation task …
OK 04_check_eval_ready complete
```

Does **not** prove DeepSeek credentials or the evaluate phase onwards. For those see [testing-eval-pipeline.md](../../docs/testing/testing-eval-pipeline.md).

### `05_check_export_import.sh`

**Purpose:** File-only `mac-k3d export` / `import` on this machine’s live `worker.yaml` or `config.yaml`. Writes scratch files under `/tmp`. Never `--force` onto `~/.config/mac-k3d/`. Does **not** queue Jenkins or call the LLM.

Needs a binary that has `export` (GitHub v0.5.3 or this checkout’s `target/release/mac-k3d`).

```bash
MAC_K3D_BIN="$PWD/target/release/mac-k3d" ./scripts/env_set_up/05_check_export_import.sh
```

**Expected output (success):** `OK 05_check_export_import complete`; the scratch YAML has no `api_token` value (a worker file keeps the empty `api_token: ''` slot).

Not part of `run_all.sh` (that suite is Release-binary health). DeepSWE TASK change + Jenkins: [docs/export-import.md](../../docs/export-import.md). `./scripts/list_deepswe_tasks.sh` prints sorted DeepSWE ids.

### `06_check_harness_prepare.sh`

**Purpose (opt-in, `RUN_HARNESS_SMOKE=1`):** Prepare each harness in `HARNESSES` for each platform in `PLATFORMS` the way a dispatcher's Prepare harness stage does (`pipeline/tools/prepare_harness.sh`), then check the bundle's platform. These are real builds: they need the network, and a PAT in `.env` or the environment for a private repo. The harness inputs come from the environment the pipeline reads (`ICODE_MODE`, `ICODE_GIT_URL`, `ICODE_GIT_REF`, `ICODE_RELEASE`, `JIUWENSWARM_MODE`, `JIUWENSWARM_GIT_URL`, `JIUWENSWARM_GIT_REF`, `JIUWENSWARM_RELEASE`, …). Each build runs in its own scratch workspace; the bundles stay in this worker's harness store, so the first eval reuses them.

iCode defaults to its release (`ICODE_MODE=binary`). On an arm64 worker, `linux/arm64` needs an `icode-linux-aarch64` drop, so a worker that keeps only the x86-64 drop stops with `the iCode release is built for x86_64, but this build's task containers are linux/arm64`. Give `ICODE_RELEASE=<a .tar.gz with both drops>`, or a git input (`ICODE_MODE=git ICODE_GIT_URL=… ICODE_GIT_REF=…`), or `PLATFORMS=linux/amd64`. An eval with that x86-64-only release runs DeepSWE emulated instead of stopping.

On Apple silicon this builds every harness twice: natively for `linux/arm64` and under Rosetta for `linux/amd64`. JiuwenSwarm builds in the container's own filesystem (a tmpfs of about 1.5 GB in Docker's memory), so give Docker Desktop its full Memory Limit while it runs.

`PLATFORMS=linux/arm64` on an x86-64 Linux host builds under QEMU (binfmt), so uv's git fetches run emulated inside the arm64 build image. On a lossy link to gitcode, a large fetch breaks mid-pack (`error: … bytes of body are still expected`, `fetch-pack: invalid index-pack output`); iCode's `agent-core` dependency failed that way on every emulated try on the lab's Linux PC, while the same fetch passed natively. A Mac builds `linux/arm64` natively.

**Expected output (success):**

```text
prepare icode for linux/arm64 (log /tmp/mac-k3d-harness-smoke.…/icode-arm64.log) …
iCode under test: git pr 2 -> 1a2b3c… (…)
OK icode prepared for linux/arm64: bundle 9f8e7d6c5b4a3921 (linux/arm64)
…
OK 06_check_harness_prepare complete
```

Without `RUN_HARNESS_SMOKE=1`: `OK 06_check_harness_prepare skipped`. On a failure the script prints the last 30 lines and keeps the full log.

### `run_all.sh`

**Purpose:** Run `01` → `02` → `03` (`REQUIRE_WORKER` default 1) → `04` → `06` in order.

**Expected output (success):** ends with `OK ALL CHECKS PASSED` and `Next: mac-k3d eval --stage env`.

---

## Relationship to `pipeline/`

| Directory | Focus |
|-----------|--------|
| `scripts/env_set_up/` | Machine bootstrap: binary, controller, worker, eval-**ready** |
| `pipeline/stages/` | Eval pipeline: seven phases (`env`, `tasks`, `evaluate`, `anticheat`, `score`, `report`, `archive`) and their steps — [docs/pipeline.md](../../docs/pipeline.md) |
| `pipeline/lib/` | Harbor agents, isolation and anti-cheat checks, scoring, report, archive |

After env checks pass, continue with `mac-k3d eval --stage env` or `pipeline/stages/run_all.sh`. Operator start: [docs/user-guide.md](../../docs/user-guide.md).
