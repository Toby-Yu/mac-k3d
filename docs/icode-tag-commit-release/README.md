# Branch process: `feat/icode-tag-commit-release`

This folder is **branch process and re-test notes**, not the user start-here. User docs stay under [docs/](../) ([user-guide](../binary-initializer/user-guide.md), [icode-harness-inputs](../binary-initializer/icode-harness-inputs.md)). Lab track for the other branch stays [binary-initializer/testing/](../binary-initializer/testing/).

## What changed on this branch

1. **Git kinds** — Jenkins/CLI pick `branch` / `tag` / `commit` (default `branch`). Leftover `auto` still maps (7–40 hex → commit, else branch). P3 records `icode_git.json`; reports copy `icode_git`.
2. **No local source mode** — workers do not hold a developer checkout. `ICODE_MODE=source` is rejected. Inputs are git clone or a downloaded `*-full-*` / `icode` drop.
3. **Jenkins release = upload** — job XML has File Parameter `ICODE_RELEASE_FILE` (not a string worker path). **Release** mode exports `ICODE_RELEASE` + `ICODE_RELEASE_UPLOADED=1` when that file is non-empty. **Git** mode `--force-rm`s a leftover upload and ignores it. P3 unpacks gzip/tar by magic, else copies as `icode`. Empty release upload fails with a clear message.
4. **Git leftover clone** — P3 wipes `eval-runs/icode-src` before clone. Root/nobody files from a prior Pier/Harbor bind-mount are removed with Docker (no `sudo rm`). P5 chowns the bind-mount back to the agent user.
5. **Local CLI unchanged for paths** — `mac-k3d eval --local` / `--stage` still take `--icode-release PATH`, persist (`mac-k3d set --icode-release`), or `~/.local/share/mac-k3d/` discover.
6. **CLI `--yes` + release** — does **not** queue GET `buildWithParameters` (cannot attach a file). Prints: open Jenkins UI, set `ICODE_MODE=release`, upload `ICODE_RELEASE_FILE`. Git `--yes` still queues; product path is still **Build with Parameters**.

## How to re-test

1. Worker: extract pipeline (`mac-k3d eval --stage p0` or `mac-k3d config -c worker.yaml`).
2. Controller: rebuild/install CLI, then `mac-k3d config --skip-secrets` so job XML has `ICODE_RELEASE_FILE`.
3. Fixtures: `bash pipeline/stages/test_icode_input.sh` and `bash pipeline/stages/test_p3_icode.sh`.
4. Live Jenkins: **Build with Parameters** → `ICODE_MODE=release` → upload the official drop → look for console lines below.
5. Live git: same UI → `ICODE_MODE=git` → URL / ref / kind; leave the file picker as it is. Console: `OK iCode git kind=…`. Optional: `removing leftover … via docker`.

Step-by-step log: [testing.md](testing.md).
