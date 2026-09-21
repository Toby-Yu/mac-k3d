# User guide: cloud controller, worker, and eval

Copy-paste these blocks. Every command has a `#` comment. This page uses placeholders (`CONTROLLER_IP`). This lab’s real IP and SSH: [testing/cloud-eval-runbook.md](testing/cloud-eval-runbook.md).

Two different downloads:

- **mac-k3d CLI** — GitHub asset `mac-k3d-linux-x86_64` (or `linux-aarch64` / `darwin-aarch64` / `darwin-x86_64`). Used in steps 1–2.
- **iCode release** — official `icode-<os>-<arch>-full-vX.Y.Z` from the iCode project. Jenkins **uploads** it on **Build with Parameters**. Local `--stage` / `--local` still uses a worker path or `~/.local/share/mac-k3d/` discover. Not the same file as mac-k3d.

Prefer **v0.5.2** Latest (or this checkout’s `cargo build --release`). **v0.5.1** DeepSWE Pier used the wrong iCode CLI and often exited in ~20s. **v0.5.0** still has the old P3: it only accepts a file named `icode` or a `*.tar.gz` / `*.tgz`. An extensionless `icode-…-full-…` download needs v0.5.1+.

Harbor CLI stays **skip** on DeepSWE-only workers. Eval jobs are two runners, both iCode + DeepSeek catalog model (default `deepseek-v4-pro`; also `deepseek-flash`), one `TASK` per build:

- **`deepswe_one_task`** — DeepSWE via **Pier**. Same worker `icode-*-full-*` bind-mount and the same iCode CLI as LoLBench (`run -t … -C … -a code --json`). **v0.5.1 and older** wrap `icode run "$PROMPT"` and exit in seconds — install **v0.5.2**.
- **`lolbench_one_task`** — LoLBench via **Harbor** (`icode_harbor_agent`; bind-mounts the same worker `icode-*-full-*` drop as DeepSWE). First LoLBench run installs `harbor` via uv if missing. Hub `smartdub26/lolbench` tags are arm64-only; on x86_64, P5 builds or retags a local image (do not use Harbor `--force-build`).

Workers do **not** create a `.env` or store the DeepSeek key. The key lives on the **cloud Jenkins** credential `deepseek-api-key`.

The worker wizard has a default Jenkins URL (press Enter to keep it). Type `http://<CONTROLLER_IP>:17070` for your controller. Same-PC controller: `http://localhost:17070`.

---

## What you need

- Cloud Linux VM (`CONTROLLER_IP`) with **root**, ~8 GB RAM, **60 GB** free, security group **17070** open to the worker.
- New Mac or Linux worker with **sudo** (Linux may be a single root account), ~8 GB RAM, **40 GB** free, Docker allowed.
- **mac-k3d CLI** GitHub asset matching the machine (`mac-k3d-linux-x86_64`, `mac-k3d-linux-aarch64`, `mac-k3d-darwin-aarch64`, `mac-k3d-darwin-x86_64`).
- **iCode release** `icode-<os>-<arch>-full-vX.Y.Z` (or a standalone executable named `icode`) to **upload in Jenkins** when you start a release-mode job. For local `--stage` / `--local` only, copy it onto the worker after setup.

Developer checkout + `.env` is optional (appendix). Users run eval through Jenkins.

iCode is an **agent harness**. The job measures whether the harness helps an LLM: **Arm A** = iCode + LLM (DeepSeek); **Arm B** = the same LLM **without** iCode (baseline). DeepSWE runs that through Pier; LoLBench through Harbor. Compare pass@1 / resolved / tokens / time in the archived JSON. Scores are comparable as “did iCode + DeepSeek resolve the task,” not as a perfectly controlled cross-benchmark A/B (different runner and iCode install).

---

## 1. Create the controller on the cloud

```bash
# SSH to the cloud VM as root (use your IP if it is not this lab)
ssh root@CONTROLLER_IP

# Put the mac-k3d CLI on PATH (not the iCode release)
export PATH="$HOME/.local/bin:$PATH"
mkdir -p "$HOME/.local/bin"

# Download the mac-k3d CLI (change tag/arch if needed)
curl -fsSL -o /tmp/mac-k3d \
  "https://github.com/Toby-Yu/mac-k3d/releases/download/v0.5.2/mac-k3d-linux-x86_64"
chmod +x /tmp/mac-k3d
cp /tmp/mac-k3d "$HOME/.local/bin/mac-k3d"

# First-run wizard: role CI controller, Jenkins 17070, Harbor/LoLBench No
# When asked for CI secrets, enter the DeepSeek key as credential deepseek-api-key
mac-k3d setup -c ~/.config/mac-k3d/config.yaml

# Confirm Jenkins and jobs deepswe_one_task + lolbench_one_task on this VM
JENKINS_URL=http://127.0.0.1:17070 ./scripts/env_set_up/02_check_controller.sh
```

If you already cloned this repo on the VM, run `02_check` from that checkout. After upgrading the binary, refresh the job XML:

```bash
# Re-apply Jenkins jobs without re-entering secrets (keeps existing credential binds)
export PATH="$HOME/.local/bin:$PATH"
mac-k3d config -c ~/.config/mac-k3d/config.yaml --skip-secrets
```

Open `http://CONTROLLER_IP:17070` (or `http://127.0.0.1:17070` on that VM). User **admin**. Password from `mac-k3d config --show-jenkins` on the controller. Create an **API token** (admin → Configure → API Token). Copy the **secret**, not the token name.

---

## 2. Create a worker on a new Mac or Linux

```bash
# On the worker: mac-k3d CLI on PATH (not the iCode release)
export PATH="$HOME/.local/bin:$PATH"
mkdir -p "$HOME/.local/bin"
# Linux x86_64 example — pick darwin-aarch64 / linux-aarch64 as needed
curl -fsSL -o /tmp/mac-k3d \
  "https://github.com/Toby-Yu/mac-k3d/releases/download/v0.5.2/mac-k3d-linux-x86_64"
chmod +x /tmp/mac-k3d
cp /tmp/mac-k3d "$HOME/.local/bin/mac-k3d"
mac-k3d --help   # must list setup and eval

# Worker wizard. Enter keeps the printed default, or type http://CONTROLLER_IP:17070
# Type http://<new-ip>:17070 if you created another controller
# API user: admin   API token: the secret from step 1
# Harbor / LoLBench: No
# Never run: mac-k3d start -c worker.yaml
mac-k3d setup -c ~/.config/mac-k3d/worker.yaml

# Agent refresh without re-wizard also extracts ~/.local/share/mac-k3d/pipeline
# Does not overwrite an iCode drop (icode or icode-*-full-*) in that folder
mac-k3d config -c ~/.config/mac-k3d/worker.yaml
```

To copy a working controller or worker YAML onto another machine (same pipeline, different host or `jenkins_job` defaults), export on the source host and import on the dest. Secrets are stripped; import only writes the file. Full flow, DeepSWE first→second question, and why `config --skip-secrets` does not put keys in the YAML: [export-import.md](export-import.md).

```bash
# Source host: sanitized copy (no api_token, no credentials.pending.yaml)
mac-k3d export -c ~/.config/mac-k3d/config.yaml -o /tmp/controller.yaml
mac-k3d export -c ~/.config/mac-k3d/worker.yaml -o /tmp/worker.yaml
# Optional: edit jenkins_job.default_task / labels / controller_url in those files

# Dest host: write only, then setup or config as usual
mac-k3d import /tmp/controller.yaml
mac-k3d import /tmp/worker.yaml -c ~/.config/mac-k3d/worker.yaml
# Overwrite an existing dest file:
# mac-k3d import /tmp/worker.yaml -c ~/.config/mac-k3d/worker.yaml --force
```

Linux without sudo will fail Docker install. As **root**, Docker is ready without a logout. As a normal user, log out/in after the `docker` group is added, then re-run `setup`. macOS: open **Docker Desktop** until it is idle.

```bash
# Optional: confirm this PC can reach cloud Jenkins and the agent unit is up
export PATH="$HOME/.local/bin:$PATH"
JENKINS_URL=http://CONTROLLER_IP:17070 \
  bash -lc 'echo "check Jenkins login"; curl -sS -o /dev/null -w "%{http_code}\n" "$JENKINS_URL/login"'
```

In Jenkins: **Manage Jenkins → Nodes**. This machine must be **online** with label **`lolbench`**.

---

## 3. Provide iCode (after the node is Online, before eval)

Details: [icode-harness-inputs.md](icode-harness-inputs.md). Both methods work for DeepSWE and LoLBench.

1. **Release binary (typical)** — download `icode-<os>-<arch>-full-vX.Y.Z`. **Jenkins:** Build with Parameters → `ICODE_MODE=release` → upload `ICODE_RELEASE_FILE` (the original `*-full-*` name is lost; P3 uses gzip/tar magic or copies a standalone binary as `icode`). **Local `--stage` / `--local`:** copy to `~/.local/share/mac-k3d/` or `mac-k3d set --icode-release /path/to/that-drop`. This is **not** a git clone of GitHub Release assets.
2. **Git clone** — `ICODE_MODE=git` plus URL + `ICODE_GIT_REF` + `ICODE_GIT_REF_KIND` (`branch` / `tag` / `commit`). A tag checkout is a git tag, not a downloaded zip. Report field `icode_git` records the resolved SHA. The clone is under the eval workdir, not a permanent developer tree.

```bash
# Local only (not Jenkins): discover under the worker share
mkdir -p "$HOME/.local/share/mac-k3d"
# Browser often saves this with no .tar.gz — that is OK:
cp /path/to/icode-linux-x86_64-full-v0.1.41 "$HOME/.local/share/mac-k3d/"
```

- A `*.tar.gz` / `*.tgz` or unpacked `*-full-*` **folder** must contain a file named `icode`.
- Do **not** copy `.venv/bin/icode` (tiny Python wrapper) into the drop folder. Do **not** put iCode inside `pipeline/`.

```bash
# Persist the binary drop path (once)
mac-k3d set --icode-release /path/to/icode-linux-x86_64-full-v0.1.41

# Git tag (clone, not Release assets)
mac-k3d eval --stage p3 --icode-mode git \
  --icode-git-url https://github.com/ORG/icode.git \
  --icode-git-ref v0.1.41 --icode-git-ref-kind tag
```

Rebuild/extract pipeline on the worker (`mac-k3d eval --stage p0` or `config`) so `pipeline/lib/icode_input.sh` is present.

---

## 4. Run evaluation (N=1)

### Jenkins UI (recommended)

1. Open `http://CONTROLLER_IP:17070` → job **`deepswe_one_task`** (Pier + DeepSWE) or **`lolbench_one_task`** (Harbor + LoLBench). One `TASK` per build. Do not flip `BENCHMARK` across jobs.
2. **Build with Parameters**:

| Field | Value |
|-------|--------|
| HARNESS / LLM / BENCHMARK | `icode` / `deepseek` / `deepswe` or `lolbench` (fixed on that job) |
| TASK | empty on DeepSWE = first alphabetical; LoLBench default `ruff_1` |
| N_TASKS | `1` |
| ICODE_MODE | Choose `release` (upload a drop) or `git` (clone URL + ref). Fill only the fields for that choice; leave the other group as it is. |
| ICODE_RELEASE_FILE | **release:** choose the `icode` / `icode-*-full-*` drop here. **git:** do not choose a file; leave this control as it is. Vice versa: if you chose git, ignore this; if you chose release, this is the file you upload. |
| ICODE_GIT_URL | **git:** https URL on github.com or gitcode.com. **release:** do not change this; leave it as it is. Vice versa: if you chose release, ignore this; if you chose git, this is required. |
| ICODE_GIT_REF | **git:** branch name, tag, or commit SHA (match KIND). **release:** do not change this; leave it as it is. Vice versa: if you chose release, ignore this; if you chose git, this is required. |
| ICODE_GIT_REF_KIND | **git:** pick `branch`, `tag`, or `commit` (no auto). **release:** do not change this; leave it as it is. Vice versa: if you chose release, ignore this; if you chose git, pick the kind that matches REF. |
| AGENT_LABEL | `lolbench` |
| CPU_LOCK_QTY | `4` |
| MAC_K3D_ROOT | empty |
| DEEPSEEK_MODEL | `deepseek-v4-pro` (catalog default; or `deepseek-flash`) |

Git-mode CI: set `ICODE_MODE=git`, fill `ICODE_GIT_URL` / `ICODE_GIT_REF` / `ICODE_GIT_REF_KIND`. The archived JSON includes `icode_git` (resolved SHA + subject). Full split: [icode-harness-inputs.md](icode-harness-inputs.md).

Release-mode CI: **upload** `ICODE_RELEASE_FILE` in this UI. `mac-k3d eval --yes` cannot attach a file (it uses GET `buildWithParameters`).

3. Click **Build**. Console must say `Running on <this-worker-name>`.
4. Download **Build Artifacts** → `eval-runs/reports/eval-icode-deepseek-{deepswe|lolbench}-n1-*.json`.
   On the worker the same files are at `$HOME/jenkins-agent/workspace/deepswe_one_task/eval-runs/reports/` or `…/workspace/lolbench_one_task/eval-runs/reports/` (or `{remote_fs}/workspace/<job>/eval-runs/reports/` if `jenkins_agent.remote_fs` was changed).

### CLI from the worker (queues Jenkins; no `--local`)

```bash
# Queue deepswe_one_task on the controller. Uses worker.yaml when config.yaml is absent.
# Does not run Pier on this shell — watch Jenkins Console Output.
export PATH="$HOME/.local/bin:$PATH"
# Git mode still queues with --yes:
mac-k3d eval --benchmark deepswe --n-tasks 1 --icode-mode git \
  --icode-git-url https://github.com/ORG/icode.git --icode-git-ref main --yes
# Release mode: do not use --yes. Open Jenkins UI, set ICODE_MODE=release, upload ICODE_RELEASE_FILE.
# LoLBench git, one task through Harbor + iCode (not Pier):
# mac-k3d eval --benchmark lolbench --task ruff_1 --icode-mode git --icode-git-url … --yes
```

`--yes` without `--local` means **Jenkins**. An old binary may run a local eval instead; replace `~/.local/bin/mac-k3d` with v0.5.2 or newer.

Expect: build **SUCCESS**, artifact present. `pass_at_1` may be `0.0` on N=1 — that is a task result, not a setup failure. Missing F2P/P2P rates are `null`, not empty test-name lists.

```bash
# Optional: schema check on the downloaded JSON (from a checkout or extracted pipeline)
./pipeline/stages/check_report.sh /path/to/eval-icode-deepseek-deepswe-n1-*.json
```

---

## 5. Developer only (this git checkout)

```bash
# Local P0–P8 without Jenkins. Needs .env (gitignored), not for workers.
cd /path/to/mac-k3d
cp .env.example .env
chmod 600 .env
# put DEEPSEEK_API_KEY and DEEPSEEK_MODEL=deepseek-v4-pro in .env
# catalog also allows DEEPSEEK_MODEL=deepseek-flash
export PATH="$HOME/.local/bin:$PATH"
mac-k3d eval --local --benchmark deepswe --n-tasks 1 --icode-mode binary
# mac-k3d eval --local --benchmark lolbench --task ruff_1 --icode-mode binary
# LoLBench local still uses Harbor in P5; P3 still resolves the iCode drop for the bind-mount
# or ICODE_MODE=git to clone from GitHub/GitCode
```

---

## Failures

| Symptom | What to do |
|---------|------------|
| Waiting for executor | Worker node offline; do not `start -c worker.yaml` |
| `pipeline/stages/run_all.sh` missing | Worker: `mac-k3d config -c worker.yaml` (extracts) **or** `mac-k3d eval --stage p0` |
| iCode not found | Jenkins release: upload `ICODE_RELEASE_FILE`. Local: drop official `icode-*-full-*` or a file named `icode` in `~/.local/share/mac-k3d/` (see §3), or use `ICODE_MODE=git`. Both DeepSWE (Pier) and LoLBench (Harbor) bind-mount that tree. |
| `cannot remove leftover … icode-src` | Old pipeline on the worker. Rebuild/install CLI, then `mac-k3d eval --stage p0` (or `config -c worker.yaml`) so `~/.local/share/mac-k3d/pipeline` has `icode_force_rm`. Do not `sudo rm` as a routine step. |
| First LoLBench image is slow | Expected: P1 may `uv tool install harbor`; P0 installs `docker buildx` if missing. Hub `smartdub26/lolbench` tags are **linux/arm64**. On x86_64, P5 runs `docker build --progress=plain` once (not Harbor `--force-build`, which hangs after the image is tagged). |
| `DEEPSEEK_API_KEY missing` on Jenkins | Add credential `deepseek-api-key` on the **controller** |
| Disk/RAM preflight | Free space (`docker system df`); keep `N_TASKS=1` |
| `--yes` ran a local eval | Old CLI; install v0.5.2+ |
| Job still `deepseek-chat` or Toby paths | Controller: install new binary, `config --skip-secrets` |

E8 (N>1) is optional after N=1 is green. Lab operator notes: [cloud-eval-runbook.md](testing/cloud-eval-runbook.md).
