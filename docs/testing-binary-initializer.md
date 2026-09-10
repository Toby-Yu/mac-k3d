# Binary-initializer testing (Mac + Linux)

This is the **binary-initializer** path (`setup`, Release asset). The **v0.3.0 / initializer** path (`cargo install`, `prepare` steps 0–7) is [testing-initializer.md](testing-initializer.md).

Use this document to **verify** a prebuilt `mac-k3d` on macOS and Linux.

**Sign-off is T0–T3 and T5–T6 only.** If you are a **new user** bringing up a blank computer, use [binary-initializer-new-machine.md](binary-initializer-new-machine.md) instead.

Related design: [prepare-wizard.md](prepare-wizard.md), [setup.md](setup.md), [commands.md](commands.md).

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
        →  controller: k3d + Jenkins :9080
           worker: Java agent connected to Jenkins URL
```

Workers do **not** join the controller’s k3d cluster. Worker `start` is rejected.

Until a `v*` pre-release publishes assets, testers use `target/release/mac-k3d` as a stand-in.

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

**Expected:** start succeeds; k3d up; Jenkins pod Running; UI `http://localhost:9080`; `mac-k3d status` healthy. If Docker was just installed, a clear **log out, log in, run `mac-k3d setup` again** error is **PASS** for this attempt (re-run after login).

## T3 — worker setup from binary

Role **CI worker**, Jenkins URL `:9080`, apply now (`config` only; **not** `start`).

**Expected:** agent unit **or** a clear “need token” / “log out for docker” message. No LoLBench clone required; Harbor/uv **not** required. When a token is present: Linux `mac-k3d-jenkins-agent.service` active (linger enabled) or macOS LaunchAgent `com.mac-k3d.jenkins-agent`; node **online** in Jenkins. `start -c worker.yaml` is rejected.

## T5 — isolation / non-regression

`prepare` / `start` / `config` still work. Dual-role one-PC: `-c config.yaml` vs `-c worker.yaml`. Do not touch other OS accounts. Worker `start` still rejected.

## T6 — macOS (checklist)

Download `mac-k3d-darwin-aarch64` (or x86_64), `xattr -d com.apple.quarantine`, run in Terminal.app. Let setup install Docker Desktop if asked; open the first GUI window.

**Expected:** same wizard; LaunchAgent `com.mac-k3d.jenkins-agent` after token + `config`; Jenkins node online. Double-click of an unsigned binary is **not** supported. Mac can be signed off later than Linux.

---

## Linux lab (2026-09-10, `mac-k3d 0.4.0`)

| Check | Result |
|-------|--------|
| T0 | `cargo test` 42 passed; `cargo build --release`; help lists `setup` |
| T1 | `/tmp/mac-k3d-user --help` lists `setup` without cargo PATH |
| T2 | `ci-controller` running; Jenkins `http://localhost:9080` HTTP 200 |
| T3 | `start -c worker.yaml` rejected; `mac-k3d-jenkins-agent.service` active |
| T5 | dual YAML; worker `start` rejected; controller `status` healthy |
| T6 | macOS later |

---

## Troubleshooting

| Message | What to do |
|---------|------------|
| `permission denied` on `docker.sock` / log out for docker | Linux: **log out and in**, then `mac-k3d setup` again |
| `docker info` fails / no Server | Linux: docker group + new login. macOS: open **Docker Desktop** and wait until it is idle |
| `agent.jar` / curl port 9080 / need token | Finish controller `start`, paste API token, then worker `config` |
| `start is for controller/standalone` | Expected on `worker.yaml`. Use `mac-k3d config -c worker.yaml` |
| `failed to bind host port … 8080` | In controller YAML set host port `8080` → `18080`; keep Jenkins on `9080` |

User commands: [binary-initializer-new-machine.md](binary-initializer-new-machine.md).
