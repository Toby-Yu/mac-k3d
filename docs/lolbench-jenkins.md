# Jenkins job `lolbench_one_task`

The Pipeline created by `mac-k3d start` / `config` runs **iCode** on a worker with `EVAL_MODE`:

- **binary** — GitCode `-full-` tarball (`ICODE_RELEASE`), unpack, `"$icode" $ICODE_ARGS`
- **source** — git clone (`ICODE_GIT_URL`) + `uv sync` **in the job workspace**, then the same `./icode` argv

It does **not** restore LoLBench `harbor run`. This repo does not implement iCode.

**Users:** [icode-ci-new-machine.md](icode-ci-new-machine.md)  
**Tests:** [testing-icode-ci.md](testing-icode-ci.md)

After changing the generator in `src/prepare/jenkins_job.rs`, re-run:

```bash
mac-k3d config -c ~/.config/mac-k3d/config.yaml
```

## Parameters

| Parameter | Meaning |
|-----------|---------|
| `EVAL_MODE` | `binary` or `source` |
| `TASK` | Exported as `TASK` and `ICODE_TASK` |
| `ICODE_RELEASE` | Binary: `icode-<os>-<arch>-full-vX.Y.Z.tar.gz` URL or path, or a stub `icode` (test target: **v0.1.41**) |
| `ICODE_GIT_URL` | Source: iCode git URL (private clone uses `gitcode-pat`) |
| `ICODE_GIT_REF` | Source: branch or tag (default `main`) |
| `ICODE_ARGS` | Argv after `./icode` (smoke: `--help`; README also documents `tui`) |
| `AGENT_LABEL` | Default `lolbench` |
| `CPU_LOCK_QTY` | Lockable `CPU_CORES` |

Binary HTTP(S) fetches send `PRIVATE-TOKEN: $GITCODE_TOKEN` when that credential is bound. Slim (non-`-full-`) tarball names log a warning.

Source of truth: [`src/prepare/jenkins_job.rs`](../src/prepare/jenkins_job.rs).
