# Binary-initializer testing (Mac + Linux)

This is the **binary-initializer** path (`setup`, Release asset). Full workflow: [workflow.md](workflow.md). The **v0.3.0 / initializer** path (`cargo install`, `prepare` steps 0–7) is [../testing-initializer.md](../testing-initializer.md).

Use this document to **verify** a prebuilt `mac-k3d` on macOS and Linux.

**Linux bootstrap sign-off is Task 0–5 and Task 7.** Task 6 is macOS (later). Task 8 is a destructive blank-machine wizard. Eval pipeline (Task 9 / P1–P8): [testing-eval-pipeline.md](testing-eval-pipeline.md). New-user walkthrough: [binary-initializer-new-machine.md](binary-initializer-new-machine.md) or [clean-machine-binary-test.md](clean-machine-binary-test.md). After setup, run [`scripts/env_set_up/run_all.sh`](../../scripts/env_set_up/run_all.sh).

Related design: [../prepare-wizard.md](../prepare-wizard.md), [../setup.md](../setup.md), [../commands.md](../commands.md).

---

## What this path is for

Turn a new Linux or Mac into a Jenkins **controller** or **worker** by downloading a Release asset. No Rust. The binary installs Docker when you choose **Install**. Jenkins UI: **http://localhost:17070**. Workers do **not** join the controller’s k3d cluster. Worker `start` is rejected.

| Role | Job | How it is achieved |
|------|-----|-------------------|
| **CLI on PATH** | Same binary on macOS and Linux | GitHub pre-release `mac-k3d-{os}-{arch}` or `cargo build --release` |
| **Controller host** | Jenkins in local k3d | `setup -c config.yaml`, role **CI controller** |
| **Worker host** | Jenkins inbound agent | `setup -c worker.yaml` then `config` (not `start`) |
| **Dual config** | One binary, two YAML files, no clobber | `-c config.yaml` vs `-c worker.yaml` |

Constants used below: tag `v0.4.0-rc.4`, repo `~/Documents/Toby/mac-k3d`, `PATH="$HOME/.local/bin:$PATH"`. Prefer a **pre-release** if GitHub Latest is still v0.3.0.

Do not commit Jenkins passwords or API tokens.

---

## Task 0 — unit / build (developer)

Purpose: check the tree compiles, unit tests pass, and the local release binary reports `0.4.x` with `setup` in `--help`.

command:

```bash
cd ~/Documents/Toby/mac-k3d
unset CARGO_TARGET_DIR
export CARGO_TARGET_DIR="$PWD/target"
cargo test
cargo build --release
./target/release/mac-k3d --version
./target/release/mac-k3d --help
```

expected results:

- `cargo test` green (lib + `tests/cli.rs`; help tests mention `setup` and `eval`).
- `cargo build --release` finishes.
- `--version` prints `mac-k3d 0.4.x` (or later).
- `--help` lists `setup`.

---

## Task 1 — Release binary as a user (Linux lab)

Purpose: check a GitHub Release asset runs without Cargo on PATH, `--help` lists `setup`/`eval`, and a TTY `setup` on a throwaway file opens the wizard.

command:

```bash
cd ~/Documents/Toby/mac-k3d
export MAC_K3D_RELEASE_TAG=v0.4.0-rc.4
./scripts/env_set_up/01_download_binary.sh
export PATH="$HOME/.local/bin:$PATH"
hash -r
which mac-k3d
mac-k3d --version
mac-k3d --help

cp ~/.local/bin/mac-k3d /tmp/mac-k3d-user
chmod +x /tmp/mac-k3d-user
env -i HOME="$HOME" USER="$USER" PATH="/usr/bin:/bin:/tmp" /tmp/mac-k3d-user --help

mkdir -p /tmp/mac-k3d-wizard-test
mac-k3d setup -c /tmp/mac-k3d-wizard-test/fresh.yaml
```

expected results:

- `OK downloaded mac-k3d-linux-x86_64 → …/.local/bin/mac-k3d`
- `mac-k3d 0.4.0` (or later 0.4.x)
- `OK help lists setup and eval`
- `which mac-k3d` is `$HOME/.local/bin/mac-k3d` (not `~/.cargo/bin`)
- stripped-PATH `--help` lists `setup` (binary does not need rustc/cargo)
- TTY wizard starts (`mac-k3d prepare — interactive setup`, then **Select base directory**). Press **Ctrl+C**. Do **not** Write / Apply — that throwaway path must not clobber the lab `config.yaml`.

---

## Task 2 — controller setup from binary

Purpose: check `setup -c config.yaml` brings up k3d + Jenkins on **http://localhost:17070**, status is healthy, and jobs exist.

command:

```bash
export PATH="$HOME/.local/bin:$PATH"
mac-k3d setup -c ~/.config/mac-k3d/config.yaml
```

If **Config already exists** (re-test on this lab): choose **Validate existing config only**, then **yes** on **Continue and apply now (start/config)?**. If credentials are offered: **n** unless you have keys ready; empty Enter skips a key. GitCode/GitHub PATs are not required for Task 2.

If this is a first write (no YAML yet), use the wizard table in [clean-machine-binary-test.md](clean-machine-binary-test.md) (role **CI controller**, Harbor/LoLBench skip, Jenkins port **17070**). If host **8080** is taken, **mac-k3d auto-remaps** cluster ports (e.g. to `18080`) and keeps `jenkins.host_port: 17070` unless that port is busy too.

Then verify:

```bash
export PATH="$HOME/.local/bin:$PATH"
cd ~/Documents/Toby/mac-k3d
mac-k3d status -c ~/.config/mac-k3d/config.yaml
curl -sS -o /dev/null -w "%{http_code}\n" http://localhost:17070/login
./scripts/env_set_up/02_check_controller.sh
```

expected results:

- Docker Engine already running; k3d cluster `ci-controller` running (or created).
- Jenkins UI: **http://localhost:17070** (ignore Helm notes about `:8080`).
- Jobs `lolbench_one_task` and `icode_eval` created/updated.
- `=== mac-k3d setup complete ===` with Role `controller`.
- `status`: Docker running, `ci-controller` running, Jenkins pod Running, `http://localhost:17070`.
- `curl` prints `200`.
- `OK controller status healthy`
- `OK Jenkins UI http://localhost:17070/login HTTP 200`
- `OK job lolbench_one_task present`
- `OK job icode_eval present`
- `OK 02_check_controller complete`

If Docker was just installed, a clear **log out, log in, run `mac-k3d setup` again** error is **PASS** for that attempt (re-run after login).

Optional: `mac-k3d config -c ~/.config/mac-k3d/config.yaml --show-jenkins` prints admin user/password for the browser. If it asks to create CI credentials again, choose **n**. Do not commit the password.

---

## Task 3 — worker setup from binary

Purpose: check `setup -c worker.yaml` registers a Jenkins agent (`config` only, not `start`), the systemd unit is active, and `start` on the worker file is rejected.

Prerequisite: Jenkins API token from the UI (**admin** → **Configure** → **API Token**). `api_user` is `admin`. Put token in `worker.yaml` if it is still empty, then `mac-k3d config -c ~/.config/mac-k3d/worker.yaml`.

command:

```bash
export PATH="$HOME/.local/bin:$PATH"
mac-k3d setup -c ~/.config/mac-k3d/worker.yaml
```

If **Config already exists**: **Validate existing config only**, then **yes** on apply. Worker apply runs **config** only.

Then:

```bash
export PATH="$HOME/.local/bin:$PATH"
cd ~/Documents/Toby/mac-k3d
REQUIRE_WORKER=1 ./scripts/env_set_up/03_check_worker.sh
```

Do **not** run `mac-k3d start -c ~/.config/mac-k3d/worker.yaml`.

expected results:

- `Worker: registering Jenkins agent (not starting k3d/Jenkins)…`
- `k3d cluster 'ci-worker' not present` is OK.
- `Jenkins agent systemd user unit started (mac-k3d-jenkins-agent.service)` (or already running).
- Setup complete with Role `worker`, Jenkins UI `http://localhost:17070`.
- `OK worker start rejected (exit 1)`
- `OK worker status role=worker`
- `OK Linux agent unit mac-k3d-jenkins-agent.service active`
- `OK 03_check_worker complete`
- Harbor / LoLBench / uv not required. Optional: Jenkins **Manage Jenkins → Nodes** shows the agent **online**.

---

## Task 4 — full env check + eval-ready (P0)

Purpose: check controller, worker, and eval-ready scripts together, including `mac-k3d eval --stage p0`.

command:

```bash
cd ~/Documents/Toby/mac-k3d
export PATH="$HOME/.local/bin:$PATH"
SKIP_DOWNLOAD=1 RUN_EVAL_SMOKE=1 ./scripts/env_set_up/run_all.sh
```

expected results:

- `OK SKIP_DOWNLOAD=1 — using existing binary` and `mac-k3d 0.4.0`
- `OK 02_check_controller complete`
- `OK 03_check_worker complete`
- `OK eval --stage p0` / `PROGRESS 10% P0 complete`
- `OK 04_check_eval_ready complete`
- `OK ALL CHECKS PASSED`

P0 only checks Docker Server, CLI `eval`, and the agent unit. It does not clone DeepSWE or call an LLM.

---

## Task 5 — isolation / dual YAML

Purpose: check `prepare` / `start` / `config` still work, the two YAML files keep different roles, worker `start` stays rejected, and the controller stays healthy.

command:

```bash
export PATH="$HOME/.local/bin:$PATH"
cd ~/Documents/Toby/mac-k3d

grep -E '^role:' ~/.config/mac-k3d/config.yaml ~/.config/mac-k3d/worker.yaml

mac-k3d prepare --non-interactive -c ~/.config/mac-k3d/config.yaml
mac-k3d prepare --non-interactive -c ~/.config/mac-k3d/worker.yaml

mac-k3d start -c ~/.config/mac-k3d/config.yaml
mac-k3d config -c ~/.config/mac-k3d/config.yaml --skip-secrets

mac-k3d start -c ~/.config/mac-k3d/worker.yaml ; echo "exit=$?"

mac-k3d status -c ~/.config/mac-k3d/config.yaml
```

If `config --skip-secrets` still asks for credentials, choose **n**. Do not re-run the wizard (it overwrites YAML). Do not use another OS account.

expected results:

- `config.yaml:role: controller` and `worker.yaml:role: worker`
- `prepare --non-interactive` exits 0 on both files
- Controller `start` / `config` complete; Jenkins still **http://localhost:17070**
- Worker `start`: `Error: Config("start is for controller/standalone configs. …")` and `exit=1`
- `status`: Docker running, `ci-controller` running, Jenkins pod Running, `http://localhost:17070`

---

## Task 7 — Intel Mac asset (`mac-k3d-darwin-x86_64`)

Purpose: check tag `v0.4.0-rc.4` publishes all four OS/arch assets and `mac-k3d-darwin-x86_64` is Mach-O **x86_64** (not arm64). Runs on Linux; no Intel Mac required for 7a/7b.

The Intel binary is **cross-compiled** on GitHub `macos-latest` (`x86_64-apple-darwin`). It is **not** built on `macos-13`.

command:

```bash
gh release view v0.4.0-rc.4 --repo Toby-Yu/mac-k3d --json assets --jq '.assets[].name'

cd /tmp
gh release download v0.4.0-rc.4 --repo Toby-Yu/mac-k3d --pattern mac-k3d-darwin-x86_64 --clobber
file ./mac-k3d-darwin-x86_64
```

Also open the **Release binaries** workflow for that tag: job **build** / matrix `x86_64-apple-darwin` green; step **Verify Intel Mac asset is x86_64** prints `x86_64` and not `arm64`.

expected results:

- Asset names include all four:
  - `mac-k3d-linux-x86_64`
  - `mac-k3d-linux-aarch64`
  - `mac-k3d-darwin-aarch64`
  - `mac-k3d-darwin-x86_64`
- `file` prints `Mach-O 64-bit x86_64 executable` (reject arm64, ELF, or arm64-only).

Optional Task 7c (Intel Mac or Rosetta): `xattr -d com.apple.quarantine ./mac-k3d-darwin-x86_64`, `chmod +x`, `./mac-k3d-darwin-x86_64 --help` lists `setup` and `eval`. Native Intel is the real 7c.

---

## Remaining (not required to sign off Linux bootstrap)

### Task 6 — macOS (checklist)

Purpose: check the Apple Silicon Release asset runs in Terminal.app, the same wizard works, and a Mac worker uses LaunchAgent `com.mac-k3d.jenkins-agent`. Double-click of an unsigned binary is **not** supported. Mac can be signed off later than Linux.

command:

```bash
# download mac-k3d-darwin-aarch64 from tag v0.4.0-rc.4
xattr -d com.apple.quarantine ./mac-k3d-darwin-aarch64
chmod +x ./mac-k3d-darwin-aarch64
./mac-k3d-darwin-aarch64 --help

./mac-k3d-darwin-aarch64 setup -c ~/.config/mac-k3d/config.yaml
# or worker: setup -c ~/.config/mac-k3d/worker.yaml
#   Jenkins URL http://<linux-controller-ip>:17070
```

If Docker is missing: choose **Install**, then open **Docker Desktop** once and wait until it is idle.

expected results:

- `--help` lists `setup` and `eval`.
- Same wizard as Linux (controller or worker).
- Worker after token + `config`: LaunchAgent `com.mac-k3d.jenkins-agent`; node **online** in Jenkins.

```bash
launchctl print "gui/$(id -u)/com.mac-k3d.jenkins-agent" 2>&1 | head -20
```

---

### Task 8 — blank-machine wizard (destructive)

Purpose: check a **first-run** wizard (role / Install Docker) on a wiped PC or throwaway config — not the lab `~/.config/mac-k3d/config.yaml`. Re-run wizard on the lab overwrites YAML and can put host port `8080` back (bind conflict).

command:

On a **wiped** machine follow [clean-machine-binary-test.md](clean-machine-binary-test.md). To only prove prompts without applying:

```bash
export PATH="$HOME/.local/bin:$PATH"
mkdir -p /tmp/mac-k3d-wizard-test
mac-k3d setup -c /tmp/mac-k3d-wizard-test/fresh.yaml
# Ctrl+C before Write / apply now
```

Full apply (wiped PC only):

```bash
export PATH="$HOME/.local/bin:$PATH"
mac-k3d setup -c ~/.config/mac-k3d/config.yaml
```

expected results (choose):

| Prompt | Choose |
|--------|--------|
| Role | **CI controller (Jenkins in k3d)** or **CI worker** |
| Docker / k3d / kubectl / helm | Use existing or **Install** |
| Harbor / LoLBench | **Skip** / **No** |
| Jenkins UI host port (controller) | **17070** |
| Jenkins URL (worker) | `http://localhost:17070` or `http://<controller-ip>:17070` |
| CI secrets | **yes** if you have `deepseek-api-key`; else skip |
| Write + apply | **yes** only on a wiped PC |

Do **not** pick “Local development only” if you need Jenkins. After apply, reuse Task 2 / Task 3 verify scripts.

---

### Task 9 — eval pipeline P1–P8

Purpose: check Process 2 (Pier, DeepSWE, iCode vs DeepSeek). **P0 is already covered in Task 4.** Details: [testing-eval-pipeline.md](testing-eval-pipeline.md). LLM stages need Jenkins credential **`deepseek-api-key`**.

command:

```bash
export PATH="$HOME/.local/bin:$PATH"
cd ~/Documents/Toby/mac-k3d

mac-k3d eval --stage p1
mac-k3d eval --stage p2
ICODE_MODE=source ICODE_SOURCE=/home/Toby/Documents/Toby/iCode-main mac-k3d eval --stage p3
mac-k3d eval --stage p4
mac-k3d eval --stage p5 --n-tasks 1
mac-k3d eval --stage p6 --n-tasks 1
mac-k3d eval --stage p7
mac-k3d eval --stage p8 --n-tasks 1
```

Then once:

```bash
mac-k3d eval --n-tasks 1 --icode-mode source
# or local: mac-k3d eval --local --n-tasks 1
```

expected results:

- **P1:** `pier --help` works (`uv tool install datacurve-pier` if missing).
- **P2:** `$WORKDIR/deep-swe/tasks` exists (clone `https://github.com/datacurve-ai/deep-swe`).
- **P3:** `icode --help` succeeds (source path above, or binary tarball via `ICODE_MODE=binary`).
- **P4:** Pier agent `icode` install script is executable.
- **P5:** `PROGRESS` lines; one DeepSWE task through iCode (`--n-tasks 1`). Clear pier/docker errors still count as “stage ran”.
- **P6:** DeepSeek API baseline on the same `instruction.md`.
- **P7:** temp JSON with `harness_resolved` / `baseline_resolved` / `f2p` / `p2p`.
- **P8:** `output/eval-icode-deepseek-deepswe-n1-<utc>.json` (or under `$WORKDIR/output/`).
- Full runner: Jenkins job `icode_eval` (or `--local`) runs both arms and archives JSON.

P5+ costs time, disk, and API usage. GitHub/GitCode PATs are only needed for **private** iCode clone or tarball.

---

## Linux lab (2026-09-11, `mac-k3d 0.4.0` / tag `v0.4.0-rc.4`)

| Check | Result |
|-------|--------|
| Task 0 | `cargo test` green; `cargo build --release`; `--help` lists `setup` |
| Task 1 | `01_download_binary.sh`; `/tmp/mac-k3d-user --help` lists `setup` without cargo PATH; TTY wizard on throwaway YAML (Ctrl+C) |
| Task 2 | `ci-controller` running; Jenkins `http://localhost:17070` HTTP 200; `OK 02_check_controller complete` |
| Task 3 | `start -c worker.yaml` rejected; `mac-k3d-jenkins-agent.service` active; `OK 03_check_worker complete` |
| Task 4 | `SKIP_DOWNLOAD=1 RUN_EVAL_SMOKE=1 ./scripts/env_set_up/run_all.sh` → `OK ALL CHECKS PASSED`; P0 complete |
| Task 5 | dual YAML; worker `start` `exit=1`; controller `status` healthy |
| Task 6 | macOS later |
| Task 7 | four Release assets; `file` → Mach-O 64-bit x86_64 executable (Actions job green still confirm on GitHub) |
| Task 8 | blank-machine wizard not run on this lab |
| Task 9 | P1–P8 not run (P0 via Task 4) |

---

## Troubleshooting

| Message | What to do |
|---------|------------|
| `permission denied` on `docker.sock` / log out for docker | Linux: **log out and in**, then `mac-k3d setup` again |
| `docker info` fails / no Server | Linux: docker group + new login. macOS: open **Docker Desktop** and wait until it is idle |
| `agent.jar` / curl port 17070 / need token | Finish controller `start`, paste API token, then worker `config` |
| `start is for controller/standalone` | Expected on `worker.yaml`. Use `mac-k3d config -c worker.yaml` |
| `failed to bind host port … 8080` | Prefer a build that auto-remaps on `setup`/`start`. Last resort: set free `cluster.ports` hosts; keep Jenkins on `17070` unless remapped |
| Release missing `darwin-x86_64` | Tag was cut **before** the `macos-latest` cross-compile workflow. Push this branch, cut a **new** `v*` tag (or upload the CI artifact onto the old release). See **Task 7**. |

User commands: [binary-initializer-new-machine.md](binary-initializer-new-machine.md).  
Eval pipeline stages: [testing-eval-pipeline.md](testing-eval-pipeline.md).
