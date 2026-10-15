# Mac mini worker: onboarding and DeepSWE parity trial

Lab checklist for adding the first Apple silicon Mac mini as a worker of this lab's controller, and for proving that it evaluates DeepSWE with **iCode** and with **JiuwenSwarm** the same way as the Linux worker (this PC, `linux-Michael-Ubuntu`). The product steps it follows are [new-machine.md](../new-machine.md#mac-mini-worker); the controller and the Linux worker are the ones in [cloud-eval-runbook.md](cloud-eval-runbook.md). LoLBench and SWE-bench Pro also run on a Mac, but this trial confirms DeepSWE only.

The trial covers each harness in each input mode, and on the Mac both kinds of task container:

| | Git input | Release input |
|--|-----------|---------------|
| **iCode** | pull request 2 of `michaelling/jiuwenicode`, pinned to its commit (tag `v0.1.41` does not build in git mode: its `uv.lock` is out of date); the Mac builds a `linux/arm64` tree for native questions | one `.tar.gz` with both drops runs native questions natively; an upload with only `icode-linux-x86_64` runs every question emulated |
| **JiuwenSwarm** | pull request 2 of `michaelling/jiuwenswarm` (the default); the Mac builds a `linux/arm64` offline pack for native questions | a `workswarm` wheel, rebuilt for `linux/arm64`; a `linux/amd64` bundle from an earlier run runs every question emulated |

Native means `linux/arm64` containers on the build that `mac-k3d eval prepare` proved on this Mac; emulated means the published `linux/amd64` image under Rosetta, the image the Linux worker runs natively ([pipeline.md](../pipeline.md#deepswe-on-linux-and-macos)).

**Pass:** `scripts/env_set_up/run_all.sh` ends with no `FAIL` or `ERROR` on both workers, `config` prints no `Warning:` on the Mac, and every row of [What must match](#what-must-match) holds for the four Linux–Mac pairs, plus the two emulated-fallback runs.

When it passes, replace "not run yet" and "it has not been run yet" in [dependencies.md](../dependencies.md#linux-and-macos) with the date and the dispatcher build numbers, and add a row to [integration-log.md](../integration-log.md).

---

## 1. Onboard the Mac

On the Mac, as the user that will run the agent:

| Setting | Check | Want |
|---------|-------|------|
| Automatic login (System Settings > Users & Groups; FileVault off) | `defaults read /Library/Preferences/com.apple.loginwindow autoLoginUser` | your user name |
| No sleep: `sudo pmset -a sleep 0` | `pmset -g \| grep -E '^ *sleep'` | `sleep 0` |
| Docker Desktop > Settings > General: start at sign-in, **Use Rosetta for x86_64/amd64 emulation on Apple Silicon** on | `docker run --rm --platform linux/amd64 alpine:3.20 uname -m` | `x86_64` |
| Docker Desktop > Settings > Resources: Memory Limit 18 GB or more, CPUs 4 or more, disk about 200 GB. The VM reports a little less than its limit; set at least `MIN_RAM_GB` + 2 GB | `docker info --format '{{.NCPU}} CPUs {{.MemTotal}} bytes'` | `4 CPUs` or more, `17179869184 bytes` or more |
| Docker Desktop > Settings > Resources > File sharing keeps `/Users` (the default) | `docker run --rm -v "$HOME:$HOME:ro" alpine:3.20 ls "$HOME" >/dev/null && echo ok` | `ok` |
| Homebrew | `brew --version` | any |

The binary must be the same commit as this PC's. `scripts/redeploy.sh` copies only to hosts of its own OS and CPU, so build it on the Mac:

```bash
git clone https://github.com/Toby-Yu/mac-k3d.git && cd mac-k3d
git checkout "$(ssh Toby@<this-pc> '~/.local/bin/mac-k3d --version' | sed -E 's/.*\(([0-9a-f]+).*/\1/')"   # this PC's commit
curl --proto '=https' -sSf https://sh.rustup.rs | sh -s -- -y && . "$HOME/.cargo/env"
bash scripts/redeploy.sh   # this Mac is the local worker; run it after `setup` below
```

A released commit can use the asset instead: download `mac-k3d-darwin-aarch64`, `xattr -d com.apple.quarantine`, and copy it to `~/.local/bin/mac-k3d` ([new-machine.md](../new-machine.md#1-download-the-binary)).

```bash
mac-k3d setup -c ~/.config/mac-k3d/worker.yaml
# Role: CI worker; Docker / git: use existing; URL: Enter (this lab's controller); API user / token: admin + a token
```

**Expect:** `java 21: ok`, `harbor 0.22.0: ok`, Homebrew bash and python3 with no warning, `brew install pyyaml` (the anti-cheat harness labels), `cpu_cores` no larger than Docker Desktop's CPUs, and no `Warning:` line. Each of these is a `Warning:` from `config` when it is wrong: python3 without PyYAML, no curl, Docker Desktop memory below `MIN_RAM_GB`, Docker that cannot run `linux/amd64`, Rosetta off in Docker Desktop, and a folder (`remote_fs`, the share) that Docker cannot mount. Then:

| Check | Command | Want |
|-------|---------|------|
| Agent service | `launchctl print "gui/$(id -u)/com.mac-k3d.jenkins-agent" \| grep -E 'state\|last exit'` | `state = running` |
| Node | Jenkins > Nodes | `macos-<hostname>` online, labels including `macos docker fits-deepswe executors-<n>` |
| Status | `mac-k3d status -c ~/.config/mac-k3d/worker.yaml` | no `Warning:` line |
| Restart | Restart the Mac, wait 2 minutes, touch nothing | the node comes back online by itself |

**Checkpoint — paste:** the `config` tail, the `launchctl` lines, and the node's labels.

---

## 2. Check both workers with `scripts/env_set_up`

Run the same checks on the Mac and on this PC, from the same checkout commit, as each worker's agent user. `<token>` is a Jenkins API token of `admin`; without it the job, plugin, credential and node checks end in a `NOTE`, because the controller answers 403 to every anonymous job URL.

On the Mac, the harness builds need an iCode input with an arm64 build, so give the git ref the trial uses (or `ICODE_RELEASE=<two-drop .tar.gz>`); `jiuwenicode` has no `main` branch:

```bash
SKIP_DOWNLOAD=1 JENKINS_USER=admin JENKINS_API_TOKEN=<token> RUN_HARNESS_SMOKE=1 \
  ICODE_MODE=git ICODE_GIT_URL=https://gitcode.com/michaelling/jiuwenicode ICODE_GIT_REF=2 ICODE_GIT_REF_KIND=pr \
  ./scripts/env_set_up/run_all.sh
```

On this PC, the same command. Its `config.yaml` describes a local cluster that is stopped; to check the cloud controller from here, add `CONTROLLER_CONFIG=/nonexistent`, so 02 reads `jenkins_agent.controller_url` from `worker.yaml` as the Mac does.

| Script | Mac | Linux worker |
|--------|-----|--------------|
| 01 | `OK eval lists --harness and prepare` | the same |
| 02 | `NOTE no controller on this machine; checking the one at http://…:17070 (jenkins_agent.controller_url)`, then `OK job deepswe_<shape> present` ×3 (and the other six), `OK no retired jobs`, `OK deepswe_<shape> HARNESS: icode jiuwenswarm`, `OK Active Choices plugin (uno-choice) active`, `OK credential deepseek-api-key present` | the same |
| 03 | `OK macOS LaunchAgent com.mac-k3d.jenkins-agent running`, `OK automatic login on`, `OK the Mac does not sleep when idle`, `OK Jenkins node macos-<hostname> online with fits-deepswe` | `OK Linux agent unit mac-k3d-jenkins-agent.service active`, `OK Jenkins node linux-Michael-Ubuntu online with fits-deepswe` |
| 04 | `OK bash 5.x at /opt/homebrew/bin/bash`, `OK python3 3.1x at /opt/homebrew/bin/python3`, `OK python3 has PyYAML`, `OK Docker memory N GB (min 8)`, `OK Docker runs linux/amd64 containers (emulated)`, `OK harness build image runs for linux/amd64` and `… linux/arm64`, `OK Docker mounts …` ×2, then before section 3 `NOTE no DeepSWE question has a proven native build: …` with the `eval prepare` hint, after it `OK 2 DeepSWE questions have a proven native linux/arm64 build` | `OK Docker runs linux/amd64 natively; DeepSWE uses the published images`, `OK harness build image runs for linux/amd64`; no native-build line |
| 06 | `OK icode prepared for linux/arm64` and `… linux/amd64`, `OK jiuwenswarm prepared for linux/arm64` and `… linux/amd64`, each with its bundle sha256 | `OK icode prepared for linux/amd64`, `OK jiuwenswarm prepared for linux/amd64` |

The builds stay in each worker's harness store, so the trial's first shard on that input reuses them. A JiuwenSwarm build holds its tree in a tmpfs, about 1.5 GB of Docker Desktop's memory, while it runs. A Docker bridge that cannot reach PyPI (a VPN with a smaller MTU) stops the build in `uv`; rerun with `MAC_K3D_BUILD_NETWORK=host`.

**Checkpoint — paste:** both workers' `run_all.sh` output.

---

## 3. Prepare the DeepSWE images (Mac only)

The Mac runs a DeepSWE question natively only on a build that `mac-k3d eval prepare` made and proved on it: a native build, then Harbor's oracle and nop agents, with no model and no harness ([pipeline.md](../pipeline.md#deepswe-on-linux-and-macos)). The proof is per question and serves both harnesses. The Linux worker needs nothing: Harbor pulls its images at the trial.

On the Mac, as the agent's user (the store is that user's `~/.local/share/mac-k3d/env-check/`, and Docker Desktop's images are per user), with no DeepSWE build running:

```bash
mac-k3d eval prepare --benchmark deepswe \
  --tasks abs-module-cache-flags,cliffy-config-file-parsing,ipython-session-bundle-replay
```

**Expect:**
- `images: linux/arm64 host, native builds on public.ecr.aws/x8v8d7g8/mars-base@sha256:91db…`;
- for the two questions that build natively, `env check: <id> pass on built linux/arm64 (oracle f2p n/n p2p m/m; nop f2p k/n p2p m/m)`, with k below n;
- for `cliffy-config-file-parsing`, `images: cliffy-config-file-parsing emulated on published linux/amd64: its Dockerfile downloads an x86-64-only build`;
- at the end, `prepare deepswe (linux/arm64): 3 questions, 2 native, 1 emulated`.

A second `prepare` prints `reused from local#0` for each native question and runs no Harbor trial. 04 then prints `OK 2 DeepSWE questions have a proven native linux/arm64 build (…)`. Stop and file it if `~/.local/share/mac-k3d/prepare/env-check/<id>/build.log` shows `unknown flag: --build-context`, or if the build pulls `mars-base:latest` instead of the digest.

---

## 4. The runs

Use the same three questions on both workers, one rollout, and the same model. Pin each dispatcher to one worker with `AGENT_LABEL` and `AGGREGATE_LABEL` (the developer profile shows them; from the CLI, `--param`). A dispatcher runs each question as its own `deepswe_one_task` build, so on the Mac the two prepared questions run natively and `cliffy-config-file-parsing` runs emulated in the same run.

```bash
QUESTIONS=abs-module-cache-flags,cliffy-config-file-parsing,ipython-session-bundle-replay
```

**Git mode**, from the CLI, on both workers (four dispatcher builds):

```bash
for node in linux-Michael-Ubuntu macos-<hostname>; do
  mac-k3d eval --job some --benchmark deepswe --tasks "$QUESTIONS" --n-rollouts 1 --model deepseek-v4-pro --yes \
    --harness icode --icode-mode git --icode-git-url https://gitcode.com/michaelling/jiuwenicode \
    --icode-git-ref 2 --icode-git-ref-kind pr \
    --param AGENT_LABEL="$node" --param AGGREGATE_LABEL="$node"
  mac-k3d eval --job some --benchmark deepswe --tasks "$QUESTIONS" --n-rollouts 1 --model deepseek-v4-pro --yes \
    --harness jiuwenswarm \
    --param AGENT_LABEL="$node" --param AGGREGATE_LABEL="$node"
done
```

`--harness jiuwenswarm` with no `--jiuwenswarm-git-*` flag takes the controller's defaults, pull request 2; the dispatcher pins it to one commit for every shard. On the Mac, each harness's first native shard builds its `linux/arm64` tree from the pinned input (`source worker` in `harness.json`); the second native shard waits for that build and reuses it (`source store`). The emulated shard uses the dispatcher's `linux/amd64` bundle (`source dispatcher`).

**Release mode** needs an upload, so use `deepswe_some_task` > Build with Parameters on the job page: `TASKS` the three questions, `N_ROLLOUTS=1`, `AGENT_LABEL` and `AGGREGATE_LABEL` the node, and the **same file** for both workers, so the recorded sha256 is equal (four dispatcher builds):

| `HARNESS` | Mode field | Upload |
|-----------|------------|--------|
| `icode` | `ICODE_MODE=release` | `ICODE_RELEASE_FILE`: one `.tar.gz` holding `icode-linux-x86_64-full-vX.Y.Z` and `icode-linux-aarch64-full-vX.Y.Z` of the same version ([icode-harness-inputs.md](../icode-harness-inputs.md#linux-and-macos-workers)) |
| `jiuwenswarm` | `JIUWENSWARM_MODE=release` | `JIUWENSWARM_RELEASE_FILE`: a `workswarm` wheel, for example `uv build --wheel` in a JiuwenSwarm checkout of the pinned commit ([jiuwenswarm-harness-inputs.md](../jiuwenswarm-harness-inputs.md#release-jiuwenswarm_moderelease)) |

**Emulated fallback**, Mac only, one question each (`deepswe_one_task`, `TASK=abs-module-cache-flags`, a question that is native otherwise):

| `HARNESS` | Upload | Want on the Mac |
|-----------|--------|-----------------|
| `icode` | `ICODE_RELEASE_FILE`: only `icode-linux-x86_64-full-vX.Y.Z` | `linux/amd64 emulated`, reason `the iCode release under test has no linux/arm64 build (it runs on linux/amd64)` |
| `jiuwenswarm` | `JIUWENSWARM_RELEASE_FILE`: the `eval-runs/harness/bundle.tar.gz` archived by the Linux JiuwenSwarm git dispatcher above | `linux/amd64 emulated`, reason `the JiuwenSwarm release under test has no linux/arm64 build (it runs on linux/amd64)`; `harness.json` `source upload`, the same bundle sha256 as the Linux run |

The Mac's first git build of a commit downloads uv's Python and wheels into `~/.local/share/mac-k3d/uv-cache-linux-arm64`; the next build of that commit reuses them, and the harness store skips the build entirely.

---

## What must match

Download each dispatcher's `aggregate/artifact.json`, then compare the Linux run with the Mac run of the same harness and mode:

```bash
python3 - linux/artifact.json mac/artifact.json <<'PY'
import json, sys
a, b = (json.load(open(p)) for p in sys.argv[1:3])
def facts(d):
    ep = d.get("eval_protocol", {})
    harness = ep.get("harness", {})
    iso = ep.get("isolation", {})
    return {
        "harness": harness.get("name"),
        "input": harness.get("identity"),
        "questions": sorted(t["id"] for t in d["icode"]["tasks"]),
        "pipeline": ep.get("pipeline", {}).get("commit"),
        "harbor": ep.get("harbor", {}).get("version"),
        "model": (ep.get("model_params") or {}).get("model"),
        "max_tokens": (ep.get("model_params") or {}).get("max_tokens"),
        "settings": harness.get("settings"),
        "bundle_sha256": harness.get("bundle_sha256"),
        "runtime_sha256": iso.get("runtime_sha256"),
        "leak_scan": iso.get("leak_scan", {}).get("statuses"),
        "canary": (iso.get("canary") or {}).get("status"),
        "anticheat": d.get("anticheat", {}).get("counts"),
    }
fa, fb = facts(a), facts(b)
for key in fa:
    print(("SAME " if fa[key] == fb[key] else "DIFF ") + key, "" if fa[key] == fb[key] else f"\n  linux {fa[key]}\n  mac   {fb[key]}")
for ta, tb in zip(sorted(a["icode"]["tasks"], key=lambda t: t["id"]), sorted(b["icode"]["tasks"], key=lambda t: t["id"])):
    print(f'{ta["id"]}: reward {ta.get("reward")} / {tb.get("reward")}  dur_s {ta.get("dur_s", 0):.0f} / {tb.get("dur_s", 0):.0f}  unscored {ta.get("unscored")} / {tb.get("unscored")}')
for tid, e in sorted((b.get("eval_protocol", {}).get("env_check") or {}).items()):
    counts = "" if e.get("mode") != "native" else f' oracle {e.get("oracle")} nop {e.get("nop")}'
    print(f'{tid}: mac {e.get("mode")} {e.get("platform")} {e.get("reason") or ""}{counts}')
PY
```

The same facts are in each run's `provenance.md`, next to `summary.md`. A merged run has one Worker line per shard. The Mac's lines end `os Darwin · Docker VM of a host with nproc … memory_kb … · containers linux/arm64` (or `linux/amd64 emulated`): `nproc` and `memory_kb` are Docker Desktop's VM, which is what the trials get, and the Mac's own follow. Its `## Environment` table shows per question whether it ran native or emulated, the native check's oracle and nop counts, and the reason for an emulated question. The Linux run has no such table.

| Row | iCode | JiuwenSwarm |
|-----|-------|-------------|
| `harness`, `input`, `questions`, `pipeline`, `harbor`, `model`, `settings` | SAME | SAME |
| `max_tokens` | SAME (`65536` unless changed) | SAME: empty on both (iCode's cap does not apply) |
| `bundle_sha256` | may DIFF: each worker prepares its own, and a Mac native shard builds for `linux/arm64` | may DIFF, for the same reasons |
| `runtime_sha256` | may DIFF in git mode: the same commit built for another CPU type | may DIFF: the same commit's offline pack for another CPU type |
| Mac `env_check` (the `mac` lines) | `native` for the two prepared questions, with oracle F2P n/n and nop F2P below n; `cliffy-config-file-parsing` `emulated` with its reason | the same |
| `leak_scan`, `canary`, `anticheat` | SAME: every rollout `clean`, canary `pass` | SAME |
| Files under each shard's `eval-runs/` | the same names | the same names (`/logs/agent/jiuwenswarm.jsonl` in each trial) |
| Reward | equal, or a difference the Linux worker also shows between two runs of that question | the same |
| Duration | Mac within 1.5× of Linux when native, within 2× under Rosetta | the same |
| OOM retries | none on either | none |

A row that should be SAME and is DIFF is a defect: stop and file it before running anything else on the Mac. Also check, on each Mac shard's console:

- `OK task containers linux/arm64 (native)` for a prepared question, `OK task containers linux/amd64 (emulated on linux/arm64)` for `cliffy-config-file-parsing`;
- `iCode under test: git pr 2 -> <sha> (<subject>)` or `JiuwenSwarm under test: git pr 2 -> <sha> (<subject>)`, with the same commit as the Linux run (release mode: `… under test: release <file> sha256 <hash>`, the same hash);
- `harness store: …` saying which shard built the `linux/arm64` tree and which reused it, and no shard building twice;
- `env check: <id> pass … reused from local#0` (prepare's answer).

The DeepSWE dispatcher's merged `summary.md` should say `Environment: 2 native, 1 emulated on macos-<hostname>` and `Emulated on macos-<hostname> (its Dockerfile downloads an x86-64-only build): cliffy-config-file-parsing`. For the two fallback runs, `provenance.md`'s Environment table gives the release reason, and the run is `linux/amd64 emulated` with no native build used.

To list each shard's files, `tar -tzf` both backups and compare the sorted paths with the build number and timestamps removed:

```bash
for f in linux.tar.gz mac.tar.gz; do tar -tzf "$f" | sed -E 's/jenkins-[0-9]+/jenkins-N/g; s/[0-9]{8}T[0-9]{6}Z/UTC/g' | sort -u >"${f%.tar.gz}.files"; done
diff linux.files mac.files && echo "OK same files"
```

**Checkpoint — paste:** the comparison output for the four pairs (two harnesses × two modes), the two fallback runs' Environment tables, and the dispatcher build URLs.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Docker cannot run linux/amd64 containers` in Environment, Tasks or Prepare harness | Rosetta is off, or Docker Desktop has not restarted since | Docker Desktop > Settings > General > Use Rosetta; Apply & restart |
| `WARNING: Docker Desktop runs linux/amd64 with QEMU, not Rosetta` | Rosetta setting off; QEMU still works but is slower | Turn Rosetta on |
| Every question `emulated` with `the iCode release under test has no linux/arm64 build` | The upload holds only `icode-linux-x86_64` | Expected for that upload. For native questions, upload the two-drop `.tar.gz` |
| Every question `emulated` with `the JiuwenSwarm release under test has no linux/arm64 build` | The upload is a `linux/amd64` bundle, an x86-64-only wheel, or a wheelhouse with a dependency that has no `aarch64` wheel | Expected for that upload. For native questions, upload a pure-Python wheel or use git mode |
| `DEEPSWE_DOCKER_PLATFORM=linux/arm64, but the … release under test has no linux/arm64 build` | Native was forced with a release that has no arm64 build | Drop `DEEPSWE_DOCKER_PLATFORM`, or upload a release with an arm64 build |
| `… holds N sets of paths that differ only in case, which a macOS worker's case-insensitive disk would merge` | A harness tree or bundle has two names that differ only by case | File it with the list; the JiuwenSwarm build already drops CPython's `share/terminfo` |
| `iCode checkout … is modified ( M uv.lock): uv sync rewrote uv.lock` | The iCode commit's `uv.lock` is out of date with its `pyproject.toml` (tag `v0.1.41`: the lock still says 0.1.40); the same on Linux | Use a commit whose lock is current (`uv lock --check` passes), such as pull request 2 |
| JiuwenSwarm build stops in `uv` with a timeout, or the build container is killed | A VPN with a small MTU, or Docker Desktop's memory too small for the 1.5 GB tmpfs plus the build | `MAC_K3D_BUILD_NETWORK=host`, or raise Docker Desktop's Memory Limit |
| `Docker Desktop has only … memory; need at least 8 GB` | Docker Desktop's memory limit below `MIN_RAM_GB` | Raise it under Settings > Resources |
| `jenkins_agent.cpu_cores is N but Docker Desktop gives containers M CPUs` from `config` | `cpu_cores` in `worker.yaml` larger than Docker's CPU limit | Lower `cpu_cores`, or raise Docker's CPUs |
| `Docker cannot mount <dir>` from `config` or 04 | The folder is outside Docker Desktop's File sharing list | Add it under Settings > Resources > File sharing, or keep `remote_fs` and the share under `/Users` |
| Node offline after a restart | Automatic login off, or FileVault on | Section 1, first row |
| `summary.md` says `Emulated on <node> (not prepared on this Mac; run mac-k3d eval prepare --benchmark deepswe)`, or `prepared for other task files or another Harbor version` | `prepare` has not run since onboarding, or since a `DEEPSWE_REF`, base or Harbor change, or a Docker prune | [Section 3](#3-prepare-the-deepswe-images-mac-only) |
| `prepare` lists `<id>: native check failed: oracle f2p a/b` | The native build grades differently from the published image; the stored fail keeps the question emulated on this Mac | File it with `~/.local/share/mac-k3d/prepare/env-check/<id>/build.log` and `built-oracle/`. After a fix, `prepare --tasks <id> --rebuild` |
| `prepare` lists `<id>: native build failed (see …/build.log); a rerun may make it native` and exits 1 | The build failed (network, disk, or `--build-context`) | Read the log, then run `prepare` again; only that question is redone |
