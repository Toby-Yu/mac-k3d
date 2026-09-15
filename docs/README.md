# mac-k3d Design Documents

| Document | Description |
|----------|-------------|
| [architecture.md](architecture.md) | System overview, components, data flow |
| [commands.md](commands.md) | CLI commands, flags, and behavior |
| [configuration.md](configuration.md) | Config file schema and state layout |
| [deployment.md](deployment.md) | Single-Mac and multi-Mac topology, including physical LAN cabling |
| [setup.md](setup.md) | Step-by-step setup for single- and multi-Mac environments |
| [binary-initializer/user-guide.md](binary-initializer/user-guide.md) | **Start here:** cloud controller, new Mac/Linux worker, iCode drop, eval commands |
| [binary-initializer/workflow.md](binary-initializer/workflow.md) | End-to-end: new machine → controller/worker → iCode eval → JSON |
| [binary-initializer/testing/binary-initializer-new-machine.md](binary-initializer/testing/binary-initializer-new-machine.md) | User guide: download Release binary, run setup; Docker installed by the binary |
| [binary-initializer/testing/testing-binary-initializer.md](binary-initializer/testing/testing-binary-initializer.md) | Sign-off Task 0–5 + Task 7 on Linux; Task 6 macOS later |
| [binary-initializer/testing/cloud-eval-runbook.md](binary-initializer/testing/cloud-eval-runbook.md) | Operator runbook: cloud root controller → this PC worker → E0–E7 JSON |
| [binary-initializer/testing/testing-eval-pipeline.md](binary-initializer/testing/testing-eval-pipeline.md) | Cloud+worker eval: E0–E8 tracking, P0–P8 stages, report schema |
| [initializer-new-machine.md](initializer-new-machine.md) | v0.3.0 user guide: `cargo install` + `prepare` on a blank Linux or Mac |
| [prepare-wizard.md](prepare-wizard.md) | Interactive `prepare` questionnaire design |
| [testing-initializer.md](testing-initializer.md) | v0.3.0 initializer tests (Mac + Linux), steps 0–7 |
| [lolbench-jenkins.md](lolbench-jenkins.md) | LoLBench Harbor job design (`lolbench_one_task`; DeepSWE eval is `icode_eval`) |
| [secrets.md](secrets.md) | CI secrets: configure once on Jenkins controller, use on all agents |

## Goals

1. **Single entry point** — One CLI to manage Docker, k3d, and optional Jenkins on **macOS and Linux**.
2. **Idempotent operations** — Safe to re-run `prepare`, `start`, and `config`.
3. **Sensible defaults** — Works out of the box; config file overrides when needed.
4. **Clear lifecycle** — `setup` (wizard + apply) or prepare → start (controller only) → config → teardown → clean.
5. **Platform adapters** — macOS (Docker Desktop, Homebrew, LaunchAgent) vs Linux (Docker Engine, apt, systemd --user).

## Non-goals (v1)

- Windows support
- Production-grade Jenkins hardening (LDAP, backup, HA)
- Replacing Helm/k3d/kubectl — we orchestrate them, not reimplement them
- In-cluster application deployment beyond Jenkins
- Auto-install package managers on non-Debian Linux (use existing binaries / paths)

## Status

Implemented CLI with prepare wizard and lifecycle commands. Linux support uses `src/platform/{macos,linux}.rs` adapters.
