# Commands

All commands accept global flags:

| Flag | Description |
|------|-------------|
| `-c, --config <PATH>` | Config file (default: `~/.config/mac-k3d/config.yaml`) |
| `-v, --verbose` | Increase log verbosity (repeatable: `-vv` for debug) |

Logging uses `tracing`; override with `RUST_LOG=debug`.

---

## `setup`

First-run for a downloadable binary: interactive wizard, then apply.

```bash
mac-k3d                 # TTY only: same as setup
mac-k3d setup
mac-k3d setup -c ~/.config/mac-k3d/worker.yaml
mac-k3d setup --disk-min-gb 20
```

### Flags

| Flag | Description |
|------|-------------|
| `--disk-min-gb N` | Override minimum free disk (GB) for prepare |

Global `-c / --config` is honored.

### Behavior

1. Run `prepare` (wizard if no config / TTY; existing-config menu if a file already exists).
2. Prompt **Continue and apply now?**
3. **Controller / standalone:** `start` then `config` (k3d, Jenkins, eval job `deepswe_one_task`).
4. **Worker:** `config` only (Jenkins agent). Does **not** run `start`.
5. Print role, config path, Jenkins URL, agent unit/LaunchAgent name.

If stdin is not a TTY and no subcommand is given, the CLI exits 2 with a short usage line.

Keep `prepare` / `start` / `config` for power users. Controller `config`/`start` ensure **`deepswe_one_task`** and **`lolbench_one_task`** (same P0–P8 pipeline; pick the job or `--benchmark`).

---

## `eval`

Run the iCode **agent harness** vs the same DeepSeek LLM **without** iCode on DeepSWE (Process 2). See [binary-initializer/user-guide.md](binary-initializer/user-guide.md), [binary-initializer/icode-harness-inputs.md](binary-initializer/icode-harness-inputs.md), and [binary-initializer/testing/testing-eval-pipeline.md](binary-initializer/testing/testing-eval-pipeline.md).

```bash
mac-k3d eval --stage p0
mac-k3d eval --stage p5 --n-tasks 1
mac-k3d eval --local --benchmark deepswe --n-tasks 1 --icode-mode release --model deepseek-v4-pro
mac-k3d eval --local --benchmark deepswe --n-tasks 1 --icode-mode git \
  --icode-git-url https://github.com/org/icode.git --icode-git-ref v0.1.41 \
  --icode-git-ref-kind tag
mac-k3d eval --local --benchmark deepswe --n-tasks 1 --model deepseek-flash
mac-k3d eval --benchmark lolbench --task ruff_1 --icode-mode git \
  --icode-git-url https://github.com/org/icode.git --icode-git-ref main --yes
mac-k3d eval                         # interactive → local or Jenkins *_one_task
# Jenkins release: open UI and upload ICODE_RELEASE_FILE (--yes cannot attach a file)
```

### Flags

| Flag | Description |
|------|-------------|
| `--stage p0..p8` | Run one stage script under `pipeline/stages/` |
| `--local` | Full local `run_all.sh` (no Jenkins) |
| `--n-tasks N` | Number of tasks when `--task` is empty (default 1) |
| `--benchmark deepswe\|lolbench` | Suite (default `deepswe`; Jenkins job follows this) |
| `--task ID` | One question id (empty DeepSWE = first alphabetical; LoLBench example `ruff_1`) |
| `--icode-mode release\|git` | `release` (`*-full-*` drop; `binary` is an alias) or `git` (clone URL + ref). See [icode-harness-inputs.md](binary-initializer/icode-harness-inputs.md) |
| `--icode-release PATH\|URL` | `release` mode for `--local` / `--stage`; empty = persist file then `~/.local/share/mac-k3d/icode-*-full-*` or `icode`. Jenkins release uses UI upload `ICODE_RELEASE_FILE` (`--yes` does not attach a file) |
| `--icode-git-url URL` | `git` mode: `https://` on github.com or gitcode.com (no tokens in the URL) |
| `--icode-git-ref REF` | `git` mode: branch name, tag, or commit SHA (PR commit to score before merge). Default `main` |
| `--icode-git-ref-kind KIND` | `branch` (default), `tag`, or `commit` (PR SHA). Leftover `auto` still maps: 7–40 hex → commit, else branch |
| `--workdir PATH` | Eval workdir (default `./eval-runs`) |
| `--model ID` | DeepSeek Chat Completions id. Catalog: `deepseek-v4-pro` (default) or `deepseek-flash`. Env `DEEPSEEK_MODEL`. TTY Select when flags are omitted |
| `--yes` | Skip prompts: Jenkins `deepswe_one_task` or `lolbench_one_task` unless `--local`. Git `--yes` queues the job. Release `--yes` prints UI upload instructions (cannot attach `ICODE_RELEASE_FILE`). Also skips the git PAT prompt |

Private `ICODE_MODE=git` clones: on a TTY, `eval` asks for a GitCode or GitHub PAT (hidden input), writes `GITCODE_TOKEN` / `GITHUB_TOKEN` to gitignored `.env` (mode 600), and never prints it. Jenkins uses controller credentials `gitcode-pat` / `github-pat` — add them with `mac-k3d config --update-secrets` on the controller. Do not put a PAT in the URL or job parameters.

v1 harness=`icode`, llm=`deepseek`. Catalog models: `deepseek-v4-pro` (default) and `deepseek-flash`. Benchmark is `deepswe` or `lolbench`. Output: `eval-runs/reports/eval-icode-deepseek-{benchmark}-n{N}-{utc}.json`.

---

## `prepare`

Verify prerequisites and generate configuration via an interactive wizard.

```bash
mac-k3d prepare                    # wizard if no config; else prompt (see below)
mac-k3d prepare -i                 # force interactive wizard (overwrite)
mac-k3d prepare --init-config      # write defaults, no prompts
mac-k3d prepare --non-interactive  # validate only
mac-k3d prepare --disk-min-gb 20   # override role disk minimum (labs only)
# Same Mac: worker config alongside a controller (see below)
mac-k3d prepare -i -c ~/.config/mac-k3d/worker.yaml
```

See [prepare-wizard.md](prepare-wizard.md) for the full questionnaire design.

### Flags

| Flag | Description |
|------|-------------|
| `-i, --interactive` | Run interactive wizard to generate config |
| `--init-config` | Write default `config.yaml` if missing (no wizard) |
| `--non-interactive` | Validate existing config only; no prompts or writes |
| `--disk-min-gb N` | Override minimum free disk (GB); `0` in config means role default |

Global `-c / --config` is honored: prepare reads and writes that path (not only the default).

### Single-Mac controller + worker test

To exercise **worker prepare** while Jenkins already runs on this Mac:

1. Prepare/start **controller** with the default config (`role: controller` → `mac-k3d start`).
2. Finish Jenkins UI setup; create an API token (for auto-registration) or leave token empty for manual node create.
3. Prepare **worker** into a separate file:

```bash
mac-k3d prepare -i -c ~/.config/mac-k3d/worker.yaml
# Role: CI worker
# Jenkins URL: wizard default http://43.107.42.252:17070; same-PC: type http://localhost:17070
# k3d agents: 0  (optional local cluster; not required for LoLBench)
```

4. Confirm agent files under the worker remote root / downloads, and that the node appears (or launch script is ready) on the controller.
5. Do **not** `clean --purge-config` the default controller config while testing the worker file.
e
`mac-k3d start -c ~/.config/mac-k3d/worker.yaml` is optional (second local k3d); workers only need the host agent + Docker. Job parameters: [lolbench-jenkins.md](lolbench-jenkins.md).

### Behavior

If `config.yaml` already exists and stdin is a TTY, `prepare` (without `-i`) prompts: **re-run wizard (overwrite)**, **validate only**, or **cancel**. Choosing re-run wizard runs the full questionnaire and overwrites the file. Use `prepare -i` to skip that menu and always overwrite.

1. Assert macOS.
2. **Storage**: scan volumes, default to the one with most free space, prompt for base directory under that volume.
3. **Role**: standalone / controller / worker; set `jenkins.enabled` for controller.
4. **Dependencies**: discover Docker Desktop, k3d, kubectl, helm, Harbor (`uv`/`pipx`), Java; prompt to use existing, specify path, or install.
5. **LoLBench**: prefer a found checkout; otherwise print `git clone` / release unpack commands and optionally clone.
6. **Resources**: controller → ensure `CPU_CORES` Lockable Resources label; worker → Jenkins URL, download `agent.jar`, optional API registration, capacity = logical CPU cores.
7. **Disk check**: fail if free space on storage volume is below role minimum (standalone 40 GB, controller 60 GB, worker 40 GB). RAM preflight is 8 GB.
8. Write `~/.config/mac-k3d/config.yaml` and run validation.

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | All checks passed / config written |
| 1 | Missing dependency, invalid config, or user cancelled |

---

## `start`

Start Docker Desktop (if needed), create or start the k3d cluster, optionally deploy Jenkins.

**Worker YAML is rejected** (`role: worker`): use `mac-k3d config` / `setup` instead. Workers are not a k3d cluster.

```bash
mac-k3d start [--jenkins <skip|in-cluster>] [--no-wait-docker] [--skip-job]
```

### Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--jenkins` | (config) | Override Jenkins mode for this run (`skip` or `in-cluster`) |
| `--no-wait-docker` | false | Skip Docker Desktop readiness wait |
| `--skip-job` | false | Skip creating Pipeline job `lolbench_one_task` after Jenkins install |

### Behavior

1. Open Docker Desktop if not running (`open -a Docker`).
2. Poll `docker info` until ready or timeout (`docker.startup_timeout_secs`).
3. If cluster missing: `k3d cluster create` with port mappings from config.
4. If cluster exists but stopped: `k3d cluster start`.
5. If Jenkins is enabled (config or `--jenkins in-cluster`): Helm install/upgrade Jenkins chart, then create Pipeline job `lolbench_one_task` if missing (unless `--skip-job`).
6. Write state file under `~/.local/state/mac-k3d/`.

`--jenkins` overrides `jenkins.enabled` for this invocation only. If omitted, the config file value is used.

### Idempotency

- Second `start` on a running cluster is a no-op aside from Helm upgrade when Jenkins is enabled.
- Job create/update refreshes `lolbench_one_task` Pipeline definition on each `start`/`config`.

---

## `config`

Apply post-start configuration: kubeconfig merge, context selection, service URLs.

```bash
mac-k3d config [--no-merge-kubeconfig] [--show-jenkins] [--skip-agent] [--skip-job] [--skip-secrets] [--update-secrets]
```

### Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--no-merge-kubeconfig` | false | Skip merging k3d kubeconfig into `~/.kube/config` |
| `--show-jenkins` | false | Print Jenkins URL and admin password |
| `--skip-agent` | false | Worker: skip Jenkins agent register / launch-script update |
| `--skip-job` | false | Skip creating Pipeline job `lolbench_one_task` when Jenkins is enabled |
| `--skip-secrets` | false | Skip creating/updating Jenkins Credentials from pending secrets; still lists existing IDs so job XML keeps `withCredentials` binds |
| `--update-secrets` | false | Re-prompt for CI secrets even if credentials already exist |

### Behavior

1. If the named k3d cluster exists: merge kubeconfig, select context, wait for API.
2. Worker without a local cluster: skip kubeconfig (agent-only is OK).
3. If Jenkins enabled or `--show-jenkins`: print URL and admin password from the cluster secret.
4. **Controller / Jenkins enabled:** upload pending CI secrets into Jenkins Credentials (see [secrets.md](secrets.md)); create/update Pipeline jobs `deepswe_one_task` / `lolbench_one_task` with `ICODE_MODE` (`release` / `git`), `ICODE_RELEASE_FILE` (upload), `ICODE_GIT_URL`, `ICODE_GIT_REF`, `ICODE_GIT_REF_KIND`, `TASK`. Parameter defaults come from `jenkins_job.*`. See [lolbench-jenkins.md](lolbench-jenkins.md) and [icode-harness-inputs.md](binary-initializer/icode-harness-inputs.md).
5. **Worker:** extract `~/.local/share/mac-k3d/pipeline` (does not overwrite `icode`); using `jenkins_agent.api_user` / `api_token` from config, create/update the Jenkins node, rewrite `launch-agent.sh`, create `CPU_CORES` locks, and **start a macOS LaunchAgent** (`com.mac-k3d.jenkins-agent`) with KeepAlive (unless `--skip-agent`).

The LaunchAgent survives closing the terminal and restarts if the Java process exits. Logs: `{remote_fs}/jenkins-agent.stdout.log`.

---

## `export`

Write a **sanitized** copy of this machine’s config YAML (controller `config.yaml` or worker `worker.yaml`). Host paths and `jenkins_agent.api_token` are stripped. `credentials.pending.yaml` is never read or copied.

```bash
mac-k3d export -o /tmp/controller.yaml
mac-k3d export -c ~/.config/mac-k3d/worker.yaml -o /tmp/worker.yaml
```

### Flags

| Flag | Description |
|------|-------------|
| `-o, --output <PATH>` | Destination YAML |

Global `-c` selects the **source** file (same resolve as other commands). Refuses to write `-o` onto the source path.

Kept: `role`, cluster/Jenkins ports, `jenkins_job.*`, Harbor `source`, labels, `controller_url`, `api_user`. Dropped: token, storage paths, tool `binary`/`app`, `lolbench.path`, `platform`, `cpu_cores`, `remote_fs`.

How to change DeepSWE TASK (use `mac-k3d set`, not sed) and queue Jenkins, and why `config --skip-secrets` does not mean the YAML has keys: [export-import.md](export-import.md).

---

## `import`

Copy a sanitized YAML onto this machine. **Write only** — does not install Docker, start k3d, or register the agent.

```bash
mac-k3d import /tmp/controller.yaml
mac-k3d import /tmp/worker.yaml -c ~/.config/mac-k3d/worker.yaml
mac-k3d import /tmp/worker.yaml --force
```

### Flags

| Flag | Description |
|------|-------------|
| `--force` | Overwrite the destination if it already exists |

Positional argument is the incoming file. Global `-c` is the **dest**; if omitted, `role: worker` → `~/.config/mac-k3d/worker.yaml`, otherwise `config.yaml`. Import re-sanitizes (so a hand-copied live `worker.yaml` still loses `api_token`), sets `platform` to this OS, and prints `setup` / `config` next steps.

`--skip-secrets` after a controller import refreshes job XML only; it does not mean the imported file contained credentials. See [export-import.md](export-import.md).

---

## `set`

Edit **non-secret** `jenkins_job` fields on a YAML file (harness, LLM family, DeepSeek model, benchmark, questions). Also persist the worker iCode **release drop** path with `--icode-release` (`~/.config/mac-k3d/icode-paths.yaml`; not the YAML you pass with `-c`). Writes YAML only for job fields — does not upload Jenkins credentials or queue an eval. Allowed values come from the catalog (`src/eval_catalog.rs`); unknown ids are rejected with `allowed: …`.

```bash
mac-k3d set --list
mac-k3d set -c /tmp/imported-config.yaml --harness icode --llm deepseek --benchmark deepswe --task abs-stepped-slices
mac-k3d set -c ~/.config/mac-k3d/config.yaml --model deepseek-flash
mac-k3d set --check-models
mac-k3d set -c ~/.config/mac-k3d/config.yaml --benchmark deepswe --n-tasks 2
mac-k3d set --icode-release /path/to/icode-linux-x86_64-full-v0.1.41
```

TTY with no flags: select harness / LLM family / DeepSeek model / benchmark, then question mode. Non-TTY requires flags (or `--check-models` / `--list`).

### Flags

| Flag | Description |
|------|-------------|
| `--list` | Print allowed harness / LLM / model / benchmark values and exit |
| `--harness <ID>` | Catalog harness (v1: `icode`) |
| `--llm <ID>` | Catalog LLM family (v1: `deepseek`) |
| `--model <ID>` | DeepSeek Chat Completions id (`deepseek-v4-pro` default, or `deepseek-flash`) |
| `--check-models` | Optional live `GET /models` against `.env` key; fail if YAML/`--model` id is not a provider id |
| `--benchmark <ID>` | `deepswe` or `lolbench` (which job receives TASK / N_TASKS / TASKS defaults) |
| `--task <ID>` | One question id |
| `--n-tasks N` | First N sorted questions (clears TASK / TASKS). `N>1` is slower and costs more LLM calls |
| `--tasks a,b` | Explicit comma-separated ids |
| `--icode-release PATH` | Worker `*-full-*` drop; writes `~/.config/mac-k3d/icode-paths.yaml` |

`--task`, `--n-tasks`, and `--tasks` are mutually exclusive. `set` updates only the flags you pass, then `save`.

`--check-models` is optional and does **not** rewrite Chat Completions. It `GET`s `{ICODE_API_BASE or https://api.deepseek.com}/models` (OpenAI list JSON, `data[].id`) using `DEEPSEEK_API_KEY` from the environment or `.env`. Fail if the YAML / `--model` id is missing from that list. Cloud `set` on YAML often has **no** key — run this on a **worker / local** machine with `.env`, not as a hard requirement of controller `set`. P0 does the same check when the key is present (before paid P5/P6).

```bash
# this PC / worker (has .env)
mac-k3d set -c ~/.config/mac-k3d/worker.yaml --check-models
mac-k3d set -c /tmp/controller-lab.yaml --model deepseek-flash --check-models
```

To add a model later: copy an id from `GET /models` (or DeepSeek “Models & Pricing”), append that **exact** string to `MODELS` in `src/eval_catalog.rs`, rebuild, then `set --model <id>` and controller `config --skip-secrets`. Never use marketing / product names (`deepseek-v4.1-flash` is not an API id).

Push the YAML into Jenkins job XML on a live controller (still **no** secrets in the file):

```bash
mac-k3d config -c ~/.config/mac-k3d/config.yaml --skip-secrets
```

Do not use `sed` on `default_task`. See [export-import.md](export-import.md).

---

## `teardown`

Stop the cluster and services without deleting data.

```bash
mac-k3d teardown [--stop-docker] [--deregister-agent]
```

### Flags

| Flag | Description |
|------|-------------|
| `--stop-docker` | Quit Docker Desktop after stopping cluster |
| `--deregister-agent` | Worker: also delete Jenkins node + CPU_CORES lockable resources |

### Behavior

1. `k3d cluster stop <name>` if running.
2. Optionally `osascript` quit Docker Desktop.
3. Leave config and state files intact.
4. **Worker:** unload LaunchAgent (stop agent process). With `--deregister-agent`, also delete the Jenkins node and locks.
---

## `clean`

Remove cluster, volumes, and local artifacts.

```bash
mac-k3d clean [--purge-config] [-y|--yes]
mac-k3d clean -c ~/.config/mac-k3d/worker.yaml --purge-config --yes
```

### Flags

| Flag | Description |
|------|-------------|
| `--purge-config` | Default config: remove `~/.config/mac-k3d/`. With `-c FILE`: remove **only that file** |
| `-y, --yes` | Skip confirmation (required to perform deletion) |

### Behavior

Without `--yes`: print warning and exit 0.

With `--yes`:

1. **Worker:** stop LaunchAgent; deregister Jenkins agent node and delete `{agent}-core-*` Lockable Resources (needs `api_token` in config).
2. `k3d cluster delete <name>` from the loaded config.
3. Without `-c`: remove `~/.local/state/mac-k3d/`. With `-c`: leave shared state intact.
4. If `--purge-config`: remove the `-c` file only, or the whole config directory when using the default path.

---

## `status`

Report current environment state (read-only).

```bash
mac-k3d status
```

### Output (example)

```text
Docker Desktop:  running
k3d cluster:     mac-k3d (running, 1 server, 0 agents)
kubectl context: k3d-mac-k3d
Jenkins:         configured, pod Running, http://localhost:17070
```

After `clean` / while the cluster is gone, Jenkins still shows as **configured** from the YAML (so the next `start` will redeploy it), not as live:

```text
k3d cluster:     ci-controller (missing, 0 server, 0 agents)
Jenkins:         configured, not running (cluster missing), http://localhost:17070
```

Pod lookup uses `--context k3d-<cluster>` from the loaded config, not whatever the shell’s current kubectl context happens to be.
---

## Typical workflows

### Minimal (no Jenkins)

```bash
mac-k3d prepare --init-config
mac-k3d start
mac-k3d config
kubectl get nodes
mac-k3d teardown   # end of day
```

### With Jenkins

```bash
mac-k3d prepare --init-config
mac-k3d start --jenkins in-cluster
mac-k3d config --show-jenkins
# open http://localhost:17070
mac-k3d clean --yes
```
