# Initialize a new Linux or Mac computer

**Users:** follow the command lists below to set up a blank machine.

**Developers:** pass/fail checks live in [testing-initializer.md](testing-initializer.md). This page does not describe how to verify the initializer.

One CLI (`mac-k3d`) on Linux and macOS.

| You want this machine to… | Follow |
|---------------------------|--------|
| Host Jenkins (UI + job queue) | [A. Controller](#a-controller--host-jenkins) |
| Run Jenkins agent jobs (iCode evals) | [B. Worker](#b-worker--jenkins-agent), then [icode-ci-new-machine.md](icode-ci-new-machine.md) |
| Do both on **this** PC | [C. One PC, both roles](#c-one-pc-both-roles) |

Workers are Jenkins **agents**. They do not join the controller’s Kubernetes cluster.

```text
Controller:  Docker → k3d → Jenkins  (usually http://HOST:9080)
Worker:      Docker + Java + agent  → that Jenkins URL
```

---

## Before any role

Run once on a new computer. Then go to **A** or **B**. **You do not need Rust.**

| Step | Linux | macOS |
|------|--------|--------|
| Docker | Engine (`docker.io`) + `docker` group | **Docker Desktop** — install and **open** the app |
| Keep Jenkins agent running after logout | `loginctl enable-linger "$USER"` | **Skip** — `mac-k3d config` installs a LaunchAgent |

Do **not** run `loginctl` or `usermod` on a Mac. Those are Linux-only.

### Linux

```bash
sudo apt-get update
sudo apt-get install -y docker.io
sudo usermod -aG docker "$USER"
# log out of the desktop completely, log back in, open a new terminal
loginctl enable-linger "$USER"
docker info   # must show a Server section
```

### macOS

```bash
brew install --cask docker
# or install from https://www.docker.com/products/docker-desktop/
open -a Docker
docker info   # must succeed before mac-k3d start / worker setup
```

If Homebrew is missing, install it from [https://brew.sh](https://brew.sh) or download Docker Desktop from Docker’s site.

### Then install the CLI (both OS)

Download the matching GitHub Release asset (`mac-k3d-linux-x86_64`, `mac-k3d-linux-aarch64`, `mac-k3d-darwin-x86_64`, or `mac-k3d-darwin-aarch64`). Open **Terminal** (double-click is not supported).

```bash
chmod +x ./mac-k3d-linux-x86_64
# macOS unsigned download:
#   xattr -d com.apple.quarantine ./mac-k3d-darwin-aarch64
mkdir -p ~/.local/bin
cp ./mac-k3d-linux-x86_64 ~/.local/bin/mac-k3d
export PATH="$HOME/.local/bin:$PATH"
which mac-k3d
mac-k3d --help    # must list `setup`
```

First-run (TTY): `mac-k3d` or `mac-k3d setup` starts the wizard, then offers to apply start/config.

Use **Use this installation** in the wizard when a tool is already found. Let the wizard install the rest (k3d, kubectl, helm, Java). Harbor/LoLBench is optional on workers.

Do **not** pick “Local development only” if you need Jenkins or an agent.

Developers building from git still use `cargo install --path .` (see [testing-initializer.md](testing-initializer.md)).

---

## A. Controller — host Jenkins

Storage: about **60 GB** free. Jenkins UI port: **9080**.

If another program already uses **8080**, after prepare edit `~/.config/mac-k3d/config.yaml` and change `cluster.ports` `host: 8080` to e.g. `18080`. Leave `jenkins.host_port: 9080`.

```bash
# 1. Write controller config and apply (wizard → start → config)
mac-k3d setup -c ~/.config/mac-k3d/config.yaml

# Power-user split:
# mac-k3d prepare -i -c ~/.config/mac-k3d/config.yaml
# mac-k3d start -c ~/.config/mac-k3d/config.yaml
# mac-k3d config -c ~/.config/mac-k3d/config.yaml --show-jenkins
```

Open **http://localhost:9080** (or `http://<this-machine>:9080`). Username **admin**. Password from `mac-k3d config --show-jenkins` if `setup` already printed it. Do not commit the password.

**Wizard (step 1) — choose:**

| Prompt | Choose |
|--------|--------|
| Base directory | Recommended path with enough space |
| Role | **CI controller (Jenkins in k3d)** |
| Docker / k3d / kubectl / **helm** | Use existing, or install |
| Harbor / LoLBench | Skip (not required) |
| Cluster name | Default is fine |
| k3d agent nodes | `0` |
| Jenkins UI host port | `9080` |
| Job defaults | `ruff_1` / `EVAL_MODE=binary`; empty `ICODE_RELEASE` is fine (set at build time). iCode job: [icode-ci-new-machine.md](icode-ci-new-machine.md) |
| Enter CI secrets now? | **no** unless you already have keys ([secrets.md](secrets.md)) |
| Write configuration? | **yes** |

Do **not** use `worker.yaml` with `start`. `start` is for the controller file only.

---

## B. Worker — Jenkins agent

Storage: about **100 GB** free. You need the Jenkins **base URL** (another machine, or this PC after **A**).

The worker **node** is created in Jenkins and the local agent process starts only after Jenkins is up **and** you put an API token in the worker config. That is not `mac-k3d start` on `worker.yaml`.

### 1. Write worker config (wizard)

```bash
mac-k3d setup -c ~/.config/mac-k3d/worker.yaml
# or: mac-k3d prepare -i -c ~/.config/mac-k3d/worker.yaml
```

If Jenkins is not up yet, `agent.jar` may fail (`curl` to port **9080**). That is OK. Config still saves. Finish **A**, then continue below.

**Wizard — choose:**

| Prompt | Choose |
|--------|--------|
| Base directory | Recommended path with enough space |
| Role | **CI worker (Jenkins agent only)** |
| Docker / java | Use existing, or install |
| k3d / kubectl | **Skip** unless you want a local cluster |
| Harbor / LoLBench | **No** (optional only for oracle/debug) |
| Add `~/.local/bin` to PATH | **yes** if asked |
| Jenkins controller URL | e.g. `http://192.168.1.10:9080` or `http://localhost:9080` on this PC |
| API user / token | Fill in if Jenkins is already up; **empty** if not |
| Agent name / labels / remote root | Defaults are fine |
| Write configuration? | **yes** |

### 2. Jenkins API token (after Jenkins is up)

The `--show-jenkins` **password** is only for the browser. It is **not** the API token.

1. Open the Jenkins URL (`http://localhost:9080` on this PC).
2. Log in as **admin** + password from `mac-k3d config -c ~/.config/mac-k3d/config.yaml --show-jenkins` (controller file).
3. Click **admin** (top right) → **Configure**.
4. **API Token** → **Add new Token** → name it (e.g. `mac-k3d-worker`) → **Generate**.
5. **Copy the token now.** Jenkins will not show it again.

`api_user` is `admin`. This token is only for `mac-k3d` to register the agent. It does not call paid LLM APIs. Do not commit it.

### 3. Put the token in `worker.yaml`

Edit `~/.config/mac-k3d/worker.yaml`. Under `jenkins_agent:`, change only:

```yaml
  api_user: admin
  api_token: PASTE_THE_TOKEN_HERE
```

Leave `controller_url` as the Jenkins URL (this PC: `http://localhost:9080`).

If you use `nano`:

```bash
nano ~/.config/mac-k3d/worker.yaml
```

**Ctrl+O**, **Enter** to save, **Ctrl+X** to quit. Quote the token if it contains `#` or spaces: `api_token: "..."`.

Or re-run step 1 and paste **admin** + token when the wizard asks.

### 4. Register the node and start the agent

```bash
mac-k3d config -c ~/.config/mac-k3d/worker.yaml
```

Use the **worker** file, not `config.yaml`. Expect: download `agent.jar`, register the node, write `launch-agent.sh`, create `CPU_CORES` locks, start the OS agent service. A line like `k3d cluster 'ci-worker' not present` is **OK** — workers are not a k3d cluster.

**Linux**

```bash
loginctl enable-linger "$USER"
systemctl --user status mac-k3d-jenkins-agent.service
```

Want **`Active: active (running)`**. If `status` pauses, press **`q`**.

**macOS** — no `loginctl`. `config` installs LaunchAgent `com.mac-k3d.jenkins-agent`:

```bash
launchctl print "gui/$(id -u)/com.mac-k3d.jenkins-agent" 2>&1 | head -20
```

### 5. Confirm the node in Jenkins

In the Jenkins UI: **Manage Jenkins → Nodes** (or **Build Executor Status** on the left). The agent name from YAML (for example `mac-Michael-Ubuntu`) should be **online**.

Then this machine can take queued jobs (`lolbench_one_task`) when an executor and locks are free.

Optional: after the node is online, run an iCode job — [icode-ci-new-machine.md](icode-ci-new-machine.md). Do **not** `uv sync` iCode on this machine at prepare time.

Do **not** run `mac-k3d start -c ~/.config/mac-k3d/worker.yaml` to start the agent. The agent is `setup`/`prepare` + token + `config`.

### Add another worker (Nth machine)

One **controller**. Each new Mac or Linux box is another **worker**. They do not join the controller’s k3d cluster.

On the new computer:

1. Docker (Engine + `docker` group + linger on Linux; Docker Desktop on macOS) — [Before any role](#before-any-role).
2. Download the matching Release asset (`mac-k3d-linux-x86_64`, `mac-k3d-linux-aarch64`, `mac-k3d-darwin-x86_64`, or `mac-k3d-darwin-aarch64`). No Rust.
3. `mac-k3d setup -c ~/.config/mac-k3d/worker.yaml` — role **CI worker**; Harbor/LoLBench **No**; Jenkins URL `http://<controller-ip>:9080`.
4. Use a **distinct** agent name (the wizard default includes the hostname).
5. Create or reuse a Jenkins API token; put `api_user` / `api_token` in that machine’s `worker.yaml`.
6. `mac-k3d config -c ~/.config/mac-k3d/worker.yaml` — node **online** in Jenkins with label `lolbench`.

Then the controller can queue `lolbench_one_task` onto any idle worker. iCode (pier, LLM, patch, f2p/p2p) runs **inside the job**, not in this binary — [icode-ci-new-machine.md](icode-ci-new-machine.md).

---

## C. One PC, both roles

Two files: `config.yaml` (controller) and `worker.yaml` (worker). Run in this order. After Jenkins is up, create the node with **B** steps 2–5 (token, `worker.yaml`, worker `config`, Nodes UI).

```bash
# 1. Before any role (Linux: docker.io + linger; macOS: Docker Desktop; then download mac-k3d binary)

# 2. Controller
mac-k3d setup -c ~/.config/mac-k3d/config.yaml
# log in at http://localhost:9080  (admin + printed password)

# 3. Worker (URL http://localhost:9080)
mac-k3d setup -c ~/.config/mac-k3d/worker.yaml
# put api_user / api_token in worker.yaml if prompted/empty, then:
mac-k3d config -c ~/.config/mac-k3d/worker.yaml
```

---

## Stop or remove (optional)

```bash
mac-k3d teardown -c ~/.config/mac-k3d/config.yaml
mac-k3d clean -c ~/.config/mac-k3d/worker.yaml --yes
mac-k3d clean -c ~/.config/mac-k3d/config.yaml --yes
```

---

## If a command fails

| Message | What to do |
|---------|------------|
| `mac-k3d: command not found` | Copy the release binary to `~/.local/bin/mac-k3d` and add that dir to PATH |
| `permission denied` on `docker.sock` | Linux only: finish `usermod`, **log out and in**; if it still fails, `systemctl --user exit` then sign in |
| `docker info` fails / no Server | Linux: docker group + new login. macOS: open **Docker Desktop** and wait until it is idle |
| `loginctl: command not found` | macOS — ignore; use LaunchAgent after worker `config` |
| `agent.jar` / curl port 9080 | Finish controller `start`, then worker `config` |
| `REPLACE_ME` / unit not started | Token missing or still `api_token: null`; edit `worker.yaml` and re-run worker `config` |
| `mac-k3d-jenkins-agent.service` could not be found | You ran `config` on **controller** YAML, or Jenkins was down / no token. Use `-c worker.yaml` after step 2 |
| `failed to bind host port … 8080` | In controller YAML set host port `8080` → `18080`; keep Jenkins on `9080` |
