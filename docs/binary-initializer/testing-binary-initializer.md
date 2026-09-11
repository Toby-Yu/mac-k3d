# Binary-initializer testing (Mac + Linux)

This is the **binary-initializer** path (`setup`, Release asset). Full workflow: [workflow.md](workflow.md). The **v0.3.0 / initializer** path (`cargo install`, `prepare` steps 0–7) is [../testing-initializer.md](../testing-initializer.md).

Use this document to **verify** a prebuilt `mac-k3d` on macOS and Linux.

**Bootstrap sign-off is T0–T3 and T5–T6 only.** Eval pipeline stages: [testing-eval-pipeline.md](testing-eval-pipeline.md). If you are a **new user** bringing up a blank computer, use [binary-initializer-new-machine.md](binary-initializer-new-machine.md) or [clean-machine-binary-test.md](clean-machine-binary-test.md). After setup, run [`scripts/env_set_up/run_all.sh`](../../scripts/env_set_up/run_all.sh).

Related design: [../prepare-wizard.md](../prepare-wizard.md), [../setup.md](../setup.md), [../commands.md](../commands.md).

---

## What this path is for

Turn a new Linux or Mac into a Jenkins **controller** or **worker** by downloading a Release asset. No Rust. The binary installs Docker when you choose **Install**.

| Role | Job | How it is achieved |
|------|-----|-------------------|
| **CLI on PATH** | Same binary on macOS and Linux | GitHub pre-release `mac-k3d-{os}-{arch}` or `cargo build --release` |
| **Controller host** | Jenkins in local k3d | `setup -c config.yaml`, role **CI controller** |
| **Worker host** | Jenkins inbound agent | `setup -c worker.yaml` then `config` (not `start`) |
| **Dual config** | One binary, two YAML files, no clobber | `-c config.yaml` vs `-c worker.yaml` |

```text
GitHub Release  mac-k3d-{os}-{arch}
        →  chmod +x && ./mac-k3d   (or setup)
        →  wizard: controller | worker
        →  binary installs Docker + the rest
        →  controller: k3d + Jenkins :17070
           worker: Java agent connected to Jenkins URL
```

Workers do **not** join the controller’s k3d cluster. Worker `start` is rejected.

Until a `v*` pre-release publishes the four assets (`linux-x86_64`, `linux-aarch64`, `darwin-aarch64`, `darwin-x86_64`), testers use `target/release/mac-k3d` as a stand-in. Intel Mac is **cross-compiled** on `macos-latest` (see **T7**).

---

## T0 — unit / build (developer)

```bash
cd /path/to/mac-k3d
cargo test
cargo build --release
./target/release/mac-k3d --version
./target/release/mac-k3d --help   # must list setup
```

**Expected:** tests green; version `mac-k3d 0.4.x` (or later); help includes `setup`.

## T1 — binary as a user (Linux lab)

Copy the release binary (or `target/release/mac-k3d`) to a PATH **without** `~/.cargo/bin`:

```bash
cp target/release/mac-k3d /tmp/mac-k3d-user
chmod +x /tmp/mac-k3d-user
env -i HOME="$HOME" USER="$USER" PATH="/usr/bin:/bin:/tmp" /tmp/mac-k3d-user --help
```

After a GitHub pre-release exists, repeat with the downloaded `mac-k3d-linux-x86_64` asset.

**Expected:** help lists `setup`; no rustc/cargo required. A TTY `./mac-k3d` starts the wizard (Controller / Worker / standalone).

## T2 — controller setup from binary

Role **CI controller**, apply now (`setup` installs Docker if needed).

**Expected:** start succeeds; k3d up; Jenkins pod Running; UI `http://localhost:17070`; `mac-k3d status` healthy. If Docker was just installed, a clear **log out, log in, run `mac-k3d setup` again** error is **PASS** for this attempt (re-run after login).

## T3 — worker setup from binary

Role **CI worker**, Jenkins URL `:17070`, apply now (`config` only; **not** `start`).

**Expected:** agent unit **or** a clear “need token” / “log out for docker” message. No LoLBench clone required; Harbor/uv **not** required. When a token is present: Linux `mac-k3d-jenkins-agent.service` active (linger enabled) or macOS LaunchAgent `com.mac-k3d.jenkins-agent`; node **online** in Jenkins. `start -c worker.yaml` is rejected.

## T5 — isolation / non-regression

`prepare` / `start` / `config` still work. Dual-role one-PC: `-c config.yaml` vs `-c worker.yaml`. Do not touch other OS accounts. Worker `start` still rejected.

## T6 — macOS (checklist)

Download `mac-k3d-darwin-aarch64`, `xattr -d com.apple.quarantine`, run in Terminal.app. Let setup install Docker Desktop if asked; open the first GUI window.

**Expected:** same wizard; LaunchAgent `com.mac-k3d.jenkins-agent` after token + `config`; Jenkins node online. Double-click of an unsigned binary is **not** supported. Mac can be signed off later than Linux.

## T7 — Intel Mac asset (`mac-k3d-darwin-x86_64`)

The Intel binary is **cross-compiled** on GitHub `macos-latest` (`x86_64-apple-darwin`). It is **not** built on `macos-13`.

### T7a — CI log (no Intel Mac required)

On the **Release binaries** run for the tag:

1. Job **build** / matrix `x86_64-apple-darwin` is green.
2. Step **Verify Intel Mac asset is x86_64** prints `file` / `lipo -info` containing **x86_64** and not **arm64**.
3. Job **publish** lists four files; Releases page has `mac-k3d-darwin-x86_64`.

```bash
# after gh auth login
gh release view <tag> --json assets --jq '.assets[].name'
# must include mac-k3d-darwin-x86_64
```

### T7b — downloaded file (any machine)

```bash
file ./mac-k3d-darwin-x86_64
# expect: Mach-O 64-bit executable x86_64
# reject: arm64, ELF, or a fat binary that is only arm64
```

On a Mac you can also run `lipo -info ./mac-k3d-darwin-x86_64` (`Non-fat file: … architecture: x86_64`).

### T7c — Intel Mac smoke (optional)

```bash
xattr -d com.apple.quarantine ./mac-k3d-darwin-x86_64
chmod +x ./mac-k3d-darwin-x86_64
./mac-k3d-darwin-x86_64 --help    # must list setup and eval
```

Apple Silicon can run this via Rosetta; that is optional. Native Intel is the real T7c.

---

## Linux lab (2026-09-10, `mac-k3d 0.4.0`)

| Check | Result |
|-------|--------|
| T0 | `cargo test` 42 passed; `cargo build --release`; help lists `setup` |
| T1 | `/tmp/mac-k3d-user --help` lists `setup` without cargo PATH |
| T2 | `ci-controller` running; Jenkins `http://localhost:17070` HTTP 200 |
| T3 | `start -c worker.yaml` rejected; `mac-k3d-jenkins-agent.service` active |
| T5 | dual YAML; worker `start` rejected; controller `status` healthy |
| T6 | macOS later |

---

## Troubleshooting

| Message | What to do |
|---------|------------|
| `permission denied` on `docker.sock` / log out for docker | Linux: **log out and in**, then `mac-k3d setup` again |
| `docker info` fails / no Server | Linux: docker group + new login. macOS: open **Docker Desktop** and wait until it is idle |
| `agent.jar` / curl port 17070 / need token | Finish controller `start`, paste API token, then worker `config` |
| `start is for controller/standalone` | Expected on `worker.yaml`. Use `mac-k3d config -c worker.yaml` |
| `failed to bind host port … 8080` | In controller YAML set host port `8080` → `18080`; keep Jenkins on `17070` |
| Release missing `darwin-x86_64` | Tag was cut **before** the `macos-latest` cross-compile workflow. Push this branch, cut a **new** `v*` tag (or upload the CI artifact onto the old release). See **T7**. |

User commands: [binary-initializer-new-machine.md](binary-initializer-new-machine.md).  
Eval pipeline stages: [testing-eval-pipeline.md](testing-eval-pipeline.md).
