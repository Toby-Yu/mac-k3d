#!/usr/bin/env python3
"""Back up one run into the repo and pack it as ``<folder>.tar.gz`` (archive phase).

The backup holds the report files, the anti-cheat verdicts and, per trial,
the files listed below. Transcripts are gzipped with key-shaped values and
this process's secret env values masked; ``.harbor-env`` is never copied.
"""

from __future__ import annotations

import argparse
import gzip
import os
import re
import shutil
import tarfile
from pathlib import Path

REPORT_FILES = (
    "artifact.json",
    "summary.md",
    "report.html",
    "cost-token-report.md",
    "container_mem.jsonl",
    "skipped_questions.txt",
)

_TRIAL_FILES = {
    "result.json",
    "reward.json",
    "agent_report.json",
    "usage.json",
    "notes.txt",
    "icode-usage.json",
    "icode.json",
    "icode.txt",
    "timing.json",
    "agent.patch",
    "model.patch",
    "capture.json",
    "capture_flags.json",
    "base_sha.txt",
    "anticheat.json",
    "trial.log",
}
_GZIP_TRIAL_FILES = {"events.jsonl"}

_KEY_SHAPED = re.compile(r"sk-[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|glpat-[A-Za-z0-9_-]{20,}")
_SECRET_ENV = ("DEEPSEEK_API_KEY", "OPENAI_API_KEY", "ICODE_API_KEY", "GITCODE_TOKEN", "GITHUB_TOKEN")


def default_backup_root() -> Path:
    for key in ("MAC_K3D_BACKUP_ROOT", "MAC_K3D_OUTPUT_ROOT"):
        override = os.environ.get(key)
        if override:
            return Path(override)
    root = os.environ.get("MAC_K3D_ROOT")
    if root:
        return Path(root) / "output"
    return Path(__file__).resolve().parents[2] / "output"


def archive_transcript(src: Path, target: Path) -> None:
    """Gzip a transcript, masking key-shaped values and this process's secret env values."""
    secrets = [v for v in (os.environ.get(k) or "" for k in _SECRET_ENV) if len(v) >= 8]
    with src.open("r", encoding="utf-8", errors="replace") as fh, gzip.open(target, "wt", encoding="utf-8") as out:
        for line in fh:
            for value in secrets:
                line = line.replace(value, "***")
            out.write(_KEY_SHAPED.sub("***", line))


def _copy_trial_files(trial: Path, dest: Path) -> None:
    if not trial.is_dir():
        return
    dest.mkdir(parents=True, exist_ok=True)
    for path in trial.rglob("*"):
        if not path.is_file():
            continue
        if path.name == ".harbor-env" or ".harbor-env" in path.parts:
            continue
        target = dest / path.relative_to(trial)
        if path.name in _GZIP_TRIAL_FILES:
            target = target.with_name(target.name + ".gz")
            target.parent.mkdir(parents=True, exist_ok=True)
            archive_transcript(path, target)
            continue
        if path.name not in _TRIAL_FILES and path.suffix != ".patch":
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)


def backup_run(
    *,
    backup_root: Path,
    suite: str,
    task_ids: list[str],
    report_dir: Path,
    run_folder: str,
    harness_dir: Path,
) -> Path:
    dest = backup_root / suite / run_folder
    dest.mkdir(parents=True, exist_ok=True)
    for name in REPORT_FILES:
        src = report_dir / name
        if src.is_file():
            shutil.copy2(src, dest / name)
    (dest / "tasks.txt").write_text("".join(f"{tid}\n" for tid in task_ids), encoding="utf-8")
    verdicts = harness_dir / "anticheat"
    if verdicts.is_dir():
        shutil.copytree(verdicts, dest / "anticheat", dirs_exist_ok=True)
    from score_results import harbor_task_trials

    for tid in task_ids:
        for index, trial in enumerate(harbor_task_trials(harness_dir, tid), start=1):
            _copy_trial_files(trial, dest / "icode" / tid / f"attempt-{index:02d}")
    return dest


def compress_backup(dest: Path) -> Path:
    """Pack a run folder into ``<folder>.tar.gz`` and remove the loose directory.

    The archive's top entry is the folder name, so extracting recreates the
    same layout. A failed pack deletes the temporary archive and leaves the
    folder in place.
    """
    archive = dest.parent / f"{dest.name}.tar.gz"
    tmp = dest.parent / f".{dest.name}.tar.gz.tmp"
    try:
        with tarfile.open(tmp, "w:gz") as tar:
            tar.add(dest, arcname=dest.name)
        tmp.replace(archive)
    except Exception:
        tmp.unlink(missing_ok=True)
        raise
    shutil.rmtree(dest)
    return archive


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--report-dir", required=True, help="run folder the report phase wrote")
    ap.add_argument("--harness-dir", required=True)
    ap.add_argument("--task-file", required=True)
    ap.add_argument("--suite", default=os.environ.get("BENCHMARK", "deepswe"))
    ap.add_argument("--backup-root", default="")
    args = ap.parse_args(argv)
    report_dir = Path(args.report_dir)
    if not (report_dir / "artifact.json").is_file():
        ap.error(f"no artifact.json in {report_dir}; run the report phase first")
    ids = [ln.strip() for ln in Path(args.task_file).read_text(encoding="utf-8").splitlines() if ln.strip()]
    dest = backup_run(
        backup_root=Path(args.backup_root) if args.backup_root else default_backup_root(),
        suite=args.suite,
        task_ids=ids,
        report_dir=report_dir,
        run_folder=report_dir.name,
        harness_dir=Path(args.harness_dir),
    )
    print(f"backup {compress_backup(dest)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
