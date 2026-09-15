# Jenkins job `lolbench_one_task` (leftover)

This job is **not** LoLBench and **not** the evaluation. Harbor / LoLBench stay skip.

**Eval:** job **`icode_eval`** on DeepSWE. Operator steps: [binary-initializer/user-guide.md](binary-initializer/user-guide.md).

`mac-k3d start` / `config` on the controller may still create `lolbench_one_task` (legacy iCode argv smoke: `EVAL_MODE` binary/source, `ICODE_RELEASE` tarball URL, `ICODE_GIT_URL`). Ignore it. Do not Build it for DeepSWE.

Source of truth for both jobs: [`src/prepare/jenkins_job.rs`](../src/prepare/jenkins_job.rs).
