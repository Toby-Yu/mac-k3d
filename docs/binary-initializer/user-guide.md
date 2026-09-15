# User guide: cloud controller, worker, and eval

Copy-paste these blocks. Every command has a `#` comment. Prefer GitHub pre-release **v0.5.0-rc.1** (or newer) so the installed CLI matches this tree.

Harbor / LoLBench stay **skip**. Workers do **not** create a `.env` or store the DeepSeek key. The key lives on the **cloud Jenkins** credential `deepseek-api-key`.

Default worker Jenkins URL is `http://43.107.42.252:17070`. Type a new `http://<ip>:17070` when you buy another controller.

---

## What you need

- Cloud Linux VM (this lab: `43.107.42.252`) with **root**, ~8 GB RAM, **60 GB** free, security group **17070** open to the worker.
- New Mac or Linux worker with **sudo** (Linux may be a single root account), ~8 GB RAM, **40 GB** free, Docker allowed.
- An iCode **binary** or `*-full-*.tar.gz` to copy after setup.
- GitHub asset matching the machine (`mac-k3d-linux-x86_64`, `mac-k3d-linux-aarch64`, `mac-k3d-darwin-aarch64`, `mac-k3d-darwin-x86_64`).

Developer checkout + `.env` is optional (appendix). Users run eval through Jenkins.

iCode is an **agent harness**. The job measures whether the harness helps an LLM on DeepSWE (via Pier): **Arm A** = iCode + LLM (DeepSeek); **Arm B** = the same LLM **without** iCode (baseline). Compare pass@1 / resolved / tokens / time in the archived JSON.

---

## 1. Create the controller on the cloud

```bash
# SSH to the cloud VM as root (use your IP if it is not this lab)
ssh root@43.107.42.252

# Put the Release binary on PATH
export PATH="$HOME/.local/bin:$PATH"
mkdir -p "$HOME/.local/bin"

# Download the Linux asset from the pre-release (change tag/arch if needed)
curl -fsSL -o /tmp/mac-k3d \
  "https://github.com/Toby-Yu/mac-k3d/releases/download/v0.5.0-rc.1/mac-k3d-linux-x86_64"
chmod +x /tmp/mac-k3d
cp /tmp/mac-k3d "$HOME/.local/bin/mac-k3d"

# First-run wizard: role CI controller, Jenkins 17070, Harbor/LoLBench No
# When asked for CI secrets, enter the DeepSeek key as credential deepseek-api-key
mac-k3d setup -c ~/.config/mac-k3d/config.yaml

# Confirm Jenkins and job icode_eval on this VM
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
# On the worker: Release binary on PATH
export PATH="$HOME/.local/bin:$PATH"
mkdir -p "$HOME/.local/bin"
# Linux x86_64 example — pick darwin-aarch64 / linux-aarch64 as needed
curl -fsSL -o /tmp/mac-k3d \
  "https://github.com/Toby-Yu/mac-k3d/releases/download/v0.5.0-rc.1/mac-k3d-linux-x86_64"
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
# (does not overwrite ~/.local/share/mac-k3d/icode)
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

## 3. Place the iCode binary (after setup, before eval)

```bash
# Official drop path (no sudo)
mkdir -p "$HOME/.local/share/mac-k3d"

# Either copy the icode executable:
cp /path/to/icode "$HOME/.local/share/mac-k3d/icode"
chmod +x "$HOME/.local/share/mac-k3d/icode"

# Or copy a -full- tarball into the same folder (name may vary):
# cp /path/to/icode-*-full-*.tar.gz "$HOME/.local/share/mac-k3d/"

# Optional system path (needs sudo):
# sudo mkdir -p /opt/mac-k3d && sudo cp /path/to/icode /opt/mac-k3d/icode
```

Do **not** put the file inside `~/.local/share/mac-k3d/pipeline/` (that folder is extracted by `mac-k3d`).

---

## 4. Run evaluation (N=1)

### Jenkins UI (recommended)

1. Open `http://43.107.42.252:17070` → job **`icode_eval`** (not `lolbench_one_task`).
2. **Build with Parameters**:

| Field | Value |
|-------|--------|
| HARNESS / LLM / BENCHMARK | `icode` / `deepseek` / `deepswe` |
| N_TASKS | `1` |
| ICODE_MODE | `binary` |
| ICODE_RELEASE | empty (discovers the drop path) |
| ICODE_SOURCE | empty |
| AGENT_LABEL | `lolbench` |
| CPU_LOCK_QTY | `4` |
| MAC_K3D_ROOT | empty |
| DEEPSEEK_MODEL | `deepseek-v4-pro` |

3. Click **Build**. Console must say `Running on <this-worker-name>`.
4. Download **Build Artifacts** → `eval-runs/reports/eval-icode-deepseek-deepswe-n1-*.json`.

### CLI from the worker (queues Jenkins; no `--local`)

```bash
# Queue icode_eval on the controller. Needs worker.yaml API token.
# Does not run Pier on this shell — watch Jenkins Console Output.
export PATH="$HOME/.local/bin:$PATH"
mac-k3d eval --n-tasks 1 --icode-mode binary --yes
```

`--yes` without `--local` means **Jenkins**. An old binary may run a local eval instead; replace `~/.local/bin/mac-k3d` with v0.5.0-rc.1 or newer.

Expect: build **SUCCESS**, artifact present. `pass_at_1_harness` may be `0.0` on N=1 — that is a task result, not a setup failure. Empty `f2p`/`p2p` lists and harness tokens `0` are known scorer limits.

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
mac-k3d eval --local --n-tasks 1 --icode-mode binary
# or ICODE_MODE=source if you have an iCode git tree
```

---

## Failures

| Symptom | What to do |
|---------|------------|
| Waiting for executor | Worker node offline; do not `start -c worker.yaml` |
| `pipeline/stages/run_all.sh` missing | Worker: `mac-k3d config -c worker.yaml` (extracts) **or** `mac-k3d eval --stage p0` |
| iCode not found | Copy binary to `~/.local/share/mac-k3d/icode` |
| `DEEPSEEK_API_KEY missing` on Jenkins | Add credential `deepseek-api-key` on the **controller** |
| Disk/RAM preflight | Free space (`docker system df`); keep `N_TASKS=1` |
| `--yes` ran a local eval | Old CLI; install v0.5.0-rc.1+ |
| Job still `deepseek-chat` or Toby paths | Controller: install new binary, `config --skip-secrets` |

E8 (N>1) is optional after N=1 is green. Lab operator notes: [cloud-eval-runbook.md](cloud-eval-runbook.md).
