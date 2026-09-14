# Binary-initializer workflow

Full path from a **new Mac or Linux** machine through Jenkins roles to an **iCode vs DeepSeek** DeepSWE evaluation and named JSON output.

This is the **binary-initializer** story. The older **v0.3.0** cargo/`prepare` path stays in [../initializer-new-machine.md](../initializer-new-machine.md).

## Why two processes

| Process | Goal | Why |
|---------|------|-----|
| **1. Machine bootstrap** | Download one `mac-k3d` binary; become a Jenkins **controller** or **worker** | A blank PC has no Rust. The binary installs Docker and the rest. |
| **2. Eval pipeline** | Run DeepSWE with harness=iCode and LLM=DeepSeek; compare to raw DeepSeek API; write JSON | Measure whether iCode improves patches (f2p / p2p) vs calling the model without a harness. |

```text
New Mac/Linux
  → download mac-k3d Release asset
  → setup: controller (k3d+Jenkins :17070) or worker (agent.jar)
  → mac-k3d eval (harness=icode, llm=deepseek, benchmark=deepswe, N)
  → Jenkins job icode_eval on worker
       clone DeepSWE, install pier, put iCode in the Pier sandbox
       arm A: pier + iCode + DeepSeek
       arm B: DeepSeek chat API only (no iCode)
       grade patches (f2p / p2p)
  → output/eval-icode-deepseek-deepswe-n{N}-{utc}.json
```

```mermaid
flowchart TD
  newHost["New Mac or Linux"]
  bin["Download mac-k3d asset"]
  setup["mac-k3d setup"]
  role{"role?"}
  ctrl["Controller: Docker k3d Jenkins :17070 credentials"]
  work["Worker: Docker Java agent pier"]
  evalCli["mac-k3d eval: icode / deepseek / deepswe / N"]
  job["Jenkins icode_eval on worker"]
  pierA["Pier + iCode inside sandbox"]
  pierB["Baseline: DeepSeek API no iCode"]
  grade["Verifier: patches f2p p2p"]
  json["output named JSON"]

  newHost --> bin --> setup --> role
  role --> ctrl
  role --> work
  ctrl --> evalCli
  work --> job
  evalCli --> job
  job --> pierA
  job --> pierB
  pierA --> grade
  pierB --> grade
  grade --> json
```

---

## Process 1 — prepare the machine

**User steps:** [binary-initializer-new-machine.md](binary-initializer-new-machine.md) · clean-machine walkthrough: [clean-machine-binary-test.md](clean-machine-binary-test.md)  
**Pass/fail:** [testing-binary-initializer.md](testing-binary-initializer.md) (Task 0–5 + Task 7 on Linux; Task 6 macOS later) · automated checks: [`scripts/env_set_up/`](../../scripts/env_set_up/README.md)

| Step | What | Why |
|------|------|-----|
| Download `mac-k3d-{os}-{arch}` | Prebuilt CLI from a GitHub **pre-release** (or Latest after a final tag) | No Rust on a new laptop |
| `chmod +x` / quarantine strip | Make the asset executable | Unsigned downloads are blocked on macOS |
| `mac-k3d setup` | Wizard: role, Install Docker / k3d / Java | One entry point for controller or worker |
| Controller: `start` + `config` | k3d cluster + Jenkins UI `:17070` + credentials | Job queue and `deepseek-api-key` live here |
| Worker: token + `config` | Inbound agent + `CPU_CORES` locks | Workloads run on the worker’s Docker, not inside the controller’s k3d nodes |

Honest leftovers: Linux **logout** after docker group; macOS first **Docker Desktop** window; worker **API token**; DeepSeek key on the **controller** credentials store.

Workers must **not** run `mac-k3d start -c worker.yaml` (rejected on purpose).

---

## Process 2 — evaluation pipeline

**Pass/fail per stage:** [testing-eval-pipeline.md](testing-eval-pipeline.md) (E0–E8 tracking; P0–P8 stage detail)

| Stage | What | Why |
|-------|------|-----|
| Choose harness / LLM / benchmark | v1: **icode** / **deepseek** / **deepswe** only | Fixed options reduce misconfiguration |
| Choose N and iCode binary vs source | Limit cost; source path defaults to `~/Documents/Toby/iCode-main` on the worker | Same machine that runs Pier must see iCode |
| Install Pier | `uv tool install datacurve-pier` | DeepSWE is Harbor/Pier-format tasks |
| Clone DeepSWE | `git clone https://github.com/datacurve-ai/deep-swe` | Not vendored in this repo |
| Pier agent `icode` | Install script + DeepSeek allowlist inside the sandbox | Puts iCode **inside** the task Docker image Pier builds |
| Arm A | `pier run … --agent-import-path icode_pier_agent:ICodeAgent --model deepseek-v4-pro` | Harness under test (`DEEPSEEK_MODEL`; key from gitignored `.env`) |
| Arm B | Baseline DeepSeek chat (`deepseek-v4-pro`) on the same `instruction.md` | Compare without iCode scaffolding |
| Grade | Verifier → f2p / p2p / `resolved` / pass@1, tokens, time, model | DeepSWE’s held-out tests plus API usage |
| JSON | `output/eval-icode-deepseek-deepswe-n{N}-{utc}.json` | Clear naming for which eval ran |

Trigger:

```bash
mac-k3d eval                  # interactive → Jenkins icode_eval (or --local)
mac-k3d eval --stage p5 --n-tasks 1   # isolated stage test
```

Jenkins job name: **`icode_eval`**. Logs print `PROGRESS n% …` so you can see completion over time. Agent label `lolbench`; builds take a `CPU_CORES` lock.

---

## Release assets (all machines)

One tag publishes **four** assets (see [`.github/workflows/release-binaries.yml`](../../.github/workflows/release-binaries.yml)):

| Asset | Runner | How it is built |
|-------|--------|-----------------|
| `mac-k3d-linux-x86_64` | `ubuntu-latest` | native |
| `mac-k3d-linux-aarch64` | `ubuntu-24.04-arm` | native |
| `mac-k3d-darwin-aarch64` | `macos-latest` (Apple Silicon) | native |
| `mac-k3d-darwin-x86_64` | `macos-latest` (Apple Silicon) | **cross-compile** `--target x86_64-apple-darwin` |

CI checks `file` + `lipo -info` so the Intel asset is **x86_64**, not arm64. There is no `macos-13` job.

---

## Related docs

| Doc | Role |
|-----|------|
| [binary-initializer-new-machine.md](binary-initializer-new-machine.md) | User bootstrap commands |
| [clean-machine-binary-test.md](clean-machine-binary-test.md) | Clean PC → binary → controller/worker → eval-ready |
| [`scripts/env_set_up/`](../../scripts/env_set_up/README.md) | Automated controller/worker/eval-ready checks |
| [testing-binary-initializer.md](testing-binary-initializer.md) | Bootstrap sign-off |
| [cloud-eval-runbook.md](cloud-eval-runbook.md) | Operator runbook: cloud root controller → local worker → JSON |
| [testing-eval-pipeline.md](testing-eval-pipeline.md) | Pipeline stage CLI tests |
| [../secrets.md](../secrets.md) | Controller credentials (`deepseek-api-key`) |
| [../lolbench-jenkins.md](../lolbench-jenkins.md) | Older `lolbench_one_task` job pointer |
