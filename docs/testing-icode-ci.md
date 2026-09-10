# Testing iCode Jenkins CI (binary + source)

Use this document to **verify** Jenkins can run **iCode** on a worker: GitCode **`-full-` release** (`EVAL_MODE=binary`) or git + `uv sync` **in the job** (`EVAL_MODE=source`).

- **Users (commands only):** [icode-ci-new-machine.md](icode-ci-new-machine.md)
- **Machine initializer tests** (Docker, `setup`, agent online): [testing-initializer.md](testing-initializer.md) — do not mix checklists

iCode README is a reminder for artifact names only. This repo does not ship iCode.

Refresh the live job after changing mac-k3d:

```bash
mac-k3d config -c ~/.config/mac-k3d/config.yaml
```

`--help` does **not** call DeepSeek. A real LLM call needs `deepseek-api-key` in Jenkins Credentials and `ICODE_ARGS` from `./icode --help` on **v0.1.41** (do not invent `eval --task`).

---

## I0 — unit tests (developer)

```bash
cd /path/to/mac-k3d
cargo test
```

**Expected:** tests pass. Jenkinsfile tests assert `EVAL_MODE`, `ICODE_RELEASE`, `tar -xzf`, `PRIVATE-TOKEN` / `GITCODE_TOKEN`, and `uv sync` in the **source** branch. They fail if the job still contains `honeyc`, `BINARY_TARGET`, `harbor run`, or `GitSCM`.

---

## I1 — stub `icode` (no GitCode download)

```bash
mkdir -p ~/.local/bin
cat > ~/.local/bin/icode << 'EOF'
#!/bin/sh
echo "icode stub: $*"
echo "TASK=${TASK-} ICODE_TASK=${ICODE_TASK-}"
echo "$@" | tee /tmp/icode-last-args.txt
exit 0
EOF
chmod +x ~/.local/bin/icode
~/.local/bin/icode --help
```

**Expected:** prints `icode stub: --help`; `/tmp/icode-last-args.txt` is `--help`.

---

## I2 — trigger job with stub path (binary)

Jenkins up, worker **online**, job rewritten (`mac-k3d config` on the **controller**).

```bash
# JENKINS_PASSWORD from: mac-k3d config -c ~/.config/mac-k3d/config.yaml --show-jenkins
curl -fsS -u "admin:$JENKINS_PASSWORD" \
  "http://localhost:9080/job/lolbench_one_task/buildWithParameters" \
  --data-urlencode "EVAL_MODE=binary" \
  --data-urlencode "TASK=ruff_1" \
  --data-urlencode "ICODE_RELEASE=$HOME/.local/bin/icode" \
  --data-urlencode "ICODE_ARGS=--help"
```

Open the build console.

**Expected:**

- **SUCCESS**
- Console runs the stub (not `uv sync`, not `harbor run`, not `honeyc`)
- `/tmp/icode-last-args.txt` on the **worker** is `--help`
- Description includes `ruff_1` and the `ICODE_RELEASE` path

---

## I3 — fake `-full-` tarball (binary)

On the worker (or any host the agent can read):

```bash
tmp="$(mktemp -d)"
printf '%s\n' '#!/bin/sh' 'echo "icode from tar: $*"' 'exit 0' > "$tmp/icode"
chmod +x "$tmp/icode"
tar -czf "$HOME/icode-linux-x86_64-full-v0.0.0.tar.gz" -C "$tmp" icode
```

Trigger with `EVAL_MODE=binary`, `ICODE_RELEASE=$HOME/icode-linux-x86_64-full-v0.0.0.tar.gz`, and `ICODE_ARGS=--help`.

**Expected:** console shows `tar -xzf` (or extract under `icode-in`); then the script runs; **SUCCESS**.

Optional URL variant: `python3 -m http.server` in the directory that holds the tarball; set `ICODE_RELEASE=http://127.0.0.1:8000/icode-linux-x86_64-full-v0.0.0.tar.gz` if the **agent** can reach that URL.

---

## I4 — missing inputs

**Binary:** `EVAL_MODE=binary` and `ICODE_RELEASE` empty.

**Expected:** **FAILURE**; console contains `ICODE_RELEASE is required`.

**Source:** `EVAL_MODE=source` and `ICODE_GIT_URL` empty.

**Expected:** **FAILURE**; console contains `ICODE_GIT_URL is required when EVAL_MODE=source`.

---

## I5 — mac-k3d still initializes roles

```bash
cp target/release/mac-k3d /tmp/mac-k3d-user
chmod +x /tmp/mac-k3d-user
env -i HOME="$HOME" USER="$USER" PATH="/usr/bin:/bin:/tmp" /tmp/mac-k3d-user --help
```

**Expected:** help lists `setup`, `prepare`, `start`, `config`. Full controller/worker checks stay in [testing-initializer.md](testing-initializer.md).

---

## I6 — macOS

Same I1–I3 with `icode-darwin-aarch64-full-vX.Y.Z.tar.gz` (or the stub). Agent: LaunchAgent `com.mac-k3d.jenkins-agent`. Docker Desktop running.

---

## I7 — real v0.1.41 binary (after GitCode invite)

Prerequisites: GitCode private-repo access, `gitcode-pat` and (for a real LLM call) `deepseek-api-key` in Jenkins Credentials. See [icode-ci-new-machine.md](icode-ci-new-machine.md).

```bash
curl -fsS -u "admin:$JENKINS_PASSWORD" \
  "http://localhost:9080/job/lolbench_one_task/buildWithParameters" \
  --data-urlencode "EVAL_MODE=binary" \
  --data-urlencode "TASK=ruff_1" \
  --data-urlencode "ICODE_RELEASE=https://gitcode.com/<ns>/<repo>/releases/download/v0.1.41/icode-linux-x86_64-full-v0.1.41.tar.gz" \
  --data-urlencode "ICODE_ARGS=--help"
```

**Expected:** **SUCCESS**; console downloads with `PRIVATE-TOKEN` when `GITCODE_TOKEN` is bound; unpacks `-full-` tarball; runs `./icode --help`. This smoke does **not** call DeepSeek.

For a real API call, set `ICODE_ARGS` from `./icode --help` on **v0.1.41** (operator-supplied). Confirm `DEEPSEEK_API_KEY` is present in the Evaluate stage env (do not print the value).

---

## I8 — source-mode smoke (`EVAL_MODE=source`)

Requires GitCode clone access and `gitcode-pat`. `ICODE_ARGS=--help` does **not** call DeepSeek.

```bash
curl -fsS -u "admin:$JENKINS_PASSWORD" \
  "http://localhost:9080/job/lolbench_one_task/buildWithParameters" \
  --data-urlencode "EVAL_MODE=source" \
  --data-urlencode "TASK=ruff_1" \
  --data-urlencode "ICODE_GIT_URL=https://gitcode.com/<ns>/<repo>.git" \
  --data-urlencode "ICODE_GIT_REF=main" \
  --data-urlencode "ICODE_ARGS=--help"
```

**Expected:** **SUCCESS**; console `git clone` + `uv sync` in the workspace (not at `mac-k3d prepare`); then `./icode --help`. **No** `harbor run`. Token must not appear in the build description.

---

## Failures to treat as bugs

- Binary mode clones iCode / LoLBench or runs `uv sync` / `harbor run`
- Job still mentions `honeyc`
- Worker `setup` runs `start` and installs a second Jenkins
- Slim tarball used in user docs as the recommended CI artifact
- GitCode PAT or DeepSeek key committed, or shown in job parameters / description
