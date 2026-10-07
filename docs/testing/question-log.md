# Question run log

One row per question per eval run, newest first. `mac-k3d eval record --job <job> --build <n>` adds a Jenkins build; every `mac-k3d eval --local` run adds itself when it ends, passed or failed. Coverage per question (which ones ever ran) stays in [question-coverage.md](question-coverage.md).

Only **fix** is yours: write what you changed or found. Re-recording a run rewrites the other cells and keeps **fix**. Problem text is masked for keys and tokens and cut to 200 characters; open the build's console or `summary.md` for the rest.

Results: `pass c/n` (every rollout resolved), `fail c/n` (scored, not all resolved), `unscored` (no verifier result), `skipped`, `failed` (the run stopped before scoring this question), `canary pass` / `no score` (a `CANARY=only` run; Open problems skips it). A `(run)` row is a run that stopped before it selected any question.

## Open problems

The latest result of each question that is not `pass`, with the newest fix note written for it.

<!-- question-log:open:begin -->
| question | benchmark | last run | worker | result | stage | problem | fix |
| --- | --- | --- | --- | --- | --- | --- | --- |
| none | | | | | | | |
<!-- question-log:open:end -->

## Runs

| date (UTC) | run | worker | benchmark | question | result | stage | problem | fix |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-10-07 02:08 | deepswe_one_task #54 | mac-iZt4ndd2dff7gqjta7mppaZ (docker 29.6.2) · egress probe substituted | deepswe | ipython-session-bundle-replay | pass 1/1 | done | - | |
| 2026-10-06 08:56 | local 20261006T085615Z | Michael-Ubuntu (docker 29.1.3) | deepswe | abs-module-cache-flags | pass 1/1 | done | - | |
