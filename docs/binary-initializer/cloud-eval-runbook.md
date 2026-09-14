# Cloud controller + local worker eval runbook

Operator path for **this lab**: Alibaba Cloud VM hosts Jenkins; this PC runs the jobs and writes the JSON.

Pass/fail table and stage detail stay in [testing-eval-pipeline.md](testing-eval-pipeline.md). Product story: [workflow.md](workflow.md). User bootstrap: [binary-initializer-new-machine.md](binary-initializer-new-machine.md).

**Sign-off:** E0–E7 with `--n-tasks 1`. Harbor / LoLBench stay skip. After each phase, paste the checkpoint output before starting the next.

---

## Topology

Jobs must **not** run inside the controller’s k3d nodes. Cloud Jenkins only queues. This PC (Docker + Pier + iCode) executes.

```text
Cloud VM (SSH as root): k3d + Jenkins :17070
    └── queue icode_eval (label lolbench)
This PC (Toby): Jenkins agent + Docker + iCode
    ├── api.deepseek.com  (deepseek-v4-pro)
    └── archive JSON back to cloud Jenkins
```

Do **not** continue the leftover `y84462643` session (no docker group, k3d missing). Cloud paths are under **root’s home**: `/root/src/mac-k3d`, `/root/.config/mac-k3d/config.yaml`, `/root/.local/bin/mac-k3d`.

Never `mac-k3d start -c worker.yaml`. Worker YAML must use `http://<CLOUD_IP>:17070`, not localhost.

---

## High-level flow

```mermaid
flowchart TD
  sshRoot["SSH root on cloud VM"]
  dockerOk["docker info shows Server"]
  cloneBuild["Clone binary-initializer and install mac-k3d"]
  setupCtrl["mac-k3d setup: CI controller Jenkins 17070"]
  e0check["E0: 02_check_controller.sh plus browser 17070"]
  workerSetup["This PC: setup worker.yaml"]
  e1check["E1: node online in cloud Jenkins"]
  cheap["E2-E3: P0-P4 plus report schema"]
  paid["E4-E6: P5 harness P6 baseline P7-P8 JSON"]
  e7job["E7: Jenkins icode_eval on this PC"]
  jsonOut["Named JSON archived and schema OK"]

  sshRoot --> dockerOk --> cloneBuild --> setupCtrl --> e0check
  e0check --> workerSetup --> e1check
  e1check --> cheap --> paid --> e7job --> jsonOut
```

```mermaid
flowchart TD
  p0["P0 Docker and CLI"]
  p1["P1 Pier"]
  p2["P2 clone DeepSWE GitHub"]
  p3["P3 iCode folder on this PC"]
  p4["P4 Pier agent icode"]
  p5["P5 Arm A: Pier plus iCode plus DeepSeek"]
  p6["P6 Arm B: DeepSeek API only"]
  p7["P7 score f2p p2p"]
  p8["P8 named JSON under output/"]
  jenkins["E7 icode_eval archives the same JSON"]

  p0 --> p1 --> p2 --> p3 --> p4
  p4 --> p5
  p4 --> p6
  p5 --> p7
  p6 --> p7
  p7 --> p8 --> jenkins
```

**iCode “source” in E0–E7** means the folder already on this PC (`ICODE_SOURCE=/home/Toby/Documents/Toby/iCode-main`). It is **not** a GitCode clone. The controller wizard `EVAL_MODE=binary` default is unused by this checklist.

---

## Facts for this lab

| Item | Value |
|------|--------|
| `<CLOUD_IP>` | `43.107.42.252` |
| SSH (controller) | `ssh root@43.107.42.252` |
| Jenkins UI | `http://43.107.42.252:17070` |
| Jenkins port | **17070** (open this in the Alibaba security group to this PC) |
| Cloud checkout | `/root/src/mac-k3d` branch `binary-initializer` |
| This PC checkout | `/home/Toby/Documents/Toby/mac-k3d` |
| iCode on this PC | `/home/Toby/Documents/Toby/iCode-main` |
| Model | `deepseek-v4-pro` (`DEEPSEEK_MODEL` or `--model`) |
| N | **1** until E7 is green |
| Credential on controller | `deepseek-api-key` |

Do not commit API keys, Jenkins passwords, or API tokens.

---

## Phase 0 — Cloud as root (clone + CLI)

**Where:** new SSH session as **root**.  
**Pass:** `docker info` shows a Server section; `mac-k3d` 0.4.x on PATH; `--help` lists `setup` and `eval`.

```bash
ssh root@43.107.42.252
```

```bash
# Docker must already talk to the daemon (root can use the socket)
docker info | head -20
# expect a "Server:" section, not "permission denied"
```

```bash
mkdir -p ~/.local/bin ~/src
cd ~/src
git clone --branch binary-initializer --single-branch https://github.com/Toby-Yu/mac-k3d.git
cd mac-k3d
git status -sb
git log -1 --oneline
```

Build the binary that matches this tree (or skip rustup if you `scp` a known-good `target/release/mac-k3d`):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

unset CARGO_TARGET_DIR
export CARGO_TARGET_DIR="$PWD/target"
cargo build --release

cp -f target/release/mac-k3d ~/.local/bin/mac-k3d
export PATH="$HOME/.local/bin:$PATH"
hash -r

which mac-k3d
mac-k3d --version
mac-k3d --help
```

Keep PATH in this shell. Optional persist:

```bash
grep -q '.local/bin' ~/.bashrc || echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
grep -q '.cargo/env' ~/.bashrc || echo 'source "$HOME/.cargo/env"' >> ~/.bashrc
```

**Checkpoint — paste:** `docker info` Server lines, `git log -1`, `which mac-k3d`, `--version`, and that `--help` lists `setup` and `eval`.

---

## Phase 1 — E0 cloud controller

**Where:** same root session.  
**Pass:** `=== mac-k3d setup complete ===` Role `controller`; `02_check_controller.sh` OK; browser `http://43.107.42.252:17070` HTTP 200.

Alibaba security group: allow **TCP 17070** from this PC’s public IP (and optionally 18080 if the wizard remaps cluster HTTP).

```bash
export PATH="$HOME/.local/bin:$PATH"
cd /root/src/mac-k3d
mac-k3d setup -c ~/.config/mac-k3d/config.yaml
```

If this is root’s first run there is no live `config.yaml`. If you see **Config already exists**, choose **Re-run wizard (overwrite)** only if that file is leftover/wrong.

| Prompt | Choose |
|--------|--------|
| Base directory | Recommended path with enough disk |
| Role | **CI controller (Jenkins in k3d)** |
| Docker | **Use this installation** |
| k3d / kubectl / helm | **Install** if missing (OK as root) or **Use this installation** |
| Harbor / LoLBench | **Skip** |
| Cluster name | Default (`ci-controller`) |
| k3d agent nodes | `0` |
| Jenkins UI host port | **17070** |
| Default EVAL_MODE | **binary** (unused by E0–E7) |
| TASK / ICODE_* | Enter / leave empty |
| Enter CI secrets now? | **yes** — paste **DeepSeek** (`deepseek-api-key`). Other keys optional. |
| Write + apply | **yes** |

`Host port 8080 in use → using 18080` or `Host port 8443 in use → using 18443` is **OK**. Keep Jenkins on **17070**.

After setup:

```bash
export PATH="$HOME/.local/bin:$PATH"
cd /root/src/mac-k3d

mac-k3d config -c ~/.config/mac-k3d/config.yaml --skip-secrets
mac-k3d config -c ~/.config/mac-k3d/config.yaml --show-jenkins
# note admin password — do not commit it

JENKINS_URL=http://127.0.0.1:17070 ./scripts/env_set_up/02_check_controller.sh
```

**Expected:** `OK Jenkins UI … HTTP 200`, `OK job lolbench_one_task present`, `OK job icode_eval present`, `OK 02_check_controller complete`.

From this PC: open `http://43.107.42.252:17070` (user **admin** + printed password).

**Checkpoint — paste:** setup complete summary, `--show-jenkins` URL (redact password), and the full `02_check_controller.sh` output.

---

## Phase 2 — E1 this PC as worker

**Where:** this PC (`Toby@Michael-Ubuntu`), **not** the cloud VM.  
**Pass:** `e1_reach_jenkins.sh` HTTP 200; worker `start` rejected; agent unit active; node **online** in cloud Jenkins → Nodes.

If a **local** lab controller still binds `:17070`, teardown it or leave it unused. Worker URL is the **cloud** IP.

### 2a. Reach Jenkins from this PC

```bash
export PATH="$HOME/.local/bin:$PATH"
cd ~/Documents/Toby/mac-k3d
JENKINS_URL=http://43.107.42.252:17070 ./scripts/eval/e1_reach_jenkins.sh
```

**Expected:** `HTTP 200` and `OK Jenkins reachable`. If this fails, fix the security group before the wizard.

### 2b. Jenkins API token (not the login password)

On `http://43.107.42.252:17070`: log in as **admin** → **admin** (top right) → **Configure** → **API Token** → **Add new Token** → generate. Copy the **secret** once. `api_user` is `admin`.

### 2c. Worker wizard

```bash
export PATH="$HOME/.local/bin:$PATH"
mac-k3d setup -c ~/.config/mac-k3d/worker.yaml
```

| Prompt | Choose |
|--------|--------|
| Role | **CI worker (Jenkins agent only)** |
| Docker / Java | Use existing or **Install** |
| k3d / kubectl | **Skip** |
| Harbor / LoLBench | **No** |
| Jenkins controller URL | **Enter** — wizard default is `http://43.107.42.252:17070` |
| API user / token | `admin` + the **token secret** |
| Agent name | Distinct default (hostname) is fine |
| Write + apply | **yes** |

If the token was empty, edit `~/.config/mac-k3d/worker.yaml` (`api_user` / `api_token` / `controller_url`) then:

```bash
mac-k3d config -c ~/.config/mac-k3d/worker.yaml
```

### 2d. Prove worker

```bash
export PATH="$HOME/.local/bin:$PATH"
cd ~/Documents/Toby/mac-k3d

mac-k3d start -c ~/.config/mac-k3d/worker.yaml ; echo "exit=$?"
# expect Error about start-is-for-controller and exit=1

systemctl --user status mac-k3d-jenkins-agent.service
# Active: active (running) — press q to leave

REQUIRE_WORKER=1 JENKINS_URL=http://43.107.42.252:17070 \
  ./scripts/env_set_up/03_check_worker.sh
```

In cloud Jenkins: **Manage Jenkins → Nodes** — this PC’s agent is **online**.

**Checkpoint — paste:** `e1` HTTP 200, worker `start` error + `exit=1`, unit active, `03_check_worker.sh` OK.

---

## Phase 3 — E2 / E3 (no LLM)

**Where:** this PC, mac-k3d checkout.

```bash
export PATH="$HOME/.local/bin:$PATH"
export DEEPSEEK_MODEL=deepseek-v4-pro
cd ~/Documents/Toby/mac-k3d

mac-k3d eval --stage p0
mac-k3d eval --stage p1
mac-k3d eval --stage p2
ICODE_MODE=source ICODE_SOURCE=/home/Toby/Documents/Toby/iCode-main mac-k3d eval --stage p3
mac-k3d eval --stage p4

./scripts/eval/check_report.sh eval/testdata/report-min.json
# optional: python3 eval/test_report.py
```

| Stage | Expected |
|-------|----------|
| P0 | Docker Server; CLI lists `eval` |
| P1 | `pier --help` works |
| P2 | `$WORKDIR/deep-swe/tasks` (clone `https://github.com/datacurve-ai/deep-swe`) |
| P3 | `icode --help` from `/home/Toby/Documents/Toby/iCode-main` |
| P4 | Pier agent `icode` discoverable |
| E3 | `OK report schema` |

**Checkpoint — paste:** last OK line from each stage and `OK report schema`.

---

## Phase 4 — E4–E6 (paid, N=1)

**Where:** this PC. Needs a gitignored `.env` (copy `.env.example`) for `--local` stages, **or** the controller credential when you later run E7. Do not `export` the API key.

```bash
export PATH="$HOME/.local/bin:$PATH"
cd ~/Documents/Toby/mac-k3d
# .env already has DEEPSEEK_API_KEY + DEEPSEEK_MODEL (chmod 600)

mac-k3d eval --stage p5 --n-tasks 1
mac-k3d eval --stage p6 --n-tasks 1
mac-k3d eval --stage p7
mac-k3d eval --stage p8 --n-tasks 1
./scripts/eval/check_report.sh
```

**Expected:**

- P5: `PROGRESS` lines; Pier/Docker/LLM work (minutes, not 4s); `harness/` artifacts. CLI usage errors (`No such option`) **fail** the stage. Other pier/docker errors after a trial starts still count as “stage ran”.
- P6: `baseline/<task>/agent.patch`; `baseline/summary.json` has usage / time / model
- P8: `output/eval-icode-deepseek-deepswe-n1-<utc>.json` with f2p, p2p, `pass_at_1_*`, `token_usage`, `duration_seconds`, `access_date_utc`, `llm_model_id` / `llm_name`
- `check_report.sh` prints `OK report schema`

**Checkpoint — paste:** P5/P6 OK or error tail, JSON path, `OK report schema`.

---

## Phase 5 — E7 Jenkins `icode_eval`

**Where:** this PC (or cloud) with the **controller** URL. **No** `--local`. Build must run on **this** node.

```bash
export PATH="$HOME/.local/bin:$PATH"
cd ~/Documents/Toby/mac-k3d
mac-k3d eval --n-tasks 1 --icode-mode source --model deepseek-v4-pro --yes
```

Or Jenkins UI: job `icode_eval` → Build with Parameters (`DEEPSEEK_MODEL=deepseek-v4-pro`, `AGENT_LABEL=lolbench`, `ICODE_MODE=source`).

**Expected:** build on this worker; archived JSON; same schema as E6. Confirm with `check_report.sh` on the downloaded artifact if needed.

If the job still uses `deepseek-chat`, on the **cloud root** session:

```bash
export PATH="$HOME/.local/bin:$PATH"
mac-k3d config -c ~/.config/mac-k3d/config.yaml --skip-secrets
```

**Checkpoint — paste:** Jenkins build URL / console tail, artifact name, schema OK.

E8 (N>1) is optional after E7.

---

## Progress checklist

| Phase | Command / UI | Pass (y/n) | Notes |
|-------|----------------|------------|-------|
| 0 | root `docker info` + `mac-k3d --help` | | |
| 1 | `02_check_controller.sh` + browser `:17070` | | |
| 2 | `e1` + `03_check_worker.sh` + Nodes online | | |
| 3 | P0–P4 + `check_report.sh` testdata | | |
| 4 | P5–P8 n=1 + named JSON | | |
| 5 | `icode_eval` on this node | | |

---

## Troubleshooting

| Message | What to do |
|---------|------------|
| `permission denied` on `docker.sock` as `y84462643` | Expected. Use **root** on the cloud VM for E0. Do not continue that user. |
| k3d install script writes `/usr/local/bin` | As root this succeeds. As a normal user it fails; this runbook does not use that user. |
| `Host port 8080/8443 in use → using 18xxx` | Pass. Keep Jenkins on **17070**. |
| Jenkins UI not 200 from this PC | Security group **17070**; confirm `02_check` on the VM first. |
| Worker URL is localhost | Wizard default is the cloud URL. If YAML still has localhost, set `controller_url: http://43.107.42.252:17070`, then `mac-k3d config -c worker.yaml`. Restart the agent unit if it was already running. |
| `start is for controller/standalone` | Correct for `worker.yaml`. Use `config`, not `start`. |
| `DEEPSEEK_API_KEY missing` | E4–E6: copy `.env.example` → `.env` (chmod 600). Do not export the key. E7: store `deepseek-api-key` on the **cloud** controller. |
| `No such option: --agent-dir` | Old P5 flags. This tree uses `--agent-import-path icode_pier_agent:ICodeAgent`. Pull/rebuild scripts; do not pass `--agent-dir` on Pier 0.3.1. |
| Job still `deepseek-chat` | On cloud root: `mac-k3d config --skip-secrets` after pulling this tree. |
| P3 `ICODE_SOURCE missing` | Clone/copy iCode to `/home/Toby/Documents/Toby/iCode-main` on **this PC**. |
| pier not found | `uv tool install datacurve-pier` |
| Docker OOM / disk | DeepSWE images are large; free disk; keep N=1. |
