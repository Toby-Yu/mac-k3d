# Store eval runs on a NAS

The NAS keeps finished runs. Harbor, Docker, and the Jenkins workspace stay on the worker's own disk.

Copy only after the build has already saved its Jenkins artifacts. If the NAS is down, the eval still stands.

## Two copies

| When | From | Put on the NAS |
|---|---|---|
| A shard's `archive` phase has printed `compress: N transcripts gzipped and masked` | That `deepswe_one_task` workspace | The gzipped transcripts and the small files next to them |
| The dispatcher `Aggregate` stage has written `backup/<suite>/<RUN_GROUP>.tar.gz` | The worker that ran Aggregate | The merged report and that tar |

Do not copy during `env`, `tasks`, `evaluate`, `anticheat`, `score`, or `report`.

## Shard copy

From `deepswe_one_task` build `N` only. Skip older builds left in the same workspace.

- `eval-runs/harness/harbor_runs/jenkins-<N>/**/events.jsonl.gz`
- `agent.patch` and `model.patch` in that same tree
- `eval-runs/harness/anticheat/**`
- `eval-runs/selected_tasks.txt`
- `eval-runs/skipped_tasks.txt`
- `eval-runs/eval_protocol_inputs.json`
- `eval-runs/eval_resources.json`

On this PC the workspace is `/home/Toby/jenkins-agent/workspace/deepswe_one_task`. On the cloud worker it is `/home/toby/jenkins-agent/workspace/deepswe_one_task`.

## Report copy

From the dispatcher workspace (`deepswe_some_task` or `deepswe_full_suite_task`):

- `backup/<suite>/<RUN_GROUP>.tar.gz`
- `aggregate/artifact.json`
- `aggregate/summary.md`
- `aggregate/report.html`
- `aggregate/cost-token-report.md`

The tar has the scores, patches, and anti-cheat verdicts. It does not have `events.jsonl.gz`. Those are the shard copy.

## Leave these on the worker

- Docker's data directory
- `eval-runs/deep-swe/`, `eval-runs/lolbench/`, `eval-runs/icode-bin/`
- `mac-k3d-pipeline/`
- `.harbor-env` and any API key
- A plain `events.jsonl` (the archive step has already replaced it with `.gz`)
- `aggregate/harness/` (a second copy of the trials)

## Layout

```text
/mnt/mac-k3d-archive/
  reports/<suite>/<icode-sha>/<RUN_GROUP>/
    artifact.json
    summary.md
    report.html
    cost-token-report.md
    <RUN_GROUP>.tar.gz
  transcripts/<suite>/<icode-sha>/<RUN_GROUP>/shard-<build>/
    events.jsonl.gz
    *.patch
    anticheat/
    selected_tasks.txt
    eval_protocol_inputs.json
    eval_resources.json
```

`<RUN_GROUP>` is the value the dispatcher prints, such as `deepswe_full_suite_task-12`. `<icode-sha>` is the iCode commit in that run's report. One directory per commit. Read `artifact.json` to compare loop iterations. Do not merge trials from two commits into one folder.

## Prepare the NAS

Put it on the controller's private network. A disk on this PC alone is not a store for the cloud worker.

- Two disks in RAID 1, with snapshots.
- One export, mounted on each worker at `/mnt/mac-k3d-archive`.
- Mount option `nofail`, so a missing NAS does not stop the agent.
- The agent user writes (`Toby` on this PC, `toby` on the cloud worker). Mode `0750`. No guest access.
- Do not mount it as the Jenkins workspace or as Docker's data root.
- Do not point `MAC_K3D_BACKUP_ROOT` at the NAS yet. A backup written outside the workspace is not offered on the Jenkins build page. Jenkins keeps the copy that must succeed. The NAS copy comes after that.

## How long to keep it

| Folder | Keep |
|---|---|
| `reports/` | Every run |
| `transcripts/` | 90 days, or the last three runs of that iCode commit |

Jenkins still stores every shard, including the transcripts. After a shard's transcript copy is on the NAS and the size matches, that `.gz` can be dropped from the controller later. Keep the dispatcher report.

## After each copy

- `artifact.json` opens, and its iCode sha matches the directory name.
- Every shard named on the report's `Transcripts:` line has a `shard-<build>/` directory.
- One opened `events.jsonl.gz` shows `***` in place of a key, and there is no `.harbor-env`.
- The worker still has local room for one uncompressed suite (about 26 GB) plus images. The NAS does not replace that disk.
