# iCode CI on a new controller or worker

**Users:** use this page to run **iCode** evals in Jenkins (`EVAL_MODE=binary` or `source`).

**Machine bootstrap** (Docker, `mac-k3d setup`, linger, Jenkins login) lives in [initializer-new-machine.md](initializer-new-machine.md). This page does not repeat that checklist.

**Developers / pass-fail checks:** [testing-icode-ci.md](testing-icode-ci.md).

iCode packaging names below are a reminder from the iCode product README (GitCode Releases). This repo does not implement iCode.

---

## Two binaries

| Binary | Where | Role |
|--------|--------|------|
| `mac-k3d` | GitHub Releases (`mac-k3d-linux-x86_64`, …) | Turn this computer into a **controller** or **worker** |
| `icode` | GitCode: **binary** = `icode-<os>-<arch>-full-vX.Y.Z.tar.gz`; **source** = git clone + `uv sync` **in the job** | Harness Jenkins runs |

You still choose **CI controller** or **CI worker** in `mac-k3d setup`.

```text
Controller:  Docker → k3d → Jenkins :9080
             job lolbench_one_task (EVAL_MODE=binary|source)
Worker:      Docker + Java + agent → that Jenkins URL
             binary: unpack tarball and run ./icode
             source: git clone + uv sync in the workspace, then ./icode
```

Do **not** `uv sync` or clone iCode onto the worker at `mac-k3d prepare` / `setup` time. Source mode installs `uv` and syncs **inside the Jenkins build**. Do **not** use the slim (`icode-…-vX.Y.Z.tar.gz` without `-full-`) archive for binary CI.

Worker `setup` must **not** run `start`.

---

## Ops prerequisites (not automated)

mac-k3d cannot invite GitCode members or mint LLM keys.

1. **GitCode private repo:** a repo admin invites Toby (or whoever runs the job). After access, copy the **v0.1.41 `-full-`** asset URL, or clone URL for source mode.
   - Linux binary (README reminder): `icode-linux-x86_64-full-v0.1.41.tar.gz`
   - macOS binary: `icode-darwin-aarch64-full-v0.1.41.tar.gz`
2. **GitCode PAT:** Jenkins Credentials ID `gitcode-pat` → env `GITCODE_TOKEN` (private download / clone).
3. **DeepSeek API token:** Jenkins Credentials ID `deepseek-api-key` → env `DEEPSEEK_API_KEY`.
   - Enter during controller `prepare` (pending file, mode 0600), or on `mac-k3d config`, or in the Jenkins UI (**Manage Jenkins → Credentials**).
   - **Never** commit the token, put it in job parameters, or write it to `config.yaml`.

See [secrets.md](secrets.md).

---

## A. Controller

1. Follow [initializer-new-machine.md](initializer-new-machine.md) section **A** (`mac-k3d setup`, role **CI controller**). Store DeepSeek / GitCode secrets when prompted, or later via `config`.
2. Re-apply the job after this mac-k3d version:

```bash
mac-k3d config -c ~/.config/mac-k3d/config.yaml
```

3. Open `http://localhost:9080` (or `http://<controller>:9080`). Job **lolbench_one_task** should have `EVAL_MODE`, `ICODE_RELEASE`, `ICODE_GIT_URL`, `ICODE_GIT_REF`, `TASK`, `ICODE_ARGS`.

Optional YAML under `jenkins_job:`: `default_eval_mode`, `default_icode_release`, `default_icode_git_url`, `default_icode_git_ref`, `default_task`, `default_icode_args`.

---

## B. Worker

1. Follow [initializer-new-machine.md](initializer-new-machine.md) section **B** (role **CI worker**, Jenkins URL, API token, `config`). Confirm the node is **online**.
2. You do not install iCode with Python on the machine. Binary mode fetches the release when the job runs (or you pass a local path). Source mode clones in the build workspace.

---

## C. Trigger an eval

In Jenkins: **lolbench_one_task** → **Build with Parameters**.

| Parameter | Example |
|-----------|---------|
| `EVAL_MODE` | `binary` (v0.1.41 test) or `source` |
| `ICODE_RELEASE` | URL of `icode-linux-x86_64-full-v0.1.41.tar.gz`, **or** a path to an unpacked `icode` binary (stubs/tests) |
| `ICODE_GIT_URL` | iCode git URL (`source` mode; private clone uses `gitcode-pat`) |
| `ICODE_GIT_REF` | branch or tag (default `main`) |
| `TASK` | `ruff_1` (exported as `TASK` and `ICODE_TASK`) |
| `ICODE_ARGS` | `--help` for smoke (does **not** call DeepSeek). Real call: whatever `./icode --help` on **v0.1.41** documents — do not invent `eval --task`. README also documents `tui`. |
| `AGENT_LABEL` | `lolbench` |

### Binary (v0.1.41)

```bash
curl -fsS -u "admin:$JENKINS_PASSWORD" \
  "http://localhost:9080/job/lolbench_one_task/buildWithParameters" \
  --data-urlencode "EVAL_MODE=binary" \
  --data-urlencode "TASK=ruff_1" \
  --data-urlencode "ICODE_RELEASE=https://gitcode.com/<ns>/<repo>/releases/download/v0.1.41/icode-linux-x86_64-full-v0.1.41.tar.gz" \
  --data-urlencode "ICODE_ARGS=--help"
```

Replace the URL with the real asset after the GitCode invite. Private URLs need `gitcode-pat` on the controller (`PRIVATE-TOKEN`). Slim tarball names print a warning.

After unpack locally (optional):

```bash
tar -xzf icode-linux-x86_64-full-v0.1.41.tar.gz
# then set ICODE_RELEASE to the path of ./icode
```

### Source (git + uv in the job)

```bash
curl -fsS -u "admin:$JENKINS_PASSWORD" \
  "http://localhost:9080/job/lolbench_one_task/buildWithParameters" \
  --data-urlencode "EVAL_MODE=source" \
  --data-urlencode "TASK=ruff_1" \
  --data-urlencode "ICODE_GIT_URL=https://gitcode.com/<ns>/<repo>.git" \
  --data-urlencode "ICODE_GIT_REF=main" \
  --data-urlencode "ICODE_ARGS=--help"
```

---

## D. One PC, both roles

Same as the initializer dual-file pattern (`config.yaml` + `worker.yaml`), then trigger the job as above. See [initializer-new-machine.md](initializer-new-machine.md) section **C**.
