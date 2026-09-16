# User guide: cloud controller, worker, and eval

Copy-paste these blocks. Every command has a `#` comment.

Two different downloads:

- **mac-k3d CLI** — GitHub asset `mac-k3d-linux-x86_64` (or `linux-aarch64` / `darwin-aarch64` / `darwin-x86_64`). Used in steps 1–2.
- **iCode release** — official `icode-<os>-<arch>-full-vX.Y.Z` from the iCode project. Used in step 3 on the **worker only**. Not the same file as mac-k3d.

Prefer **v0.5.2** Latest (or this checkout’s `cargo build --release`). **v0.5.1** DeepSWE Pier used the wrong iCode CLI and often exited in ~20s. **v0.5.0** still has the old P3: it only accepts a file named `icode` or a `*.tar.gz` / `*.tgz`. An extensionless `icode-…-full-…` download needs v0.5.1+.

Harbor CLI stays **skip** on DeepSWE-only workers. Eval jobs are two runners, both iCode + `deepseek-v4-pro`, one `TASK` per build:

- **`deepswe_one_task`** — DeepSWE via **Pier**. Same worker `icode-*-full-*` bind-mount and the same iCode CLI as LoLBench (`run -t … -C … -a code --json`). **v0.5.1 and older** wrap `icode run "$PROMPT"` and exit in seconds — install **v0.5.2**.
- **`lolbench_one_task`** — LoLBench via **Harbor** (`icode_harbor_agent`; bind-mounts the same worker `icode-*-full-*` drop as DeepSWE). First LoLBench run installs `harbor` via uv if missing. Hub `smartdub26/lolbench` tags are arm64-only; on x86_64, P5 builds or retags a local image (do not use Harbor `--force-build`).

Workers do **not** create a `.env` or store the DeepSeek key. The key lives on the **cloud Jenkins** credential `deepseek-api-key`.

Default worker Jenkins URL is `http://43.107.42.252:17070`. Type a new `http://<ip>:17070` when you buy another controller.

---

## What you need

- Cloud Linux VM (this lab: `43.107.42.252`) with **root**, ~8 GB RAM, **60 GB** free, security group **17070** open to the worker.
- New Mac or Linux worker with **sudo** (Linux may be a single root account), ~8 GB RAM, **40 GB** free, Docker allowed.
- **mac-k3d CLI** GitHub asset matching the machine (`mac-k3d-linux-x86_64`, `mac-k3d-linux-aarch64`, `mac-k3d-darwin-aarch64`, `mac-k3d-darwin-x86_64`).
- **iCode release** `icode-<os>-<arch>-full-vX.Y.Z` (or a standalone executable named `icode`) to copy onto the worker **after** setup, before eval.

Developer checkout + `.env` is optional (appendix). Users run eval through Jenkins.

iCode is an **agent harness**. The job measures whether the harness helps an LLM: **Arm A** = iCode + LLM (DeepSeek); **Arm B** = the same LLM **without** iCode (baseline). DeepSWE runs that through Pier; LoLBench through Harbor. Compare pass@1 / resolved / tokens / time in the archived JSON. Scores are comparable as “did iCode + DeepSeek resolve the task,” not as a perfectly controlled cross-benchmark A/B (different runner and iCode install).

---

## 1. Create the controller on the cloud

```bash
# SSH to the cloud VM as root (use your IP if it is not this lab)
ssh root@43.107.42.252

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

Open `http://43.107.42.252:17070` (or your IP). User **admin**. Password from `mac-k3d config --show-jenkins` on the controller. Create an **API token** (admin → Configure → API Token). Copy the **secret**, not the token name.

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

# Worker wizard. Enter keeps http://43.107.42.252:17070
# Type http://<new-ip>:17070 if you created another controller
# API user: admin   API token: the secret from step 1
# Harbor / LoLBench: No
# Never run: mac-k3d start -c worker.yaml
mac-k3d setup -c ~/.config/mac-k3d/worker.yaml

# Agent refresh without re-wizard also extracts ~/.local/share/mac-k3d/pipeline
# Does not overwrite an iCode drop (icode or icode-*-full-*) in that folder
mac-k3d config -c ~/.config/mac-k3d/worker.yaml
```

Linux without sudo will fail Docker install. As **root**, Docker is ready without a logout. As a normal user, log out/in after the `docker` group is added, then re-run `setup`. macOS: open **Docker Desktop** until it is idle.

```bash
# Optional: confirm this PC can reach cloud Jenkins and the agent unit is up
export PATH="$HOME/.local/bin:$PATH"
JENKINS_URL=http://43.107.42.252:17070 \
  bash -lc 'echo "check Jenkins login"; curl -sS -o /dev/null -w "%{http_code}\n" "$JENKINS_URL/login"'
```

In Jenkins: **Manage Jenkins → Nodes**. This machine must be **online** with label **`lolbench`**.

---

## 3. Place the iCode release on the worker (after the node is Online, before eval)

Do this on the **worker**, not the cloud controller. New users use **A**. Type a Jenkins path only when the file is **not** in the drop folder.

### A. New user — binary (default)

Put **one** official iCode download into `~/.local/share/mac-k3d/` (keep the name GitHub/GitCode gave it; do not invent a `.tar.gz` suffix):

```bash
mkdir -p "$HOME/.local/share/mac-k3d"
# Browser often saves this with no .tar.gz — that is OK:
cp /path/to/icode-linux-x86_64-full-v0.1.41 "$HOME/.local/share/mac-k3d/"
# or the archive, if that is what you have:
# cp /path/to/icode-linux-x86_64-full-v0.1.41.tar.gz "$HOME/.local/share/mac-k3d/"
# or an unpacked folder of that name:
# cp -a /path/to/icode-linux-x86_64-full-v0.1.41 "$HOME/.local/share/mac-k3d/"
# or a standalone executable already named icode:
# cp /path/to/icode "$HOME/.local/share/mac-k3d/icode" && chmod +x "$HOME/.local/share/mac-k3d/icode"
```

- A `*.tar.gz` / `*.tgz` or unpacked `*-full-*` **folder** must contain a file named `icode`.
- A **single file** named `icode-…-full-…` (no suffix) is the release itself; P3 copies it to `eval-runs/icode-bin/icode`.
- Do **not** also leave an old file named `icode` next to a `*-full-*` drop. Discover prefers the official `*-full-*` name, then `icode`.
- Do **not** copy `.venv/bin/icode` (tiny Python wrapper). Do **not** put iCode inside `pipeline/` (mac-k3d extracts that folder).

Optional fallback (sudo, not required): the same shapes under `/opt/mac-k3d/`.

If the file stays in Downloads, set Jenkins `ICODE_RELEASE` to that path (or `mac-k3d eval --icode-release`). Still leave `ICODE_SOURCE` empty.

Jenkins UI: `ICODE_MODE=binary`, **`ICODE_RELEASE` and `ICODE_SOURCE` empty**.

### B. Developer — source

Use a **directory** (git checkout) that has `.venv/bin/icode` or `pyproject.toml` (then `uv sync` on the worker).

Jenkins: `ICODE_MODE=source`. Leave `ICODE_RELEASE` empty. Leave **`ICODE_SOURCE` empty** only if the tree is one of: `$HOME/Documents/iCode-main`, `$HOME/iCode-main`, `$HOME/src/iCode-main`, or `iCode-main` next to the mac-k3d checkout. Otherwise paste the folder path, or `mac-k3d eval --icode-mode source --icode-source /path/to/iCode`.

---

## 4. Run evaluation (N=1)

### Jenkins UI (recommended)

1. Open `http://43.107.42.252:17070` → job **`deepswe_one_task`** (Pier + DeepSWE) or **`lolbench_one_task`** (Harbor + LoLBench). One `TASK` per build. Do not flip `BENCHMARK` across jobs.
2. **Build with Parameters**:

| Field | Value |
|-------|--------|
| HARNESS / LLM / BENCHMARK | `icode` / `deepseek` / `deepswe` or `lolbench` (fixed on that job) |
| TASK | empty on DeepSWE = first alphabetical; LoLBench default `ruff_1` |
| N_TASKS | `1` |
| ICODE_MODE | `binary` |
| ICODE_RELEASE | empty (discovers the drop path) |
| ICODE_SOURCE | empty |
| AGENT_LABEL | `lolbench` |
| CPU_LOCK_QTY | `4` |
| MAC_K3D_ROOT | empty |
| DEEPSEEK_MODEL | `deepseek-v4-pro` |

Source-mode users: set `ICODE_MODE=source` and fill `ICODE_SOURCE` only when the tree is not in the default list (see §3 B).

3. Click **Build**. Console must say `Running on <this-worker-name>`.
4. Download **Build Artifacts** → `eval-runs/reports/eval-icode-deepseek-{deepswe|lolbench}-n1-*.json`.
   On the worker the same files are at `$HOME/jenkins-agent/workspace/deepswe_one_task/eval-runs/reports/` or `…/workspace/lolbench_one_task/eval-runs/reports/` (or `{remote_fs}/workspace/<job>/eval-runs/reports/` if `jenkins_agent.remote_fs` was changed).

### CLI from the worker (queues Jenkins; no `--local`)

```bash
# Queue deepswe_one_task on the controller. Uses worker.yaml when config.yaml is absent.
# Does not run Pier on this shell — watch Jenkins Console Output.
export PATH="$HOME/.local/bin:$PATH"
mac-k3d eval --benchmark deepswe --n-tasks 1 --icode-mode binary --yes
# LoLBench, one task through Harbor + iCode (not Pier):
# mac-k3d eval --benchmark lolbench --task ruff_1 --icode-mode binary --yes
# Same thing, explicit file:
# mac-k3d eval -c ~/.config/mac-k3d/worker.yaml --benchmark deepswe --icode-mode binary --yes
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
export PATH="$HOME/.local/bin:$PATH"
mac-k3d eval --local --benchmark deepswe --n-tasks 1 --icode-mode binary
# mac-k3d eval --local --benchmark lolbench --task ruff_1 --icode-mode binary
# LoLBench local still uses Harbor in P5; P3 still resolves the iCode drop for the bind-mount
# or ICODE_MODE=source if you have an iCode git tree
```

---

## Failures

| Symptom | What to do |
|---------|------------|
| Waiting for executor | Worker node offline; do not `start -c worker.yaml` |
| `pipeline/stages/run_all.sh` missing | Worker: `mac-k3d config -c worker.yaml` (extracts) **or** `mac-k3d eval --stage p0` |
| iCode not found | Drop official `icode-*-full-*` or a file named `icode` in `~/.local/share/mac-k3d/` (see §3). Both DeepSWE (Pier) and LoLBench (Harbor) bind-mount that drop; GitCode clone is not used. |
| First LoLBench image is slow | Expected: P1 may `uv tool install harbor`; P0 installs `docker buildx` if missing. Hub `smartdub26/lolbench` tags are **linux/arm64**. On x86_64, P5 runs `docker build --progress=plain` once (not Harbor `--force-build`, which hangs after the image is tagged). |
| `DEEPSEEK_API_KEY missing` on Jenkins | Add credential `deepseek-api-key` on the **controller** |
| Disk/RAM preflight | Free space (`docker system df`); keep `N_TASKS=1` |
| `--yes` ran a local eval | Old CLI; install v0.5.2+ |
| Job still `deepseek-chat` or Toby paths | Controller: install new binary, `config --skip-secrets` |

E8 (N>1) is optional after N=1 is green. Lab operator notes: [cloud-eval-runbook.md](testing/cloud-eval-runbook.md).
