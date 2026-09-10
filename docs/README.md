# mac-k3d Design Documents

| Document | Description |
|----------|-------------|
| [architecture.md](architecture.md) | System overview, components, data flow |
| [commands.md](commands.md) | CLI commands, flags, and behavior |
| [configuration.md](configuration.md) | Config file schema and state layout |
| [deployment.md](deployment.md) | Single-Mac and multi-Mac topology, including physical LAN cabling |
| [setup.md](setup.md) | Step-by-step setup for single- and multi-Mac environments |
| [initializer-new-machine.md](initializer-new-machine.md) | User guide: download a Release binary, run setup; Docker is installed by the binary |
| [prepare-wizard.md](prepare-wizard.md) | Interactive `prepare` questionnaire design |
| [testing-initializer.md](testing-initializer.md) | Sign-off T0–T3 / T5–T6 (machine bootstrap). iCode I0–I8 is a separate doc |
| [icode-ci-new-machine.md](icode-ci-new-machine.md) | User guide: Jenkins iCode evals (`EVAL_MODE=binary` or `source`) |
| [testing-icode-ci.md](testing-icode-ci.md) | Pass/fail checks for iCode Jenkins job (binary + source) |
| [lolbench-jenkins.md](lolbench-jenkins.md) | Short pointer: `lolbench_one_task` + `EVAL_MODE` |
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
