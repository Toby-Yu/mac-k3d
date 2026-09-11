# Testing the iCode eval pipeline (P0–P8)

Per-stage CLI checks for Process 2 (DeepSWE + Pier + iCode vs DeepSeek baseline).  
Machine bootstrap first: [testing-binary-initializer.md](testing-binary-initializer.md) and [workflow.md](workflow.md).

**Sign-off:** pass P0–P8 with `--n-tasks 1` where relevant, then run the full runner once.

Scripts live under [`scripts/eval/`](../../scripts/eval/). The CLI wraps them:

```bash
mac-k3d eval --stage p0
mac-k3d eval --stage p5 --n-tasks 1
mac-k3d eval --local --n-tasks 1    # full local pipeline after stages pass
mac-k3d eval                        # interactive → Jenkins job icode_eval
```

Default workdir: `./eval-work` (override with `MAC_K3D_EVAL_WORKDIR`).  
Default iCode source: `/home/Toby/Documents/Toby/iCode-main` (override with `ICODE_SOURCE`).

---

## P0 — Docker, CLI, worker readiness

**Why:** Without Docker and an online agent (or local Docker for `--local`), Pier cannot run sandboxes.

```bash
mac-k3d eval --stage p0
# or: scripts/eval/p0_prereqs.sh
```

**Expected:** `docker info` shows Server; `mac-k3d --help` lists `eval`; optional note if Jenkins worker is offline (OK for `--local`).

## P1 — Pier on PATH

**Why:** DeepSWE is Pier/Harbor-format; Pier builds task images and runs agents.

```bash
mac-k3d eval --stage p1
```

**Expected:** installs via `uv tool install datacurve-pier` if missing; `pier --help` works.

## P2 — DeepSWE clone

**Why:** Tasks (Dockerfile, instruction, tests) are not in this git repo.

```bash
mac-k3d eval --stage p2
```

**Expected:** `$WORKDIR/deep-swe/tasks` exists after shallow clone of `https://github.com/datacurve-ai/deep-swe`.

## P3 — iCode binary or source

**Why:** Arm A must invoke the harness under test.

```bash
# binary mode
ICODE_MODE=binary ICODE_RELEASE=/path/to/icode-*-full-*.tar.gz mac-k3d eval --stage p3
# source mode (default path)
ICODE_MODE=source ICODE_SOURCE=/home/Toby/Documents/Toby/iCode-main mac-k3d eval --stage p3
```

**Expected:** unpacked/`uv run` `icode --help` succeeds.

## P4 — Pier sees agent `icode`

**Why:** Custom agent install + DeepSeek allowlist must register with Pier.

```bash
mac-k3d eval --stage p4
```

**Expected:** agent package under `eval/pier-agent-icode` is discoverable; install script is executable.

## P5 — One DeepSWE task through iCode (N=1)

**Why:** End-to-end harness path (expensive: image build + LLM).

```bash
export DEEPSEEK_API_KEY=…   # or rely on Jenkins credential when not --local
mac-k3d eval --stage p5 --n-tasks 1
```

**Expected:** `PROGRESS` lines; Pier run for one task; patch or trajectory under workdir. Failures with clear pier/docker errors still count as “stage ran”.

## P6 — Same task via DeepSeek API only

**Why:** Baseline without iCode scaffolding.

```bash
mac-k3d eval --stage p6 --n-tasks 1
```

**Expected:** `instruction.md` posted to DeepSeek; optional `.patch` written under `baseline/`.

## P7 — Score f2p / p2p into temp JSON

**Why:** Compare harness vs baseline using verifier signals.

```bash
mac-k3d eval --stage p7
```

**Expected:** temp JSON with per-task `harness_resolved`, `baseline_resolved`, `f2p`, `p2p` (lists may be empty if the task only exposes a single verdict).

## P8 — Named file under `output/`

**Why:** Stable identification of which eval ran.

```bash
mac-k3d eval --stage p8 --n-tasks 1
```

**Expected:** `output/eval-icode-deepseek-deepswe-n1-<utc>.json` (or under `$WORKDIR/output/`).

---

## Full runner

After P0–P8:

```bash
mac-k3d eval --local --n-tasks 1
# or trigger Jenkins:
mac-k3d eval --n-tasks 1 --icode-mode source
```

**Expected:** Jenkins `icode_eval` (or local) runs both arms, prints progress, archives JSON.

---

## Troubleshooting

| Message | What to do |
|---------|------------|
| pier not found | `uv tool install datacurve-pier` or `uv tool install git+https://github.com/datacurve-ai/pier` |
| DeepSWE clone fails | Network / git; retry P2 |
| DEEPSEEK_API_KEY missing | Set env for `--local`, or store `deepseek-api-key` on the controller ([secrets.md](../secrets.md)) |
| Docker OOM / disk | DeepSWE images are large; free disk; lower N |
| Worker offline | Finish bootstrap T3; for local-only tests use `--local` |
