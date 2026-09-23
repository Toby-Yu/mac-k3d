# mac-k3d documentation

**Users start here:** [user-guide.md](user-guide.md).

Do not add a new top-level folder for each feature branch. Update these product pages, or if the page is **only** for that branch, add `docs/<branch>/README.md` + `testing.md`. A pre-commit hook (`scripts/check_docs.sh`) enforces layout, start-here (no v0.3 links from the repo README or user-guide), and the **Releases** inventory below.

## Users (current product)

| Document | Description |
|----------|-------------|
| [user-guide.md](user-guide.md) | Cloud controller, worker, iCode drop, Jenkins eval |
| [new-machine.md](new-machine.md) | Download the Release binary; `setup` installs Docker |
| [icode-harness-inputs.md](icode-harness-inputs.md) | Jenkins upload vs git clone (`branch` / `tag` / `commit`) |
| [workflow.md](workflow.md) | End-to-end: new machine → controller/worker → JSON |
| [export-import.md](export-import.md) | Worker vs controller YAML; `--force` vs `--skip-secrets` |
| [secrets.md](secrets.md) | Jenkins credentials on the controller; local `.env` for `--local` |
| [lolbench-jenkins.md](lolbench-jenkins.md) | `deepswe_one_task`, `lolbench_one_task`, and `swebenchpro_one_task` (all Harbor) |

## Lab runbooks (this team, not the user start-here)

Copy-paste checklists with this lab’s IPs live under [testing/](testing/).

| Document | Description |
|----------|-------------|
| [testing/README.md](testing/README.md) | What belongs in `testing/` |
| [testing/testing-eval-pipeline.md](testing/testing-eval-pipeline.md) | E0–E8 / P0–P8 |
| [testing/cloud-eval-runbook.md](testing/cloud-eval-runbook.md) | This cloud VM + this PC |
| [testing/testing-binary-initializer.md](testing/testing-binary-initializer.md) | Bootstrap sign-off |
| [testing/clean-machine-binary-test.md](testing/clean-machine-binary-test.md) | Wipe or new PC → eval-ready |

## Developers

| Document | Description |
|----------|-------------|
| [commands.md](commands.md) | CLI flags |
| [configuration.md](configuration.md) | YAML schema |
| [architecture.md](architecture.md) | Components |
| [deployment.md](deployment.md) | Multi-Mac topology |
| [setup.md](setup.md) | Wizard / k3d scenarios (includes local cluster without Jenkins) |
| [prepare-wizard.md](prepare-wizard.md) | `prepare` questionnaire |

## Releases (what each version shipped)

Current install is the GitHub Release binary on **Users** above. This table is the inventory; it is not a second start-here. Repo README and [user-guide.md](user-guide.md) must not link the v0.3 pages.

| Version | What it implemented | Where to read |
|---------|---------------------|---------------|
| v0.5.2 (this tree) | Jenkins iCode **git** (`branch` / `tag` / `commit`) or **release** file upload; leftover clone wipe; SWE-bench Pro third job | [user-guide.md](user-guide.md), [icode-tag-commit-release/README.md](icode-tag-commit-release/README.md) |
| binary-initializer (v0.4+) | Download `mac-k3d-{os}-{arch}`; `setup` installs Docker; controller vs worker | [new-machine.md](new-machine.md), [binary-initializer/README.md](binary-initializer/README.md) |
| v0.3.0 | `cargo install` + `prepare`; user installs Docker first | **Historical** below (not the product path) |

## Historical (v0.3.0 `cargo install` / `prepare`)

Not the product path. Keep for old machines only. Do not link these from the repo README or [user-guide.md](user-guide.md).

| Document | Description |
|----------|-------------|
| [initializer-new-machine.md](initializer-new-machine.md) | v0.3.0 blank machine via `cargo install` |
| [testing-initializer.md](testing-initializer.md) | v0.3.0 steps 0–7 |

## Branch process (not start-here)

Each folder is **only** `README.md` (what that git branch shipped) and `testing.md` (how to re-test it). Product behavior belongs in **Users** / **Developers** above.

| Document | Description |
|----------|-------------|
| [icode-tag-commit-release/README.md](icode-tag-commit-release/README.md) | Git or release iCode input; Harbor on DeepSWE, LoLBench, and SWE-bench Pro |
| [binary-initializer/README.md](binary-initializer/README.md) | Release `setup` + Docker install |

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
