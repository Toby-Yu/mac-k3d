# Cloud worker repair: step by step

- **For:** Toby, as root on the cloud worker `iZt4ndd2dff7gqjta7mppaZ` (`47.84.16.18`), with root watching. Root and the mentor approved the repair after the investigation.
- **Goal:** one image store, on the 14 TB disk, in which every record has its file or folder. The 12 base images, the 113 published DeepSWE images and the 418 jiuwenswarm images start. Guards stop the 12 Oct failure from happening again.
- **Record:** everything on the worker is recorded in `/var/log/cloud-worker-repair/<date>/` and copied to your PC at the end (step 16). The [repair tracker](#repair-tracker) is the checklist: a step starts only when the step before it is PASS.
- **Based on:** [`CLOUD_WORKER_MENTOR_REPORT.md`](CLOUD_WORKER_MENTOR_REPORT.md), section 9, and [`CLOUD_WORKER_INVESTIGATION_STEPS.md`](CLOUD_WORKER_INVESTIGATION_STEPS.md). Numbers are from the 15 Oct inventory unless a step says otherwise. How the store fits together is in the investigation guide, "What you are looking at".
- **Status:** 21 Oct 11:23 CST. The 15 `lolbench-linux` digests are back. `sha256:cec9f7d0…` retried at 11:23 and printed `Status: Downloaded newer image`. Lolbench is 264 of 335. The other 71 names are local builds and stay with the owner. capbench is 4 of 67, and the missing 63 are local builds. sweap (19.5) waits until that benchmark is the one to run. The progress table is in [`CLOUD_WORKER_REPAIR_REPORT.md`](CLOUD_WORKER_REPAIR_REPORT.md).
- **Name:** on 20 Oct 2026 the project was renamed from mac-k3d to beacon in a codebase migration. Records of what the server showed before that rename keep the old name. Commands run after it use `beacon`.

## Read this first: why remove-then-pull is not enough on this store

The approved plan repairs an image with `docker rmi`, then `docker pull` (mentor report, section 9.1). That works only when no other image, container or cache entry names the image's broken records. On this store, many do. Three rules of containerd 2.2 decide it:

1. A pull does not download a file whose download record already exists. It prints `Already exists`, even when the file is gone.
2. Unpacking skips a layer whose layer record already exists, even when its snapshot folder is gone.
3. containerd's cleanup keeps a record while any image, container, build-cache entry or child layer still names it. `docker rmi X` removes the name `X`. The records that X shares with other images stay.

These rules come from containerd's source code (its content writer and its unpacker). They were not tested on this host. Steps 9.2 and 10.2 are the test: if the rules are wrong, those checks fail and the guide stops.

From the 15 Oct inventory:

| Group | What remove-then-pull would leave behind |
|---|---|
| `ubuntu:24.04` | 1 of its 7 missing files is also named by 113 `*-pier-egress-proxy` task images. The pull skips that file, and the image still fails |
| jiuwenswarm, 418 images | 1,249 of its 4,903 missing files are also named by 625 other images, mostly `chrys-bench-agent`. These images are built on this server, so "pull" means a rebuild, and the rebuild meets the same records |
| Published DeepSWE, 113 | No file is missing. But its layer records are the parents of 15,572 other layer records: 7,874 layers of 4,700 other images, 6,807 build-cache entries and 891 container layers. After `docker rmi`, those keep DeepSWE's broken layer records alive, so a pull or a load unpacks nothing new |
| `alpine:3.19`, `ubuntu:22.04` | Only the image itself and its own digest alias name the missing files. Remove-then-pull would work for these two |

**The method in this guide.** It ends where the approved plan ends, by a different path:

1. Save every image that still has all its downloaded files into tar files: the 113 published DeepSWE images and 140 local images, about 28 GB together. The 113 share their base layers, so their own data is about 24 GB. The 120 GB in the mentor report is the sum of their sizes.
2. Remove what cannot work now: every container that cannot mount its layers, every image that fails the start test (about 7,500 names), and Docker's build cache. Then remove the download records whose file is gone.
3. Let containerd's cleanup remove every record that nothing names any more. Check the store: no download record without its file, and no more layer records than snapshot folders.
4. Only then pull the 12 base images and load the saved images back. The jiuwenswarm owner then rebuilds.

**What changes against the approved plan:**

| | Approved plan | This guide |
|---|---|---|
| How a broken image is fixed | `docker rmi`, then `docker pull`, one group at a time | Remove everything that already fails, let containerd clear the records nothing names, check, then pull or load |
| Published DeepSWE | Unpacked again from the files on disk | Saved to a tar file first, then loaded back |
| Other users' images | Each owner removes and re-pulls later | Images that fail now are removed in the window, once, by root. Owners still re-pull or rebuild later. The 140 that can be saved come back in step 11 |
| Build cache | Not mentioned | Removed. Only build speed is lost |
| Step 1, one store | Unmount `/data`, move the hidden store, `chattr +i`, mount again | The same end state through a second view of the system disk, without unmounting `/data`, so the TaskAnchor runtime and other `/data` users keep running. If root prefers the unmount, the rest of the guide is unchanged |
| jiuwenswarm | Inside the 8 hours, with the server empty | Rebuilt by its owner after the store passes its checks (step 15). Root decides in step 0.1 whether everyone else waits until it passes |

Without the written OK of step 0.1, stop there.

## Before you start

### How each step is written

- **Where:** your PC, or the recorded root session on the worker.
- **Why:** what the step does, and why the repair needs it.
- **Risk:**
  - **none:** it only reads, and writes only into the repair log folder;
  - **low:** a change that removes no data and can be undone; the step says how;
  - **high:** it stops other users' work, or removes records or data that can't be put back.
- **Command:** copy the whole grey block and paste it into the terminal. Text after `#` is a comment.
- **Expected:** what you should see.
- **Pass check:** what must be true before the next step. Record it with `mark <step> PASS`. If it fails, record `mark <step> FAIL "<what you saw>"` and stop.
- **Achieves:** what is true after the step.
- **Send me:** what to paste back into the chat.
- **Result:** filled in after the run, as in the investigation guide.

### Ground rules

1. One person types, in one recorded root session. Root watches, and gives the OK at three gates: steps 0.1, 6.4 and 17.
2. Copy the blocks exactly. The only text you change is the note in a `mark` line, and the `<…>` placeholders in messages.
3. Wait for the prompt before you paste the next block. Long steps say how long they take.
4. If the output differs from **Expected**, or a **Pass check** fails: `mark <step> FAIL "<what you saw>"`, stop, and send me the output. Don't try a fix, and don't skip ahead.
5. None of these is in this guide, and none may be typed: `docker system prune`, `docker image prune`, `rm` under `/data/containerd` or `/data/docker`, editing either database (the only exception is step 8.10, which deletes child names that are not layer records, and only after its own copy), `mount` or `umount` of `/data`, a reboot (except step 18, in its own window), and `kill -9` of containerd or Docker.
6. Never paste a password or the contents of `root.pem` into the chat or anywhere else. Fingerprints (`SHA256:…`) are fine.
7. Work inside `tmux`. A dropped connection then doesn't stop a long step (step 1.2).

### Know where you are

| Prompt ends with | Where you are | Care |
|---|---|---|
| `Toby@<your PC>:~$` | Your PC | Normal |
| `root@iZt4ndd2dff7gqjta7mppaZ:~#` | The worker as root, not recorded | Only steps 1.2 and 1.3, which start the recording. Anything else: start the recording first |
| `[repair 09:12:30] root@iZt4ndd2dff7gqjta7mppaZ:~#` | The worker as root, recorded, inside `tmux` | Every worker step from 1.4 on. Root has no limits: type only this guide's blocks |

The green bar at the bottom of the window means you are inside `tmux`; it shows `[repair]` on the left. The keys you need:

- `Ctrl-b d` leaves `tmux` running in the background; `tmux attach -t repair` returns to it.
- `Ctrl-b c` opens a new window; `Ctrl-b 0` and `Ctrl-b 1` switch between windows.
- `Ctrl-b [` scrolls back (arrow keys, PageUp); `q` ends scrolling.

Terminals:

- **Terminal 1:** the worker, `tmux` session `repair`. Window 0 is for every step; window 1 (step 1.6) is for watching long steps.
- **Terminal 2:** your PC, for messages, Jenkins, and the copy of the logs at the end.

### How the repair is logged

Everything lives in one folder, `$R` = `/var/log/cloud-worker-repair/<date>`, on the system disk, readable by root only.

| Record | Path in `$R` | What it holds |
|---|---|---|
| Screen recording | `sessions/<time>-main.log`, `sessions/<time>-watch.log` | Everything that appeared on screen, from step 1.3 on |
| Command history | `commands.history` | Every command typed, each with a `#<seconds>` time line before it |
| Step output | `out/<step>_*.txt`, `out/*.tsv`, `out/*.txt` | The full output of each step, also when the screen shows only a summary |
| Checklist | `checklist.tsv` | One line per `mark`: time, step, PASS, FAIL, APPROVED or INFO, and a note |
| Scripts | `bin/` | The three scripts exactly as they ran |
| Service logs | `journal_window.txt` | containerd's and Docker's journal for the whole window (step 16.1) |
| Checksums | `SHA256SUMS` | A checksum of every file above (step 16.2) |

At the end, the folder is packed into `<date>.tar.gz` and copied to `~/cloud-worker-evidence/repair/` on your PC. Its checksum goes to root and the mentor, so either of them can check that a copy is unchanged. Passwords aren't shown on screen, so they aren't recorded. The database copies and the saved images are not in the log folder.

In the HTML page, the boxes in the tracker can be ticked. The ticks are kept in your browser only. The record that counts is `checklist.tsv` on the worker.

### What can't be undone, and what protects it

| Step | What is removed or stopped | Why that is acceptable | What protects it |
|---|---|---|---|
| 3.1 | Every running container stops | `live_restore` is false, so any Docker restart does this | Announced in 0.2. Containers with a restart policy start again in 5.2. Step 12.3 lists the others for their owners |
| 8.1 | Containers that cannot mount their layers, with their writable layer | They cannot start, and nothing can be copied out of them | Owners are told in 0.2. The lists are in `out/` |
| 8.2 | Docker's build cache | It only makes builds faster | None needed |
| 8.3 | About 7,500 image names that fail the start test, and the files that only they use | They don't start now, and most of their files are already gone | The 253 that still have every file are in tar files (step 7). Owners pull or rebuild the rest |
| 8.4 | Download records whose file is gone | The file is already gone. Without the record, a pull downloads it again | Each digest is checked twice, just before |
| 8.10 | Child names in `moby` that are not layer records | The layer record is already gone. The name is only a leftover on its parent, and it blocks removal of that parent | A copy of `meta.db` is taken first. If the following image check fails, that copy is put back before any layer record is removed |
| 8.11 | Folderless layer records that no Docker image and no active container uses | The folder is already gone. A later pull would skip that layer while the record remains | Run only after 8.10's image check passes. Records in an image chain or an active container chain are not in the list |
| 8.11b | The unused child records under the last 42, including snapshot folders those children still have | No current image and no active container uses them, and they are what still block the folderless parents | The list is built only from the 42. The step removes nothing if any name is in an image chain or an active container chain |
| 4.2 | Nothing. The hidden store is moved, not deleted | | A root-only archive on the system disk |

The copies of the two databases (step 3.3) are for the review. They are not a way back: after step 8, the files they name are deleted.

### Time

| Steps | Work | Time | Who may use the server |
|---|---|---|---|
| 0 | Approvals, announcement, read-only check | The day before | Everyone |
| 1 to 2 | Recording, the state before | 30 min | Nobody, as announced |
| 3 to 5 | Stop, one store, start | 30 min | Nobody. Docker is closed to the `docker` group from 5.2 |
| 6 | Test and sort | 45 to 60 min | Nobody |
| 7 | Save 253 images | 20 to 40 min | Nobody |
| 8 | Remove, and containerd's cleanup | 30 min to 3 h | Nobody. The cleanup deletes the files and folders of about 7,500 images; how long depends on how much of them is still on disk |
| 8.9 to 8.12 | Match check, clear leftover child names, remove folderless records, match again | 40 to 70 min | Nobody. Docker is stopped for a few minutes in 8.10, then started again |
| 9 to 12 | Bases, DeepSWE, the other saved images, final check | 1 to 1½ h | Nobody |
| 13 to 14 | Guards, reopen | 30 min | The jiuwenswarm owner, or everyone, as root decided in 0.1 |
| 15 | jiuwenswarm rebuild | The owner's estimate; the approved plan reserved 4 h | As root decided in 0.1 |
| 16 | Hand over the logs | 15 min | Everyone |
| 19 | Optional pulls of published images Toby can restore | 8 to 16 h if every tag is still published. The small official images are under 2 h | Everyone. Docker is open. A large pull uses disk and bandwidth |

Steps 1 to 14 take about 4½ to 8 hours; most of the spread is step 8.

## Repair tracker

Tick a box only after the step's **Pass check** holds and `mark <step> PASS` is in the checklist.

| Step | What the step does | What to check | Pass when | Pass |
|---|---|---|---|---|
| 0.1 | Method approved | Written replies | Root and the mentor OK, with answers to (a) to (d) | ☐ |
| 0.2 | Window announced | Message sent, deadline passed | No open objection | ☐ |
| 0.3 | Read-only check of key and server | Fingerprint, tools, space, mount, socket | No `MISSING`; `/` at least 10 GB free; `/data` at least 200 GB free; no TCP listener; `live_restore=false` | ☐ |
| 1.1 | Jenkins node offline | The node's page in Jenkins | Offline, no build running | ☐ |
| 1.2 | Root shell inside `tmux` | Prompt and green bar | `root@iZt4ndd2dff7gqjta7mppaZ` inside `tmux` | ☐ |
| 1.3 | Recording started | New prompt | No error | ☐ |
| 1.4 | Helpers loaded, approvals recorded | Prompt, `mark` line | Prompt starts with `[repair`; the 0.1 to 0.3 lines are in `checklist.tsv` | ☐ |
| 1.5 | Repair scripts saved | `bash -n` | `scripts OK` | ☐ |
| 1.6 | Watch window (optional) | Window 1 | Its prompt starts with `[repair` | ☐ |
| 2.1 | Nobody still working | `who`, Docker clients | No Docker client running; the running containers were announced | ☐ |
| 2.2 | Mount and guards before | `findmnt`, fstab, drop-ins | `/data` is the UUID disk; 4 guard lines; `live_restore=false` | ☐ |
| 2.3 | Store before | `store_check.sh` | `open_meta.db` open equals on disk; counts recorded | ☐ |
| 2.4 | Lists before | `wc -l` | Containers, running, images listed | ☐ |
| 2.5 | Canary paused | `crontab -l` | `no crontab for toby` | ☐ |
| 3.1 | Docker and containerd stopped | `is-active`, MainPID | `inactive` three times, then `0` | ☐ |
| 3.2 | Databases closed | `fuser` | No process | ☐ |
| 3.3 | Database copies | `sha256sum` | Both pairs equal | ☐ |
| 4.1 | Writable view of the system disk | `findmnt`, listing | `rw`; only `containerd`, `docker`, `hanxu`; about 1.7 GB | ☐ |
| 4.2 | Hidden store archived | Count, archive listing | `0` left; three folders in the archive | ☐ |
| 4.3 | Empty folder immutable | `lsattr`, `touch`, `mkdir` | `i` flag; both refused | ☐ |
| 4.4 | View removed, `/data` unchanged | `findmnt`, `diff` | `view removed`; `same top-level entries on /data` | ☐ |
| 5.1 | containerd on the 14 TB database | `store_check.sh --quick` | `open_meta.db` open equals on disk; `RESULT OK` | ☐ |
| 5.2 | Docker up, closed to users | `stat`, `runuser` | `root:root 660`; `toby` refused; root works | ☐ |
| 5.3 | What restarted by itself | `docker ps` | Recorded; no Docker client running | ☐ |
| 6.1 | Start test of every image | Counts | Every name tested; `other` and `timeout` few | ☐ |
| 6.2 | Mount test of every container | Counts | Every container tested; `other` and `timeout` few | ☐ |
| 6.3 | Remove, keep, review and save lists | `wc -l` | `save_deepswe.txt` 113; counts close to 15 Oct, or explained | ☐ |
| 6.4 | Root approves the removal | Root's reply | `APPROVED` line in the checklist | ☐ |
| 7.1 | Free space | `df` | At least 200 GB available | ☐ |
| 7.2 | DeepSWE tar | Output, size | No error; about 24 to 26 GB | ☐ |
| 7.3 | Other images tar | Output, size | No error, or the fallback with `0` failed | ☐ |
| 7.4 | Tar files checked | Image counts, checksums | Counts equal the save lists | ☐ |
| 8.1 | Broken containers removed | Counts | All removed, no `Error` | ☐ |
| 8.2 | Build cache removed | Second `Total` | `0B` | ☐ |
| 8.3 | Failing images removed, cleanup done | Counts | All removed, no error; the `--sync` call returned | ☐ |
| 8.4 | Records without file removed | `content active`, output | No download running; no error | ☐ |
| 8.5 | Store check | `store_check.sh`, kept images | `records_without_blob 0`; layer records not more than folders; every kept image `ok` | ☐ |
| 8.6 | Build-cache leases removed | Lease count, store check | 6,929 leases removed; no error; layer-record gap smaller | ☐ |
| 8.7 | Folderless layer records removed | Removal log, store check | 288 records removed; images still 523; layer gap smaller | ☐ |
| 8.8 | Child records of those layers removed | Removal log, store check | Not a pass. Do not re-run. Continue at 8.9 | ☐ |
| 8.9 | Every Docker image matches its files; leftover child names counted | `images check`, orphan count | 523 images, `bad 0`; orphan count 25,442 | ☐ |
| 8.10 | Leftover child names cleared | Program output, image check | `deleted 25442 remaining 0`; 523 images, `bad 0` | ☐ |
| 8.11 | Folderless layer records nothing still needs removed | Removal log, image check | Not a pass. 42 left, each with a real child. Continue at 8.11b | ☐ |
| 8.11b | Unused children under those 42 removed, then the 42 | Removal log, image check | `removed 90 failed 0`, `left 0`; 523 images, `bad 0` | ☐ |
| 8.12 | Match check before any pull | Image check, orphan count, folderless count | Not a pass. `bad 0`, but `orphan_child_keys 2`. The 42 is a bad parse. Continue at 8.12b | ☐ |
| 8.12b | Two leftover init child names cleared | Program output, image check | `deleted 2 remaining 0`; 523 images, `bad 0`; `orphan_child_keys 0` | ☐ |
| 9.1 | Bases pulled | Pull log | Not a pass. `alpine:3.23.4` hit `AlreadyExists`. Continue at 9.1a | ☐ |
| 9.1a | Hidden alpine snapshot tree removed, then those two pulls retried | Removal log, pull log | `left 0`; both `alpine:3.23.4` names printed `Status:` | ☐ |
| 9.2 | Bases tested | Start test | 16 lines, every line `ok` | ☐ |
| 10.1 | DeepSWE loaded | `Loaded image` count | 113 loaded in 8m45s | ☐ |
| 10.2 | DeepSWE tested | Start test | Not a pass. 113 `layers_missing`. Do not mark. Continue at 10.2a | ☐ |
| 10.2a | Stale DeepSWE layer records removed | Removal log | `snapshot_ok 1226`, `snapshot_fail 0`, `images 113` | ☐ |
| 10.2b | DeepSWE loaded again | `Loaded image` count | 113 loaded in 13m40s | ☐ |
| 10.2c | DeepSWE tested again | Start test | `113 ok` | ☐ |
| 11.1 | Other images loaded | `Loaded image` count | 178 loaded in 29m17s, equal to `save_other.txt` | ☐ |
| 11.2 | Other images tested | Start test | Not a pass. `177 ok`, `1 other`. Do not mark. Continue at 11.2a | ☐ |
| 11.2a | Untagged expressvpn image started by id | One start | Not a pass. Missing snapshot `159017`. Continue at 11.2b | ☐ |
| 11.2b | Three stale expressvpn layer records removed, then the id started | Removal log, one start | `READY 5`, `snapshot_ok 5`, `snapshot_fail 0`, `rc 0` | ☐ |
| 12.1 | Every image tested | Start test | `932 ok` in 4m21s | ☐ |
| 12.2 | Store after | `store_check.sh` | Not a pass. `RESULT WARN`. Databases match, canaries `ok`. Do not mark | ☐ |
| 12.3 | Before and after | Table, not-running list | Recorded. 12 containers that were running are not running now | ☐ |
| 13.1 | Daily store check | Timer, `last.txt` | Timer enabled, next run 21 Oct 07:30; same three warnings as 12.2 | ☐ |
| 13.2 | Mount guard | Test run | Refused because `docker.socket` is active, `exit 1` | ☐ |
| 13.3 | Every guard present | Listing | Disk, fstab, four mount lines, immutable empty folder, timer, three scripts | ☐ |
| 14.1 | Socket and canary restored | `stat`, `runuser`, `crontab` | `root:docker 660`; `toby` sees `29.6.2`; canary line back | ☐ |
| 14.2 | mac-k3d env checks set aside | `ls` | Old path. `no env-check folder`. The 20 Oct rename is step 14.2b | ☐ |
| 14.2b | beacon env checks set aside | `ls` | `no env-check folder` | ☐ |
| 14.2c | Unused folderless snapshot records removed | Removal log | `snapshot_ok 53`, `snapshot_fail 0`, `images 932` | ☐ |
| 14.3 | Team told | Message | Local steps are done. Send when Toby is ready | ☐ |
| 15.1 | jiuwenswarm rebuilt | Owner's log | Skipped for now. Owner's later build, after the remaining local steps | ☐ |
| 15.2 | jiuwenswarm tested | Start test, store check | Skipped for now. After 15.1 | ☐ |
| 16.1 | Checklist and service logs | Files | `journal_window.txt` 321,121 lines; `checklist.txt` 62 lines | ☐ |
| 16.2 | Recording stopped, folder sealed | Checksum | `2026-10-16.tar.gz` sha256 `b629f4e1…6373`, 116 MB | ☐ |
| 16.3 | Copy on your PC | `sha256sum` | Same hash as 16.2, copied to `~/cloud-worker-evidence/repair/` | ☐ |
| 16.4 | Report sent | Message | Local steps are done. Send when Toby is ready | ☐ |
| 17 | Jenkins node online | Sign-off | Root and the mentor OK; node online | ☐ |
| 18 | Watched reboot, own window | Boot order, store check | `/data` mounted before containerd; `RESULT OK`, or `WARN` only for disk space | ☐ |
| 19.1 | Pull the official bases still missing | Pull log | Temurin 3 of 3, maven 3 of 3, python 7 of 7. `python:3.11` pulled at 09:22 after the leftover snapshot was removed | ☐ |
| 19.2 | Pull buildkit and mambaforge | Pull log | Both `Status:` at 09:13. `buildkit:v0.24.0`, `mambaforge:24.7.1-2` | ☐ |
| 19.3 | Pull the 8 lolbench tags the investigation named | Pull log | 4 `lolbench-linux` tags done at 09:27. 4 `lolbench` tags are `linux/arm64` only | ☐ |
| 19.4 | Pull the other published lolbench tags | Pull log | `arrow_2` and `cpython_5` done at 09:38. Four `lolbench` jdk tags are `linux/arm64` only | ☐ |
| 19.5 | Pull 40 `jefzda/sweap-images` tags | Pull log | About 2 to 4 h | ☐ |
| 19.6 | Pull 42 published SWE-bench eval images | Pull log | About 4 to 8 h. Start only with 200 GB free | ☐ |
| 19.7 | Later one-off images | Pull log | Only if root asks. `curl`, `verus`, `dafny`, two `xinzhou1997` tags, four terminal-bench digests | ☐ |

## Step 0: before the window (your PC, the day before)

### 0.1 Get the corrected method approved

- **Where:** terminal 2, a written message to root and the mentor.
- **Why:** the approved remove-then-pull cannot repair `ubuntu:24.04`, jiuwenswarm or DeepSWE on this store ("Read this first"). The method here removes other users' failing images and containers and the build cache, so it needs a written OK.
- **Risk:** none.

```text
Before the repair window: a correction to the approved method (CLOUD_WORKER_REPAIR_STEPS.md, "Read this first").

containerd does not download a file whose record already exists, and does not unpack a layer whose
record already exists, even when the file or folder is gone. Its cleanup keeps a record while any image,
container or build-cache entry still names it. So "docker rmi X; docker pull X" leaves X broken when
another image shares its broken records. From the 15 Oct inventory:
- ubuntu:24.04: 1 of its 7 missing files is also named by 113 *-pier-egress-proxy task images.
- jiuwenswarm: 1,249 of its 4,903 missing files are also named by 625 other images, mostly chrys-bench.
- published DeepSWE: its layer records are the parents of 15,572 others (7,874 layers of 4,700 other
  images, 6,807 build-cache entries, 891 container layers).

Proposed instead, in the same window:
1. Save the 253 images that still have every downloaded file (113 DeepSWE, 140 local; about 28 GB of tar files).
2. Remove every container that cannot mount its layers, every image that fails the start test (about 7,500
   names, all failing already), and the build cache. Remove the download records whose file is gone.
3. Let containerd's cleanup clear what nothing names. Check: no record without its file.
4. Pull the 12 bases, load the 253 saved images back; the jiuwenswarm owner rebuilds.
Step 1 is done through a second view of the system disk, so /data is not unmounted.
Steps 1 to 14 take about 4.5 to 8 hours; the cleanup in step 8 is most of the spread.

Please confirm in writing:
(a) this method;
(b) removing the failing images, the containers that cannot mount, and the build cache, for all users;
(c) whether everyone else waits until the jiuwenswarm rebuild passes, or Docker opens to all after step 14;
(d) the date and time of the window, and who watches as root.
```

- **Expected:** a written OK from root and from the mentor, with answers to (a) to (d).
- **Pass check:** both OKs are in writing. You record them in step 1.4.
- **Achieves:** the method, and its effect on other users, are agreed before anything changes.
- **Result:** not run yet.

### 0.2 Announce the window

- **Where:** terminal 2, the team channel. Attach `~/cloud-worker-evidence/inventory/containers_on_broken.tsv` from your PC.
- **Why:** stopping Docker stops every user's containers, and step 8 removes images and containers that other users created. Everyone needs to know what goes, what stays, and until when they can object.
- **Risk:** none.

```text
Cloud worker maintenance: <date>, <start> to <end> (CST). Please stop your work on the worker before <start>.

- Docker is stopped at <start>, so every container stops. Containers with a restart policy start again by themselves.
- From <start> until the all-clear, Docker is closed to the docker group. Please pause cron jobs and timers that use Docker.
- Removed during the window, because they cannot work now:
  - every container that cannot mount its layers, including the 158 created from broken images (list attached)
    and the two created on 12 Oct at 11:33 and 11:41;
  - every image that fails a start test (about 7,500 names);
  - Docker's build cache.
- Kept: every image that starts, every container that can mount its layers, all volumes, everything else on /data.
- Saved to tar files and loaded back: the 113 published DeepSWE images, and 140 local images that still have all their files.
- An image that fails now has to be pulled or rebuilt after the window: its downloaded files are gone.
- A folder named hanxu, written under /data on the system disk on 12 Oct at 10:38, is moved into a root-only archive. Ask root if you need it.

Objections or questions before <deadline>, please.
```

- **Expected:** the message is posted, and the deadline passes.
- **Pass check:** no objection is open. Root answered any that came.
- **Result:** not run yet.

### 0.3 Check the key and the server, read-only

- **Where:** terminal 2, your PC.
- **Why:** find a blocker the day before, not in the window: a missing tool, too little space, Docker listening on a network port, or `live_restore` changed.
- **Risk:** none. It logs in as root, runs commands that only print, and logs out.

```bash
ssh-keygen -lf ~/.ssh/cloud-worker-root.pem
ssh -i ~/.ssh/cloud-worker-root.pem -o IdentitiesOnly=yes root@47.84.16.18 '
hostname; date
for c in tmux script fuser chattr lsattr ctr docker python3 runuser; do printf "%-8s %s\n" "$c" "$(command -v "$c" || echo MISSING)"; done
df -h / /data
findmnt -no TARGET,SOURCE,FSTYPE,UUID /data
systemctl show docker.socket -p Listen --value
ss -ltnp | grep -E "dockerd|containerd" || echo "no TCP listener"
docker info --format "live_restore={{.LiveRestoreEnabled}} containers={{.Containers}} running={{.ContainersRunning}}"
systemctl list-dependencies --reverse --plain --no-pager containerd.service
'
```

- **Expected:**
  - The same fingerprint as in investigation step 0.2.
  - `iZt4ndd2dff7gqjta7mppaZ`, the date, and a path for each tool; no `MISSING`.
  - `/` with at least 10 GB available, and `/data` with at least 200 GB.
  - `/data /dev/nvme1n1p1 ext4 25454905-…`.
  - `/run/docker.sock (Stream)`, `no TCP listener`, `live_restore=false`.
  - The units that need containerd: `docker.service`, and any other unit that would stop with it in step 3.1.
- **Pass check:** all of the above. If `tmux` is `MISSING`, stop: root decides (use `screen`, or install `tmux`).
- **Send me:** the whole output.
- **Result:** run 16 Oct 16:30 CST. **Pass.**
  - Fingerprint `2048 SHA256:QHhVAmPVaTRus4W2bIv1v5ekDbwRct4041vLrXOJfuc (RSA)`, the same as investigation step 0.2.
  - All nine tools found.
  - `/` 166 GB available (66% used); `/data` 335 GB available (98% used). Enough for the about 52 GB of tars in step 7, but every full store check will show the disk WARN until the cleanup frees space.
  - `/data /dev/nvme1n1p1 ext4 25454905-c932-4246-baa7-e50351cb6798`.
  - `/run/docker.sock (Stream)`, `no TCP listener`, `live_restore=false`.
  - 510 containers, 44 running (43 on 14 Oct). All 44 stop in step 3.1, so their owners must be in the 0.2 announcement.
  - Only `docker.service` needs containerd (plus the system targets), so step 3.1 stops nothing else.

## Step 1: start the recorded root session (window start)

### 1.1 Take the Jenkins node offline

- **Where:** terminal 2, Jenkins in your browser.
- **Why:** no Jenkins job may start on the worker during the repair.
- **Risk:** low. It only stops new jobs; "Bring this node back online" undoes it (step 17).

In Jenkins: **Manage Jenkins → Nodes → `linux-iZt4ndd2dff7gqjta7mppaZ` → Mark this node temporarily offline**, reason `image store repair <date>`.

- **Expected:** the node shows as offline, and no build is running on it.
- **Pass check:** offline and idle. If a build is running, wait for it, or ask the mentor.
- **Result:** the node page at 16:57:45 CST shows **Bring this node back online**, so it is temporarily offline. The connection line is `Disconnected by admin`. That time is 35 seconds after step 3.1 finished. The description on the page is still `mac-k3d worker agent`. That wording is from before the 20 Oct codebase migration to beacon. Set the offline reason to `image store repair 2026-10-16` if it is not already. When the node returns, the description should say beacon.

### 1.2 Log in as root, inside `tmux`

- **Where:** terminal 1.
- **Why:** the repair needs root. `tmux` keeps the session running on the worker when the connection drops, so a long step isn't cut off halfway.
- **Risk:** **high.** A root shell has no limits. Type only this guide's blocks.

```bash
ssh -i ~/.ssh/cloud-worker-root.pem -o IdentitiesOnly=yes -o ServerAliveInterval=30 root@47.84.16.18
```

Then, on the worker:

```bash
tmux new -s repair
```

- **Expected:** the prompt `root@iZt4ndd2dff7gqjta7mppaZ:~#`, then a green bar at the bottom with `[repair]`.
- **If the connection drops later:** run the `ssh` line again, then `tmux attach -t repair`. The step that was running kept running.
- **Result:** run 16 Oct 16:35 CST. **Pass.** Prompt `(base) root@iZt4ndd2dff7gqjta7mppaZ:~#`, status bar `[repair] 0:bash*`. The `(base)` is conda, started by root's bashrc. Nothing is recorded yet: `script` starts in 1.3.

### 1.3 Create the log folder, and start recording

- **Why:** `script` records everything on screen from now on, into `$R/sessions/`. The file `/root/cloud-worker-repair.env` remembers `$R` and the start time, for a reconnect and for step 16.
- **Risk:** none. It creates one root-only folder in `/var/log` and one file in `/root`.

```bash
export R=/var/log/cloud-worker-repair/$(date +%F)
install -d -m 700 /var/log/cloud-worker-repair "$R" "$R/out" "$R/sessions" "$R/bin"
printf 'export R=%s\nexport REPAIR_START="%s"\n' "$R" "$(date '+%F %T')" > /root/cloud-worker-repair.env
script -q -f -a "$R/sessions/$(date +%H%M%S)-main.log"
```

- **Expected:** no output, and a new prompt.
- **Result:** run 16 Oct 16:37:17 CST. **Pass.** Checked from a separate read-only login, so this check is not in the recording. `$R` is `/var/log/cloud-worker-repair/2026-10-16`. The four folders are mode 700, root. `script` (pid 1994884) is writing `sessions/163717-main.log` inside the `repair` tmux session. The env file has `REPAIR_START="2026-10-16 16:37:17"`.

### 1.4 Load the helpers, and record the approvals

- **Why:**
  - The command history goes into `$R/commands.history`, with the time of each command.
  - The prompt shows the time and `[repair`, so you can see that the recording is on.
  - `mark` writes one line into `$R/checklist.tsv`: time, step, PASS, FAIL, APPROVED or INFO, and a note.
- **Risk:** none.

```bash
cat >> /root/cloud-worker-repair.env <<'EOF'
export HISTFILE=$R/commands.history HISTTIMEFORMAT='%F %T ' HISTSIZE=100000 HISTFILESIZE=100000
shopt -s histappend
PROMPT_COMMAND='history -a'
PS1='[repair \t] \u@\h:\w\$ '
mark() { printf '%s\t%s\t%s\t%s\n' "$(date '+%F %T %Z')" "$1" "$2" "${*:3}" | tee -a "$R/checklist.tsv"; }
EOF
source /root/cloud-worker-repair.env
cat /root/cloud-worker-repair.env
```

Then record step 0, with your own times in the notes:

```bash
mark 0.1 APPROVED "root <time>, mentor <time>; jiuwenswarm: <their answer to c>"
mark 0.2 PASS "announced <time>, deadline <time>, no open objection"
mark 0.3 PASS "pre-flight on <date>"
mark 1.1 PASS "Jenkins node offline"
```

- **Expected:** the prompt changes to `[repair 09:12:30] root@iZt4ndd2dff7gqjta7mppaZ:~#`. `cat` shows the file, starting with `export R=/var/log/cloud-worker-repair/<date>`. Each `mark` prints its line. The `(base)` from 1.2 goes away, because this block replaces the prompt.
- **Pass check:** the prompt starts with `[repair`. If it still starts with `(base)`, stop and send me that line before the `mark` commands. `mark 1.4 PASS`
- **Result:** helpers loaded 16 Oct 16:39:31 CST. **Pass** for the prompt and the env file. `cat` printed both halves, and the prompt is `[repair 16:39:31] root@iZt4ndd2dff7gqjta7mppaZ:~#`, with no `(base)`. The `mark` lines are still to run.
- **If `tmux` itself is gone after a drop** (`tmux attach` says no session): `tmux new -s repair`, then `source /root/cloud-worker-repair.env && script -q -f -a "$R/sessions/$(date +%H%M%S)-main.log"`, then `source /root/cloud-worker-repair.env`. Then `tail -n 3 "$R/checklist.tsv"`, and send me that before you go on.
- **Result:** not run yet.

### 1.5 Save the three repair scripts

- **Why:**
  - `store_check.sh` compares containerd's records with the files and folders on disk, checks which database containerd has open, and starts a few canary images. Step 13 installs the same script as the daily check.
  - `start_test.sh` is the investigation's start test, with two changes: 8 images at a time, and a test that hits its 120-second limit is reported as `timeout`, not `ok`.
  - `container_test.sh` checks that each stopped container can still mount its layers. It copies one small file out of the container, to nowhere. It starts nothing.
  
  None of them removes anything.
- **Risk:** none. It writes three files into `$R/bin`, and `bash -n` checks them without running them.

```bash
cat > "$R/bin/store_check.sh" <<'EOF'
#!/bin/bash
# Read-only check of the containerd store behind Docker. Changes nothing.
# Usage: store_check.sh [--quick] [canary image ...]   --quick: only the mount and the open databases.
# The last line is RESULT OK, or RESULT WARN after one WARN line per problem.
C=/data/containerd
DB=$C/io.containerd.metadata.v1.bolt/meta.db
SDB=$C/io.containerd.snapshotter.v1.overlayfs/metadata.db
BLOBS=$C/io.containerd.content.v1.content/blobs/sha256
SNAPS=$C/io.containerd.snapshotter.v1.overlayfs/snapshots
quick=0; [ "${1:-}" = --quick ] && { quick=1; shift; }
T=$(mktemp -d) || exit 2
trap 'rm -rf "$T"' EXIT
warn=0
line() { printf '%-24s %s\n' "$1" "$2"; }
bad() { line WARN "$*"; warn=1; }

line time "$(date '+%F %T %Z')"
line data_mount "$(findmnt -no SOURCE,FSTYPE,UUID /data || echo none)"
mountpoint -q /data || bad "/data is not a mount point"
pid=$(systemctl show -p MainPID --value containerd)
line containerd_pid "$pid"
if [ "${pid:-0}" -gt 0 ]; then
  for f in "$DB" "$SDB"; do
    open=none
    for fd in /proc/"$pid"/fd/*; do
      [ "$(readlink "$fd")" = "$f" ] && { open=$(stat -L -c '%d:%i' "$fd"); break; }
    done
    disk=$(stat -c '%d:%i' "$f" 2>/dev/null || echo none)
    line "open_${f##*/}" "open=$open on_disk=$disk"
    if [ "$open" != none ] && [ "$open" != "$disk" ]; then bad "containerd has a different $f open than the one on /data"; fi
    if [ "$f" = "$DB" ] && [ "$open" = none ]; then bad "containerd does not have $DB open"; fi
  done
else
  bad "containerd is not running"
fi
if [ "$quick" = 0 ]; then
  ctr namespaces ls -q > "$T/ns"
  line namespaces "$(tr '\n' ' ' < "$T/ns")"
  while read -r ns; do ctr -n "$ns" content ls -q; done < "$T/ns" | sort -u > "$T/records"
  ls -U "$BLOBS" | sed 's/^/sha256:/' | sort > "$T/files"
  line content_records "$(wc -l < "$T/records")"
  line blob_files "$(wc -l < "$T/files")"
  n=$(comm -23 "$T/records" "$T/files" | wc -l)
  line records_without_blob "$n"
  [ "$n" -eq 0 ] || bad "$n download records have no blob file"
  line blobs_without_record "$(comm -13 "$T/records" "$T/files" | wc -l)"
  while read -r ns; do ctr -n "$ns" snapshots ls | tail -n +2; done < "$T/ns" > "$T/snaps"
  s=$(wc -l < "$T/snaps"); d=$(ls -U "$SNAPS" | wc -l)
  line snapshot_records "$s"
  line snapshot_folders "$d"
  [ "$s" -le "$d" ] || bad "$((s - d)) more layer records than snapshot folders"
  line images "$(ctr -n moby images ls -q | wc -l)"
  line leases "$(ctr -n moby leases ls | tail -n +2 | wc -l)"
  for m in / /data; do
    u=$(df --output=pcent "$m" | tail -n 1 | tr -dc 0-9)
    line "disk_used_$m" "$u%"
    [ "$u" -lt 85 ] || bad "$m is $u% full"
  done
fi
for img in "$@"; do
  if timeout 120 docker run --rm --pull never --network none --entrypoint true "$img" > /dev/null 2> "$T/err" < /dev/null; then
    line "canary $img" ok
  else
    bad "canary $img: $(tr '\n' ' ' < "$T/err" | cut -c1-160)"
  fi
done
[ "$warn" = 0 ] && echo "RESULT OK" || echo "RESULT WARN"
exit "$warn"
EOF
cat > "$R/bin/start_test.sh" <<'EOF'
#!/bin/bash
# Start each image once: no network, no pull, entrypoint "true", 8 at a time. Never removes an image.
# Usage: start_test.sh <file with one containerd image name per line> <output .tsv>
# Output columns: result, image name, first 200 characters of Docker's message.
in=${1:?image list}
out=${2:?output file}
one() {
  name=$1
  ref=${name#moby-dangling@}
  msg=$(timeout 120 docker run --rm --pull never --network none --entrypoint true "$ref" 2>&1 < /dev/null)
  rc=$?
  if [ "$rc" = 0 ]; then s=ok
  elif [ "$rc" = 124 ]; then s=timeout
  else
    case $msg in
      *"failed to stat parent"*|*"lowerdir="*|*"overlayfs/snapshots/"*) s=layers_missing ;;
      *"blob not found"*) s=blob_missing ;;
      *"already exists"*|*"missing parent"*) s=stale_record ;;
      *"executable file not found"*) s=ok ;;   # the layers mounted; the image just has no "true"
      *) s=other ;;
    esac
  fi
  printf '%s\t%s\t%s\n' "$s" "$name" "$(printf '%s' "$msg" | tr '\n\t' '  ' | cut -c1-200)"
}
export -f one
grep -v '^$' "$in" | xargs -d '\n' -P 8 -n 1 bash -c 'one "$1"' _ > "$out"
EOF
cat > "$R/bin/container_test.sh" <<'EOF'
#!/bin/bash
# For each container that is not running, mount its layers for a moment by copying one small file out of it, to nowhere.
# Starts nothing, removes nothing. Usage: container_test.sh <output .tsv>
# Output columns: result, full ID, name, state, image, first 200 characters of Docker's message.
docker ps -a --no-trunc --format '{{.ID}}\t{{.Names}}\t{{.State}}\t{{.Image}}' |
while IFS=$'\t' read -r id name state image; do
  msg=
  case $state in
    running) s=running ;;
    dead) s=dead ;;
    *)
      msg=$(timeout 120 docker cp "$id:/etc/hostname" - 2>&1 > /dev/null < /dev/null)
      rc=$?
      if [ "$rc" = 0 ]; then s=ok
      elif [ "$rc" = 124 ]; then s=timeout
      else
        case $msg in
          *"Could not find the file"*) s=ok ;;   # the layers mounted; the file just isn't there
          *"failed to stat parent"*|*"lowerdir="*|*"overlayfs/snapshots/"*|*"does not exist"*|*"not found"*) s=layers_missing ;;
          *) s=other ;;
        esac
      fi ;;
  esac
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$s" "$id" "$name" "$state" "$image" "$(printf '%s' "$msg" | tr '\n\t' '  ' | cut -c1-200)"
done > "${1:?output file}"
EOF
bash -n "$R/bin/store_check.sh" && bash -n "$R/bin/start_test.sh" && bash -n "$R/bin/container_test.sh" && echo "scripts OK"
ls -l "$R/bin"
```

- **Expected:** `scripts OK`, then the three files.
- **Pass check:** `scripts OK`. `mark 1.5 PASS`
- **Result:** run 16 Oct 16:42:54 CST. **Pass**, marked at 16:48:47. `scripts OK`, then `store_check.sh` 2963 bytes, `start_test.sh` 1089 bytes, `container_test.sh` 1154 bytes. Those sizes are the guide's scripts.

### 1.6 Open a second window to watch long steps (optional)

- **Why:** steps 6, 7 and 8 run for a long time in window 0. Window 1 lets you watch their progress without touching them. It is recorded too.
- **Risk:** none.

Press `Ctrl-b c`, then paste:

```bash
source /root/cloud-worker-repair.env && script -q -f -a "$R/sessions/$(date +%H%M%S)-watch.log"
```

Then:

```bash
source /root/cloud-worker-repair.env
```

- **Expected:** a `[repair …]` prompt in window 1. `Ctrl-b 0` goes back to window 0. Type the steps in window 0 only. In window 1, type only the watch commands that a step gives you.
- **Pass check:** `mark 1.6 PASS` (in window 0).
- **Result:** not run yet.

## Step 2: record the state before any change (Docker still running)

### 2.1 Is anyone still working?

- **Why:** stopping Docker in step 3 kills whatever still runs. Check that only announced work is left.
- **Risk:** none.

```bash
{
date
who
pgrep -a -f 'docker (build|buildx|pull|push|run|load|save|compose)' || echo "no docker client running"
docker ps --format '{{.ID}}\t{{.Names}}\t{{.Status}}\t{{.Image}}'
} 2>&1 | tee "$R/out/2.1_who.txt"
```

- **Expected:** your own login in `who` (other logins may be idle; ask about them), `no docker client running`, then the running containers: services, and work whose owners were told in 0.2. On 14 Oct, 43 were running.
- **Pass check:** no Docker client is running, and nobody has asked to wait. `mark 2.1 PASS`
- **Result:** run 16 Oct 16:43:16 CST. **Fail**, marked at 16:48:47. At 16:49 Toby decided the owner has been told and the stop in step 3 may go ahead. The client was still running then (pid 1076068). Steps 2.3 and 2.4 still have to be recorded first, because they need Docker running.
  - `who` printed nothing. The root shell is the `repair` session, and this login is not recorded in utmp.
  - A Docker client is running: user `naiyu`, pid 1076068, started 14:17:57, `docker run --name marathon-ds-swarmflow-native-4bada14ca9064ce9` of `swe-marathon-cpu/pact-jiuwen:20261013-pr1`. Its deadline is 2026-10-17 00:17:58 CST. No container of that name is in `docker ps -a`.
  - `docker ps` lists 12 running containers. `docker info` still says Running 44, Paused 9, Stopped 457. The per-container states are 12 running, 0 paused, 279 created, 198 exited, 21 dead (510). The 12 lines are the full list, not a cut-off screen.

### 2.2 The mount, the guards and the services

- **Why:** record the state that steps 4, 5 and 13 change or rely on: the UUID line in fstab, the two settings that stop containerd and Docker from starting without `/data`, and `live_restore`.
- **Risk:** none.

```bash
{
findmnt -no TARGET,SOURCE,FSTYPE,UUID /data
grep -n ' /data ' /etc/fstab
findmnt --verify 2>&1 | tail -n 1
systemctl cat containerd docker | grep -E '^(RequiresMountsFor|AssertPathIsMountPoint)'
systemctl is-active docker.socket docker containerd
docker info --format 'live_restore={{.LiveRestoreEnabled}} containers={{.Containers}} running={{.ContainersRunning}} version={{.ServerVersion}}'
stat -c '%U:%G %a %n' /run/docker.sock
df -h / /data
} 2>&1 | tee "$R/out/2.2_mount_guards.txt"
ls -A /data > "$R/out/2.2_data_top.txt"; wc -l < "$R/out/2.2_data_top.txt"
```

- **Expected:**
  - `/data /dev/nvme1n1p1 ext4 25454905-…`, and the fstab line `UUID=25454905-… /data ext4 defaults 0 0`.
  - `Success, no errors or warnings detected` (this server's `findmnt --verify` prints that instead of `0 parse errors, 0 errors, 0 warnings`). As root, the three permission warnings of investigation step 9.8 should be gone.
  - Four lines: `RequiresMountsFor=/data` and `AssertPathIsMountPoint=/data`, once for each service.
  - `active` three times, `live_restore=false`, version `29.6.2`, `root:docker 660 /run/docker.sock`, then the free space.
  - The number of entries at the top of `/data`, kept for step 4.4.
- **Pass check:** the mount, the fstab line, the four guard lines and `live_restore=false`. `mark 2.2 PASS`
- **Send me:** the output.
- **Result:** run 16 Oct 16:43:26 CST. **Pass**, marked at 16:48:47. `/data` is `/dev/nvme1n1p1` ext4 `25454905-c932-4246-baa7-e50351cb6798`, fstab line 12 is the same UUID, `findmnt --verify` succeeded, four guard lines, `active` three times, `live_restore=false`, Docker 29.6.2, `root:docker 660 /run/docker.sock`, `/` 166 GB free, `/data` 335 GB free. Top of `/data` has 27 entries, saved for step 4.4.

### 2.3 The store, before

- **Why:** the numbers to compare with after the repair, and the first check that containerd has the 14 TB database open.
- **Risk:** none. It lists records and files, and starts two canary images with `--rm`. It takes 1 to 3 minutes.

```bash
bash "$R/bin/store_check.sh" docker.io/library/alpine:3.21 docker.io/library/busybox:1.36 2>&1 | tee "$R/out/2.3_store_check_before.txt"
```

- **Expected:** close to the 15 Oct inventory, plus what was added since:
  - `open_meta.db open=X on_disk=X`, the same pair twice;
  - `namespaces moby`, and perhaps one or two others;
  - `content_records` about 46,000, `blob_files` about 3,400, `records_without_blob` about 42,900, `blobs_without_record` about 97;
  - `snapshot_records` about 19,600 and `snapshot_folders` about 10,200, with a WARN;
  - `images` about 7,750, the disk lines (a WARN for `/data`), both canaries `ok`, and `RESULT WARN`.
- **Pass check:** `open_meta.db` has the same pair twice. The WARN lines are the damage, expected here. `mark 2.3 PASS`
- **Send me:** the output.
- **Result:** run 16 Oct 16:52:21–16:52:28 CST. **Pass.** `open_meta.db` and `open_metadata.db` each have the same pair twice. Both canaries `ok`. `RESULT WARN` from the three expected lines: 77,732 records without a blob, 10,774 more layer records than folders, `/data` 98% full.
  - Counts are higher than the 15 Oct moby estimate because this check adds every namespace: 4 `capbench-recovery-*`, 16 `lolbench-recovery-*`, `moby`, and `moby_history`. Records 82,010, blob files 4,278, images 8,057, leases 20,944 (moby only). `blobs_without_record` is 0.
  - The `^[[B` characters are the Down arrow key. They are not part of the result.
  - Step 8 only removes names in `moby`. The same check at step 8.5 still counts the recovery namespaces, so a leftover there can keep `records_without_blob` above 0. That is a gate to look at then, not a reason to stop now.

### 2.4 Containers, restart policies and image names

- **Why:** the lists before anything is stopped. Step 12.3 uses them to tell owners which containers were running and are not running afterwards.
- **Risk:** none.

```bash
cd "$R/out"
docker ps -a --no-trunc --format '{{.ID}}\t{{.Names}}\t{{.State}}\t{{.Image}}\t{{.CreatedAt}}' > 2.4_containers_before.tsv
docker ps -q --no-trunc > 2.4_running_before.txt
docker ps -aq --no-trunc | xargs -r docker inspect --format '{{.Id}}{{"\t"}}{{.Name}}{{"\t"}}{{.HostConfig.RestartPolicy.Name}}{{"\t"}}{{range .Mounts}}{{.Source}} {{end}}' > 2.4_inspect_before.tsv
ctr -n moby images ls -q > 2.4_images_before.txt
wc -l 2.4_containers_before.tsv 2.4_running_before.txt 2.4_images_before.txt
cut -f3 2.4_inspect_before.tsv | sort | uniq -c
grep -c 'docker.sock' 2.4_inspect_before.tsv
```

- **Expected:** about 500 containers, the running ones from 2.1, and about 7,750 image names. Then the count per restart policy (`no`, `always`, `unless-stopped`, `on-failure`), and how many containers mount a Docker socket.
- **Pass check:** the three files are written. `mark 2.4 PASS`
- **Result:** run 16 Oct 16:54:15 CST. **Pass**, marked at 16:55:33. 510 containers, 12 running, 8,057 image names. Inspect has all 510. Restart policy: 506 `no`, 4 `unless-stopped` (`jiuwen-tb4-daemon-20261006`, `chrys-bench-capbench-daemon-20260927-v1`, `buildx_buildkit_cgb0`, `expressvpn-proxy`). Those four start again in step 5.2. One container mounts a Docker socket: `chrys-bench-capbench-daemon-20260927-v1`. No `always` or `on-failure`.

### 2.5 Pause the hourly canary

- **Why:** `toby`'s hourly canary starts the four working images. While Docker is closed it would fail and write to `canary_fail.log`, which would look like damage.
- **Risk:** low. It saves `toby`'s crontab, which has only the canary line, then removes it. Step 14.1 puts it back from the saved copy.

```bash
crontab -u toby -l > "$R/out/2.5_toby_crontab_before.txt"
cat "$R/out/2.5_toby_crontab_before.txt"
crontab -u toby -r
crontab -u toby -l 2>&1 | head -n 1
```

- **Expected:** the canary line, then `no crontab for toby`.
- **Pass check:** `no crontab for toby`, and the saved file has the line. `mark 2.5 PASS`
- **Result:** run 16 Oct 16:55:39 CST. **Pass**, marked at 16:55:48. The saved file is the one canary line (`17 * * * *`, alpine 3.21, alpine 3.22, busybox 1.36, debian bookworm). Then `no crontab for toby`. Step 14.1 puts this line back.

## Step 3: stop Docker and containerd, and copy the two databases

### 3.1 Stop

- **Why:** while containerd runs, its databases change and its cleanup can run. The databases are copied in 3.3, and the store is changed in step 4. Both need containerd stopped. `docker.socket` is stopped first, so nothing starts Docker again by connecting to it.
- **Risk:** **high.** Every container on the main Docker stops, because `live_restore` is false. Jobs inside them are killed after Docker's 10-second grace time. Announced in 0.2. The TaskAnchor runtime has its own store and is not stopped, unless step 0.3 listed it as needing containerd.

```bash
date; systemctl stop docker.socket docker; systemctl stop containerd; date
systemctl is-active docker.socket docker containerd
systemctl show -p MainPID --value containerd
```

- **Expected:** one to two minutes between the two dates; `inactive` three times; `0`.
- **Pass check:** `inactive` three times, and `0`. `mark 3.1 PASS`
- **Result:** run 16 Oct 16:56:58–16:57:10 CST. **Pass.** Twelve seconds between the two dates. `inactive` three times, then pid `0`. The `mark` line is still to run.

### 3.2 Check that nothing has the databases open

- **Why:** a copy of a database that a program still writes is not a true copy.
- **Risk:** none.

```bash
fuser -v /data/containerd/io.containerd.metadata.v1.bolt/meta.db /data/containerd/io.containerd.snapshotter.v1.overlayfs/metadata.db 2>&1 || echo "no process has either database open"
```

- **Expected:** `no process has either database open`.
- **Pass check:** that line, and no process listed. `mark 3.2 PASS`
- **Result:** run 16 Oct 16:58:51 CST. **Pass.** `no process has either database open`. The `mark` line is still to run.

### 3.3 Copy the databases

- **Why:** a record of the two databases as they were before the repair, for root and the mentor to review. It is not a way back: step 8 deletes the files they name.
- **Risk:** low. It writes about 720 MB into a root-only folder on the system disk. The copies stay on the worker.

```bash
B=/root/cloud-worker-repair-backup/$(date +%F)
install -d -m 700 /root/cloud-worker-repair-backup "$B"
cp -p --sparse=always /data/containerd/io.containerd.metadata.v1.bolt/meta.db "$B/meta.db"
cp -p --sparse=always /data/containerd/io.containerd.snapshotter.v1.overlayfs/metadata.db "$B/overlayfs-metadata.db"
sha256sum /data/containerd/io.containerd.metadata.v1.bolt/meta.db "$B/meta.db" /data/containerd/io.containerd.snapshotter.v1.overlayfs/metadata.db "$B/overlayfs-metadata.db" | tee "$R/out/3.3_db_backup.txt"
ls -l "$B"
```

- **Expected:** four hash lines; the first two are the same hash, and so are the last two. Then the two copies, about 655 MB and 64 MB.
- **Pass check:** both pairs match. `mark 3.3 PASS`
- **Result:** run 16 Oct 16:59:21 CST. **Pass.** Both hash pairs match. `meta.db` is 654,802,944 bytes (about 655 MB), mode 644, copied with its 16:57 timestamp. `overlayfs-metadata.db` is 67,133,440 bytes (about 64 MB), mode 600, timestamp 16:52. The copies are in `/root/cloud-worker-repair-backup/2026-10-16/`. They stay on the worker. The `mark` line is still to run.

## Step 4: one store: archive the hidden store, and lock the empty folder

The system disk's `/data` folder still holds the old store under the mount (investigation step 6.4). This step empties that folder, and makes it immutable, so containerd can never find or create a second store there.

### 4.1 Open a writable view of the system disk

- **Why:** the hidden folder is only reachable through a second view of the system disk, as in investigation step 6.4. This time the view is writable, because 4.2 moves folders through it. A plain bind of `/` shows the system disk only, not the 14 TB disk mounted on `/data`.
- **Risk:** low. The view changes no file by itself. 4.4 removes it.

```bash
mkdir -p /mnt/rootfs
mount --bind / /mnt/rootfs
findmnt -no TARGET,SOURCE,OPTIONS /mnt/rootfs
ls -la --full-time /mnt/rootfs/data | tee "$R/out/4.1_hidden_before.txt"
du -sh /mnt/rootfs/data | tee -a "$R/out/4.1_hidden_before.txt"
```

- **Expected:** `/mnt/rootfs`, the system disk's device, `rw,…`. The listing shows exactly three folders: `containerd` (4 Aug), `docker` (12 Oct 10:39) and `hanxu` (12 Oct 10:38). About `1.7G`.
- **Pass check:** those three folders and nothing else, about 1.7 GB. If there is anything else, run `umount /mnt/rootfs` and stop. `mark 4.1 PASS`
- **Result:** run 16 Oct 17:00:35 CST. **Pass.** `/mnt/rootfs` is `/dev/nvme0n1p3`, `rw,relatime`. Exactly three folders: `containerd` (4 Aug 20:44), `docker` (12 Oct 10:39:46), `hanxu` (12 Oct 10:38:15). Size `1.7G`. The `mark` line is still to run.

### 4.2 Move the hidden store into a root-only archive

- **Why:** the old store must not be at the mount point any more. It is kept, not deleted, because the review may need it. Moving within one disk is a rename: nothing is copied.
- **Risk:** low. It renames three folders on the system disk. Until step 4.3, `mv` back undoes it. The archive is root-only and stays on the worker.

```bash
A=/mnt/rootfs/root/cloud-worker-repair-backup/hidden-data-under-mount
install -d -m 700 "$A"
find /mnt/rootfs/data -mindepth 1 -maxdepth 1 -exec mv -t "$A" {} +
ls -A /mnt/rootfs/data | wc -l
ls -la --full-time "$A" | tee "$R/out/4.2_archive.txt"
du -sh "$A" | tee -a "$R/out/4.2_archive.txt"
```

- **Expected:** `0`, then the archive with `containerd`, `docker` and `hanxu`, about `1.7G`.
- **Pass check:** `0` entries left in the hidden folder, and all three in the archive. `mark 4.2 PASS`
- **Result:** run 16 Oct 17:02:00 CST. **Pass.** `0` left under the hidden `/data`. The archive `/mnt/rootfs/root/cloud-worker-repair-backup/hidden-data-under-mount` holds `containerd`, `docker`, and `hanxu`, with the same timestamps as in 4.1, size `1.7G`. The archive directory is mode 700. The `mark` line is still to run.

### 4.3 Make the empty folder immutable

- **Why:** with the `i` flag set, the kernel refuses to create anything in the folder, even for root. If the 14 TB disk is ever not mounted, containerd cannot make a new store on the system disk, and nobody can put files there by mistake.
- **Risk:** low. It changes one flag on one empty folder of the system disk. `chattr -i /mnt/rootfs/data`, through the same view, undoes it. The flag does not affect the 14 TB disk, which is mounted over the folder.

```bash
chattr +i /mnt/rootfs/data
lsattr -d /mnt/rootfs/data
touch /mnt/rootfs/data/test 2>&1; mkdir /mnt/rootfs/data/test 2>&1
```

- **Expected:** a flag line with an `i`, such as `----i---------e------- /mnt/rootfs/data`. Then both `touch` and `mkdir` refused (`Permission denied` or `Operation not permitted`).
- **Pass check:** the `i` flag, and both refusals. `mark 4.3 PASS`
- **Result:** run 16 Oct 17:05:11 CST. **Pass.** Flag line `----i---------e------- /mnt/rootfs/data`. `touch` and `mkdir` both printed `Permission denied`. The `mark` line is still to run.

### 4.4 Remove the view, and check that `/data` is unchanged

- **Why:** the view is not needed any more. Confirm that steps 4.1 to 4.3 changed nothing on the 14 TB disk.
- **Risk:** none.

```bash
umount /mnt/rootfs
findmnt /mnt/rootfs || echo "view removed"
findmnt -no TARGET,SOURCE,FSTYPE /data
lsattr -d /data
diff <(ls -A /data) "$R/out/2.2_data_top.txt" && echo "same top-level entries on /data"
```

- **Expected:** `view removed`; `/data /dev/nvme1n1p1 ext4`; the flags of the 14 TB disk's top folder, with no `i`; `same top-level entries on /data`.
- **Pass check:** all four. `mark 4.4 PASS`
- **Achieves:** there is one store, `/data/containerd` on the 14 TB disk. Without that disk, `/data` is an empty folder nothing can write into, and containerd refuses to start (`AssertPathIsMountPoint=/data`).
- **Result:** run 16 Oct 17:06:31 CST. **Pass.** `view removed`. `/data /dev/nvme1n1p1 ext4`. Flags `--------------e------- /data`, no `i`. `same top-level entries on /data`. The `mark` line is still to run.

## Step 5: start containerd and Docker, with users kept out

### 5.1 Start containerd, and check which database it opened

- **Why:** the 12 Oct split brain was a containerd that had a different database open than the one at `/data`. `store_check.sh` compares the file containerd has open with the file at the path, by disk and inode.
- **Risk:** low. It starts containerd. No container starts until Docker starts.

```bash
systemctl start containerd
systemctl is-active containerd
bash "$R/bin/store_check.sh" --quick 2>&1 | tee "$R/out/5.1_open_db.txt"
```

- **Expected:** `active`, then `open_meta.db open=X on_disk=X`, the same pair, and `RESULT OK`. `open_metadata.db` may say `open=none` until the first layer is used; that is not a warning.
- **Pass check:** the same pair for `open_meta.db`, and `RESULT OK`. `mark 5.1 PASS`
- **If the pair differs:** that is the split-brain signal. Run `systemctl stop containerd` at once, and send me the output.
- **Result:** run 16 Oct 17:07:37 CST. **Pass.** containerd `active`, pid 2055239. `open_meta.db` is `66309:37131376` both open and on disk, the same pair as step 2.3. `open_metadata.db` is `open=none`, which this step allows. `RESULT OK`. The `mark` line is still to run.

### 5.2 Start Docker, closed to the `docker` group

- **Why:** from now until step 14, only root may use Docker. A job or a cron line of another user can't pull, build or remove while records are being removed. The socket belongs to group `docker`. Giving it to group `root` before Docker starts closes it to that group.
- **Risk:** low. The group change lasts until `docker.socket` restarts. Step 14.1 gives it back with `chgrp docker /run/docker.sock`. Containers with a restart policy start again now.

```bash
systemctl start docker.socket
chgrp root /run/docker.sock
stat -c '%U:%G %a %n' /run/docker.sock
systemctl start docker
systemctl is-active docker.socket docker
runuser -u toby -- docker version --format '{{.Server.Version}}' 2>&1 | tail -n 1
docker version --format '{{.Server.Version}}'
```

- **Expected:** `root:root 660 /run/docker.sock`; `active` twice; for `toby`, a line with `permission denied`; for root, `29.6.2`.
- **Pass check:** `toby` is refused, and root gets the version. `mark 5.2 PASS`
- **Result:** run 16 Oct 17:09:52 CST. **Pass.** `root:root 660 /run/docker.sock`, `active` twice. `toby` got `permission denied while trying to connect to the docker API at unix:///var/run/docker.sock`. Root got `29.6.2`. The `mark` line is still to run.

### 5.3 What started by itself

- **Why:** containers with a restart policy are running again. They are kept. Record them, and check that nothing has started a pull or a build.
- **Risk:** none.

```bash
docker ps --format '{{.ID}}\t{{.Names}}\t{{.Status}}\t{{.Image}}' | tee "$R/out/5.3_running_after_start.txt"
wc -l < "$R/out/5.3_running_after_start.txt"
pgrep -a -f 'docker (build|buildx|pull|push|run|load|save|compose)' || echo "no docker client running"
ctr -n moby content active
```

- **Expected:** the restarted containers, their count, `no docker client running`, and only the header line of `content active` (no download running).
- **Pass check:** no Docker client, and no download. `mark 5.3 PASS`
- **Result:** run 16 Oct 17:11:15 CST. **Pass**, marked at 17:11:39. `docker ps` listed 0 containers. `no docker client running`. `ctr content active` printed only its header, `REF SIZE AGE`.

## Step 6: test every image and container, and sort them

### 6.1 Start test of every image (20 to 40 minutes)

- **Why:** the 15 Oct start test is a day old, and it skipped the 903 leftover build tags. This run decides, from today's store, which images are removed in step 8.
- **Risk:** low. Each test container runs `true` and is removed at once (`--rm`). Like any `docker run`, it may unpack an image whose files are all present. That adds folders and changes nothing that exists.

```bash
cd "$R/out"
ctr -n moby images ls -q > 6.1_images.txt
wc -l < 6.1_images.txt
time nice bash "$R/bin/start_test.sh" 6.1_images.txt 6.1_start_test.tsv
wc -l < 6.1_start_test.tsv
cut -f1 6.1_start_test.tsv | sort | uniq -c
```

To watch, in window 1: `watch -n 30 wc -l "$R/out/6.1_start_test.tsv"`, then `Ctrl-c` there when it is done.

- **Expected:** about 7,750 names or more; 20 to 40 minutes; the same number of result lines. On 15 Oct the 6,847 named images gave: `ok` 254, `blob_missing` 6,313, `layers_missing` 253, `stale_record` 27. The build tags and images added since come on top. `other` and `timeout` should be few.
- **Pass check:** as many result lines as names; fewer than 20 `other` and `timeout` together. `mark 6.1 PASS`
- **Send me:** the counts.
- **Result:** run 16 Oct 17:14:40–17:22:45 CST, 7 min 58 s. **Pass.** 8,057 names and 8,057 results. No `other` and no `timeout`.
  - `blob_missing` 7,216, `layers_missing` 291, `ok` 523, `stale_record` 27.
  - Against 15 Oct (6,847 names: 6,313 `blob_missing`, 253 `layers_missing`, 254 `ok`, 27 `stale_record`): `blob_missing` grew by 903, the build tags that run skipped; `ok` grew by 269; `layers_missing` grew by 38; `stale_record` is unchanged. 903 + 269 + 38 = 1,210, which is 8,057 − 6,847. Marked at 17:27:18.

### 6.2 Mount test of every container (5 to 10 minutes)

- **Why:** a container whose layers can't mount can't start, and it keeps its broken records alive. A container that mounts is kept, whatever its image.
- **Risk:** low. For each stopped container, `docker cp` mounts its layers for a moment and copies `/etc/hostname` to nowhere. Nothing is started, and nothing in the container changes.

```bash
cd "$R/out"
time bash "$R/bin/container_test.sh" 6.2_container_test.tsv
wc -l < 6.2_container_test.tsv
cut -f1 6.2_container_test.tsv | sort | uniq -c
```

- **Expected:** one line per container. `running` for those from 5.3, `ok` for those that mount, `layers_missing` for at least the 158 of 15 Oct and the two from 12 Oct, and `dead` for about 6. `other` and `timeout` should be few.
- **Pass check:** one line per container; fewer than 20 `other` and `timeout` together. `mark 6.2 PASS`
- **Send me:** the counts.
- **Result:** run 16 Oct, finished by 17:29. **Reviewed, not a clean pass.** 501 lines, and `docker ps -a` is 501 now. Counts: `layers_missing` 330, `dead` 21, `ok` 20, `other` 130. No `running` and no `timeout`.
  - Nine containers from step 2.4 are gone. All nine were `created` and had never started: four `proofke-check-*`, `p3k-proofke-startup-8a38cf1f0a0b4e7093c1f21d62737d6a`, and `silly_pasteur`, `laughing_solomon`, `flamboyant_mclean`, `zen_kalam`.
  - All 130 `other` lines are the same error: `RWLayer of container … is unexpectedly nil`. The pass limit is under 20, so these stay in `review_containers.tsv` and step 8 does not remove them.

### 6.3 Sort into remove, keep, review and save

- **Why:** step 8 removes exactly what these lists say. `review` is everything the tests could not classify; it is not removed, and you send it to me. `save` is the images that have every file but no folder; step 7 saves them first. Build tags (`moby-dangling@…`) can't be saved by name, so they are not in `save`.
- **Risk:** none. It writes lists.

```bash
cd "$R/out"
awk -F'\t' '$1=="blob_missing"||$1=="layers_missing"||$1=="stale_record"{print $2}' 6.1_start_test.tsv > remove_images.txt
awk -F'\t' '$1=="ok"{print $2}' 6.1_start_test.tsv > keep_images.txt
awk -F'\t' '$1!="ok"&&$1!="blob_missing"&&$1!="layers_missing"&&$1!="stale_record"' 6.1_start_test.tsv > review_images.tsv
awk -F'\t' '$1=="layers_missing"||$1=="dead"{print $2}' 6.2_container_test.tsv > remove_containers.txt
awk -F'\t' '$1=="ok"||$1=="running"' 6.2_container_test.tsv > keep_containers.tsv
awk -F'\t' '$1!="ok"&&$1!="running"&&$1!="layers_missing"&&$1!="dead"' 6.2_container_test.tsv > review_containers.tsv
awk -F'\t' '$1=="layers_missing"{print $2}' 6.1_start_test.tsv | grep -v -e '^moby-dangling@' -e '^sha256:' > save_all.txt
grep '^public.ecr.aws/d3j8x8q7/swe-bench-202605:' save_all.txt > save_deepswe.txt
grep -v '^public.ecr.aws/d3j8x8q7/swe-bench-202605:' save_all.txt > save_other.txt
wc -l remove_images.txt keep_images.txt review_images.tsv remove_containers.txt keep_containers.tsv review_containers.tsv save_deepswe.txt save_other.txt
sed -E 's#[:@][^/]*$##' remove_images.txt | sort | uniq -c | sort -rn | head -n 25 | tee 6.3_remove_by_repo.txt
cut -f1,2,3 review_images.tsv | head -n 20
cut -f1,3,4,6 review_containers.tsv | head -n 20
```

- **Expected:**
  - `remove_images.txt` about 7,500: the 6,593 failing images of 15 Oct, plus most of the 903 build tags.
  - `keep_images.txt` about 254 or more; `save_deepswe.txt` exactly 113; `save_other.txt` about 140.
  - `remove_containers.txt` about 160 or more.
  - The top of the removal by repository; DeepSWE's `public.ecr.aws/d3j8x8q7/swe-bench-202605` is among them with 113.
  - The review lines, if any.
- **Pass check:** `save_deepswe.txt` is 113, and the other counts are within about 10% of the 15 Oct numbers, or the difference is explained. `mark 6.3 PASS`
- **Send me:** the whole output.
- **Result:** run 16 Oct, on screen at 17:29. **Pass**, with the container difference explained. `save_deepswe.txt` is 113. `save_other.txt` is 178, which is the 15 Oct 140 plus the 38 new `layers_missing` images. `save_all.txt` is 291, so every `layers_missing` image has a name that can be saved. `remove_images.txt` 7,534 and `keep_images.txt` 523 match step 6.1. `review_images.tsv` is empty. `remove_containers.txt` is 351 (330 + 21 dead), above the 15 Oct figure of about 160. `keep_containers.tsv` is 20. `review_containers.tsv` is the 130 nil-layer containers, and they are not in the removal list. DeepSWE's repository is in the removal summary with 113. Marked at 17:33:20.

### 6.4 Root approves the removal (gate)

- **Where:** terminal 2, to root, with the output of 6.3.
- **Why:** step 8 can't be undone. Root sees the real numbers before it starts.
- **Risk:** none.

```text
Repair step 6.4: OK before anything is removed? The lists are in <R>/out/ on the worker.
<paste the wc -l lines and the removal by repository from step 6.3>
Not removed, for review: <n> images (review_images.tsv), <n> containers (review_containers.tsv).
Saved first in step 7: 113 DeepSWE images and <n> other images.
```

Then, in window 0:

```bash
mark 6.4 APPROVED "root <time>: remove <n> images, <n> containers, and the build cache"
```

- **Pass check:** root's written OK, recorded with `mark`.
- **Result:** marked 16 Oct 17:35:14 CST. **Approved.** The note records root's approval at 17:33: remove 7,534 images, 351 containers, and the build cache. The 130 nil-layer containers stay in `review_containers.tsv`.

## Step 7: save the images that still have all their files

### 7.1 Free space, and the save folder

- **Why:** the two tar files take about 52 GB. Step 10 and 11 unpack them again later, which needs more.
- **Risk:** low. It creates one root-only folder on `/data`.

```bash
install -d -m 700 /data/cloud-worker-repair /data/cloud-worker-repair/saved
df -h /data | tee "$R/out/7.1_df.txt"
```

- **Expected:** at least 200 GB available.
- **Pass check:** at least 200 GB. If less, stop: root decides. `mark 7.1 PASS`
- **Result:** run 16 Oct 17:36:18 CST. **Pass.** `/data` had 335 GB free. The `mark` line is still to run.

### 7.2 Save the 113 published DeepSWE images (10 to 20 minutes)

- **Why:** step 8 deletes the downloaded files of every image it removes. A tar file keeps DeepSWE's 113 for loading back in step 10, without a 24 GB download, and without depending on the registry still serving these tags.
- **Risk:** low. It reads the files and writes one tar file.

```bash
cd "$R/out"
time docker save -o /data/cloud-worker-repair/saved/deepswe-published.tar $(cat save_deepswe.txt) 2>&1 | tee 7.2_save_deepswe.txt
ls -lh /data/cloud-worker-repair/saved/deepswe-published.tar
```

- **Expected:** no message from `docker save`, the time, and a file of about 24 to 26 GB.
- **Pass check:** no error, and the size. `mark 7.2 PASS`
- **Result:** run 16 Oct 17:36:18–17:42:22 CST, 5 min 59 s. **Pass.** No error from `docker save`. `deepswe-published.tar` is 23 GiB, about 25 GB, inside the 24 to 26 GB range. The `mark` line is still to run.

### 7.3 Save the other images (10 to 20 minutes)

- **Why:** the same, for the local images that still have every file: local `deepswe*`, clarify, chrys-bench, jiuwenswarm probes and others, about 140. Their owners get them back in step 11.
- **Risk:** low.

```bash
cd "$R/out"
time docker save -o /data/cloud-worker-repair/saved/other-local.tar $(cat save_other.txt) 2>&1 | tee 7.3_save_other.txt
ls -lh /data/cloud-worker-repair/saved/other-local.tar
```

- **Expected:** no message, and a file of about 28 GB. It holds the DeepSWE base layers again.
- **Only if it fails** (for example `blob not found`): save them one at a time instead, and list the ones that fail. The partial file stays; root removes it after the review.

```bash
cd "$R/out"
install -d -m 700 /data/cloud-worker-repair/saved/one
n=0; while read -r i; do n=$((n+1)); if docker save -o "/data/cloud-worker-repair/saved/one/$n.tar" "$i" 2>> 7.3_save_one_errors.txt; then printf '%s\t%s\n' "$n.tar" "$i"; else printf 'FAILED\t%s\n' "$i"; fi; done < save_other.txt > 7.3_save_one.tsv
grep -c FAILED 7.3_save_one.tsv
```

- **Pass check:** no error; or, with the fallback, `0` failed, or the failed names sent to me for their owners. `mark 7.3 PASS`
- **Result:** run 16 Oct 17:42:22–18:05:58 CST, 23 min 5 s. **Pass for the command.** No error from `docker save`. `other-local.tar` is 82 GiB, about 88 GB. The planning figure of about 28 GB was for about 140 images; this list has 178, and the tar is still larger than that scaling. Step 7.4 has to show 178 images in the tar before step 8. The `mark` line is still to run.

### 7.4 Check the tar files

- **Why:** step 8 removes the originals. Before that, check that each tar has every image, and record a checksum.
- **Risk:** none. It reads the tar files. The checksums take a few minutes.

```bash
cd /data/cloud-worker-repair/saved
for t in *.tar; do printf '%s\t' "$t"; tar -xOf "$t" index.json | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["manifests"]), "images")'; done | tee "$R/out/7.4_tars.txt"
wc -l < "$R/out/save_deepswe.txt"; wc -l < "$R/out/save_other.txt"
sha256sum *.tar | tee -a "$R/out/7.4_tars.txt"
```

- **Expected:** `deepswe-published.tar 113 images`, and `other-local.tar <n> images` with the same `<n>` as `save_other.txt`. Then a checksum per file. With the fallback of 7.3, only `deepswe-published.tar` is listed.
- **Pass check:** the counts equal the lists. If not, send me the output before step 8. `mark 7.4 PASS`
- **Result:** run 16 Oct 18:10:47–18:22:08 CST. **Pass**, marked at 18:24:23. `deepswe-published.tar` has 113 images, `other-local.tar` has 178, matching both lists. Checksums are in `7.4_tars.txt`. Paused here for the weekend. Resumed 20 Oct 10:19 CST in the same log folder. New recording: `sessions/101924-main.log`. Docker was still closed to the `docker` group, and both tar files were still in place. Next command is step 8.1.

## Step 8: remove what can't work, and let containerd clear the records

This step needs `APPROVED` in 6.4 and PASS in 7.4. It is the one that can't be undone.

### 8.1 Remove the containers that can't mount (5 minutes)

- **Why:** each of them keeps broken layer records alive, and none of them can start.
- **Risk:** **high.** It removes the containers and their writable layers. Without `-v`, their volumes stay. Owners were told in 0.2.

```bash
cd "$R/out"
wc -l < remove_containers.txt
xargs -a remove_containers.txt -r -n 50 docker rm > 8.1_containers_rm.txt 2>&1
grep -c -v '^Error' 8.1_containers_rm.txt
grep '^Error' 8.1_containers_rm.txt | head
```

- **Expected:** the same number twice (listed, removed), and no `Error` line.
- **Pass check:** the counts are equal, and there is no `Error`. Don't use `-f` on a failure; send it. `mark 8.1 PASS`
- **Result:** run 20 Oct 10:20:31–10:22:37 CST. **Pass**, marked at 10:24:43. Listed 351, removed 351, no `Error` line. 150 containers remained: the 130 held for review and the 20 that mounted.

### 8.2 Remove the build cache

- **Why:** 6,807 build-cache entries sit on DeepSWE's broken layers alone. The cache only makes builds faster.
- **Risk:** **high** in reach, low in loss: every user's next build of an image is slower once.

```bash
cd "$R/out"
time docker builder prune -af > 8.2_builder_prune.txt 2>&1; tail -n 2 8.2_builder_prune.txt
docker builder prune -af 2>&1 | tail -n 1 | tee -a 8.2_builder_prune.txt
```

- **Expected:** `Total: <size>`, then `Total: 0B` for the second run.
- **Pass check:** `Total: 0B`. `mark 8.2 PASS`
- **Result:** run 20 Oct 10:24:43–10:27:25 CST, 2 min 35 s. **Pass**, marked at 10:30:59. First prune reclaimed `Total: 1.738TB`. Second prune printed `Total: 0B`.

### 8.3 Remove the images that fail, and wait for the cleanup (30 minutes to 3 hours)

- **Why:** Docker's own image commands read every image's files first, and they fail on this store (`docker image ls`, report section 2.3). `ctr images rm` removes only the name record and reads nothing. Docker on this host keeps no image list of its own: these are the names Docker shows. The last name is removed with `--sync`, which waits until containerd's cleanup has finished. That cleanup removes every record that nothing names any more, with its file or folder if one is left.
- **Risk:** **high.** About 7,500 image names, and every file and folder only they use. The 253 saved in step 7 come back in steps 10 and 11.

```bash
cd "$R/out"
wc -l < remove_images.txt
time head -n -1 remove_images.txt | xargs -r -d '\n' -n 200 ctr -n moby images rm > 8.3_images_rm.txt 2>&1
time tail -n 1 remove_images.txt | xargs -r -d '\n' ctr -n moby images rm --sync >> 8.3_images_rm.txt 2>&1
grep -c -v -i -E 'error|unable' 8.3_images_rm.txt
grep -i -E 'error|unable' 8.3_images_rm.txt | head
```

To watch the cleanup, in window 1:

```bash
watch -n 60 'ls -U /data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots | wc -l; ls -U /data/containerd/io.containerd.content.v1.content/blobs/sha256 | wc -l; df -h /data | tail -n 1'
```

- **Expected:** the first `time` returns within minutes. The second blocks until the cleanup is done: from 30 minutes to a few hours. In window 1 the folder and file counts fall and the free space grows. Then the same count as listed, and no error line. Don't interrupt it, and start nothing else meanwhile.
- **Pass check:** the `--sync` call returned; removed count equals listed count; no error. `mark 8.3 PASS`
- **Result:** run 20 Oct, `--sync` returned in 16 min 15 s. **Pass**, marked at 10:51:50. 7,534 names listed and 7,534 lines in the removal log, with none missing. 523 images remain, the same count as `keep_images.txt`. `/data` free space went from 253 GB to 673 GB, so the cleanup did delete files. The 115 lines matched by `grep error` are image names that contain the word `error`, not removal failures.

### 8.4 Remove the download records whose file is gone

- **Why:** some images that start still have download records whose file is gone (a layer was deleted, but its folder survived). With the record in place, a later pull that needs that file would skip it (rule 1 in "Read this first"). Each digest is checked twice: when the list is made, and again just before its removal.
- **Risk:** low. No file is deleted; only records of files that are already gone. A later pull downloads the file again.

```bash
cd "$R/out"
BLOBS=/data/containerd/io.containerd.content.v1.content/blobs/sha256
ctr -n moby content active
ctr -n moby content ls -q | while read -r d; do [ -e "$BLOBS/${d#sha256:}" ] || echo "$d"; done > 8.4_records_without_blob.txt
wc -l < 8.4_records_without_blob.txt
while read -r d; do [ -e "$BLOBS/${d#sha256:}" ] || echo "$d"; done < 8.4_records_without_blob.txt | xargs -r -n 200 ctr -n moby content rm > 8.4_content_rm.txt 2>&1
head 8.4_content_rm.txt
ctr -n moby images tag docker.io/library/busybox:1.36 repair.local/gc-trigger:1
time ctr -n moby images rm --sync repair.local/gc-trigger:1
```

- **Expected:** only the header of `content active`; a count, possibly `0`; nothing from `content rm`; then `repair.local/gc-trigger:1` twice. The last two lines make one more cleanup run and wait for it; that run should take seconds to minutes.
- **Pass check:** no download running, and no error. `mark 8.4 PASS`
- **Result:** run 20 Oct, finished 10:52:17 CST. **Pass**, marked at 10:57:04. `content active` printed only its header. 6,937 records had no file, and `content rm` printed those 6,937 digests with no other line. The guide's "nothing from `content rm`" means no error text; `ctr` prints each removed digest, and that is what `head` showed. `repair.local/gc-trigger:1` printed twice. The last cleanup returned in 0.5 s, which is expected after the 16-minute cleanup in 8.3. 523 images remain, and the trigger tag is gone.

### 8.5 Check the store (gate)

- **Why:** before anything is pulled or loaded: every download record has its file; there are no more layer records than folders; and every image that was kept still starts, so the cleanup took nothing they need.
- **Risk:** low. It reads, and starts each kept image with `--rm`.

```bash
cd "$R/out"
bash "$R/bin/store_check.sh" docker.io/library/alpine:3.21 docker.io/library/busybox:1.36 2>&1 | tee 8.5_store_check.txt
nice bash "$R/bin/start_test.sh" keep_images.txt 8.5_keep_test.tsv
cut -f1 8.5_keep_test.tsv | sort | uniq -c
```

- **Expected:**
  - `open_meta.db`: the same pair.
  - `records_without_blob 0`, `blobs_without_record 0` or close to it.
  - `snapshot_records` not more than `snapshot_folders`, ideally equal.
  - `images` about the size of `keep_images.txt`; fewer `leases` than in 2.3; more free space than in 2.3; both canaries `ok`.
  - `RESULT OK`, or `RESULT WARN` only for the disk lines.
  - Every kept image `ok`.
- **Pass check:** `records_without_blob 0`; layer records not more than folders; every kept image `ok`. `mark 8.5 PASS`
- **If there are more layer records than folders:** something still names broken layer records. Don't continue. Send me `8.5_store_check.txt`, and the output of `ctr -n moby leases ls | head -n 20` and `ctr -n moby snapshots ls | awk 'NR>1{print $NF}' | sort | uniq -c`.
- **Achieves:** a store whose records match its files and folders. Everything from here adds new, correct records.
- **Result:** run 20 Oct 10:57–11:14:55 CST. **Not a pass.** `open_meta.db` is `66309:37131376` both open and on disk. Images are 523. `blobs_without_record` is 0. Both canaries are `ok`. `/data` at 95% is the expected disk warning. The kept-image test wrote 523 lines: 520 `ok`, 3 `timeout`, no other result. Those three `tdd-python-recovery` images start on their own (3.2 s, 0.7 s, 0.7 s); the 120 s limit was hit while eight starts ran together.
  - `records_without_blob` is 52,588. `moby` itself has 3,763 download records and none are missing a file. `moby_history` has 52,506 records, 52,340 with no file, 0 images and 0 layer records, plus 7,235 leases. The 16 `lolbench-recovery-*` namespaces add about 250 missing records. Four `capbench-recovery-*` namespaces have no missing download records; one of them has 1,357 layer records.
  - Layer records are 11,142 and folders were 5,803, so 5,339 records have no folder. `moby` alone has 9,825 layer records: 2,462 named `sha256:…`, 7,012 short build-cache names, and about 350 container init records. `moby` also still has 17,384 leases. Step 9 stays stopped until these records are accounted for.
  - Marked FAIL at 11:21:30.

### 8.6 Remove the build-cache leases

- **Why:** 6,929 committed build-cache snapshots in `moby` are each held by a lease of the same name. 3,832 of them have no folder. None of the 523 kept images uses one of those snapshots: every kept image's own snapshot is a `sha256:` layer, and none of those image snapshots is missing its folder. The leases are what still stop cleanup from dropping the build-cache records. `moby_history` is not part of this step.
- **Risk:** low for the kept images. Cleanup can also delete build-cache folders that are still on disk. It does not remove `sha256:` image layers, container init layers, or active snapshots.

```bash
cd "$R/out"
ctr -n moby snapshots ls | awk 'NR>1 && $NF=="Committed" && $1 !~ /^sha256:/ && $1 !~ /-init/ {print $1}' | sort -u > 8.6_build_snapshots.txt
ctr -n moby leases ls | awk 'NR>1{print $1}' | sort -u > 8.6_leases.txt
comm -12 8.6_leases.txt 8.6_build_snapshots.txt > 8.6_leases_to_remove.txt
n=$(wc -l < 8.6_leases_to_remove.txt)
echo "$n"
if [ "$n" -eq 6929 ]; then
  xargs -r -d '\n' -n 200 ctr -n moby leases rm < 8.6_leases_to_remove.txt > 8.6_leases_rm.txt 2>&1
  wc -l < 8.6_leases_rm.txt
  grep -i -E 'error|unable|failed' 8.6_leases_rm.txt | head
  ctr -n moby images tag docker.io/library/busybox:1.36 repair.local/gc-trigger:1
  time ctr -n moby images rm --sync repair.local/gc-trigger:1
  bash "$R/bin/store_check.sh" docker.io/library/alpine:3.21 docker.io/library/busybox:1.36 2>&1 | tee 8.6_store_check.txt
else
  echo "STOP count is $n"
fi
```

- **Expected:** `6929`. Then that many removal lines, no error, the trigger name twice, and a store check. The lease removal is a minute or two. The cleanup and the store check can take 10 to 25 minutes. `records_without_blob` can stay high, because `moby_history` is untouched. The layer-record gap should be much smaller than 5,339. Images stay 523. Both canaries `ok`.
- **Pass check:** the count was 6,929, no error, images still 523, and the layer-record gap is smaller. `mark 8.6 PASS`
- **Result:** run 20 Oct, store check finished 11:45:03 CST. **Pass**, marked at 11:53:46. The removal list and the removal log are both 6,929, with no error. Images stay 523. `open_meta.db` is `66309:37131376` both open and on disk. Both canaries are `ok`. `/data` free space went from 673 GB to 790 GB. Leases in `moby` went from 17,384 to 10,455. Layer records went from 11,142 to 4,567 and folders from 5,803 to 2,873, so the gap went from 5,339 to 1,694. `records_without_blob` is still 52,588, all outside `moby`. Of the 1,694, 526 are `moby` records whose folder is gone. None of those 526 is in the parent chain of the 523 kept images. Step 9 stays stopped.

### 8.7 Remove folderless layer records nothing still needs

- **Why:** 288 of those 526 records are committed, have no folder, are not in a kept image's layer chain, and are not the parent of any snapshot that still has a folder. A later pull can skip a layer while one of these records remains. The other 186 stay because a snapshot that still has a folder names them as its parent. The 52 active ones stay with the containers that are still on the host. `moby_history` is not part of this step.
- **Risk:** low for the kept images. Their layer chains are not in the removal list. A record that still has a child is skipped and tried again after the child is gone.

```bash
cd "$R/out"
python3 - << 'PY'
import os, struct, subprocess
from collections import defaultdict
path = "/data/containerd/io.containerd.snapshotter.v1.overlayfs/metadata.db"
snapdir = "/data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots"
f = open(path, "rb")
def rd(off, n):
    f.seek(off)
    return f.read(n)
def u16(b, o): return struct.unpack_from("<H", b, o)[0]
def u32(b, o): return struct.unpack_from("<I", b, o)[0]
def u64(b, o): return struct.unpack_from("<Q", b, o)[0]
def read_page(pgid):
    b = rd(pgid * 4096, 4096)
    ov = u32(b, 12)
    if ov:
        b = rd(pgid * 4096, 4096 * (ov + 1))
    return b
def items(page):
    if u16(page, 8) & 0x01:
        acc = []
        for i in range(u16(page, 10)):
            acc.extend(items(read_page(u64(page, 16 + i * 16 + 8))))
        return acc
    acc = []
    for i in range(u16(page, 10)):
        base = 16 + i * 16
        flags, pos, ksize, vsize = u32(page, base), u32(page, base + 4), u32(page, base + 8), u32(page, base + 12)
        k = page[base + pos:base + pos + ksize]
        v = page[base + pos + ksize:base + pos + ksize + vsize]
        if flags & 0x01:
            acc.append((k, u64(v, 0), v[16:] if len(v) > 16 else None))
        else:
            acc.append((k, v))
    return acc
def fields(root, inline):
    page = inline if inline is not None else read_page(root)
    return {a: b for a, b in ((it[0], it[1]) for it in items(page) if len(it) == 2)}
def uvarint(b):
    x = s = 0
    for c in b:
        x |= (c & 0x7F) << s
        if c < 128:
            return x
        s += 7
b0, b1 = read_page(0), read_page(1)
meta = b0 if u64(b0, 64) >= u64(b1, 64) else b1
top = items(read_page(u64(meta, 32)))
v1 = items(read_page(top[0][1]))
recs = items(read_page([r for r in v1 if r[0] == b"snapshots"][0][1]))
present, missing = set(), set()
for key, root, inline in recs:
    key_s = key.decode()
    if not key_s.startswith("moby/"):
        continue
    name = key_s.split("/", 2)[-1]
    sid = str(uvarint(fields(root, inline)[b"id"]))
    (present if os.path.isdir(os.path.join(snapdir, sid)) else missing).add(name)
parent, kind = {}, {}
for line in subprocess.check_output(["ctr", "-n", "moby", "snapshots", "ls"], text=True).splitlines()[1:]:
    p = line.split()
    if len(p) >= 2:
        parent[p[0]] = p[1]
        kind[p[0]] = p[-1]
usage = []
for img in open("keep_images.txt"):
    img = img.strip()
    if not img:
        continue
    lines = subprocess.check_output(["ctr", "-n", "moby", "images", "usage", img], text=True, stderr=subprocess.DEVNULL).splitlines()
    if len(lines) > 1:
        usage.append(lines[1].split()[0])
chain, stack = set(), list(usage)
while stack:
    n = stack.pop()
    if not n or n == "-" or n in chain:
        continue
    chain.add(n)
    if parent.get(n):
        stack.append(parent[n])
kids = defaultdict(set)
for n, p in parent.items():
    if p and p != "-":
        kids[p].add(n)
structural = {n for n in missing if any(c in present for c in kids.get(n, ()))}
removable = sorted(n for n in missing if kind.get(n) == "Committed" and n not in chain and n not in structural)
open("8.7_snapshots_to_remove.txt", "w").write("".join(n + "\n" for n in removable))
print(len(removable))
PY
n=$(wc -l < 8.7_snapshots_to_remove.txt)
echo "$n"
if [ "$n" -eq 288 ]; then
  cp 8.7_snapshots_to_remove.txt 8.7_left.txt
  : > 8.7_snapshots_rm.txt
  for pass in 1 2 3 4 5 6 7 8; do
    [ -s 8.7_left.txt ] || break
    : > 8.7_next.txt
    while read -r k; do
      if ctr -n moby snapshots rm "$k" >> 8.7_snapshots_rm.txt 2>&1; then
        echo "$k" >> 8.7_snapshots_rm.txt
      else
        echo "$k" >> 8.7_next.txt
      fi
    done < 8.7_left.txt
    mv 8.7_next.txt 8.7_left.txt
  done
  echo "left $(wc -l < 8.7_left.txt)"
  echo "images $(ctr -n moby images ls -q | wc -l)"
  ctr -n moby images tag docker.io/library/busybox:1.36 repair.local/gc-trigger:1
  time ctr -n moby images rm --sync repair.local/gc-trigger:1
  bash "$R/bin/store_check.sh" docker.io/library/alpine:3.21 docker.io/library/busybox:1.36 2>&1 | tee 8.7_store_check.txt
else
  echo "STOP count is $n"
fi
```

- **Expected:** `288`, then `left 0`, then `images 523`. The trigger name prints twice. The store check still warns about the 52,588 download records, because `moby_history` is untouched. The layer-record gap should be smaller than 1,694. Building the list reads every kept image, so this can take 10 to 20 minutes. Stop if the first number is not 288.
- **Pass check:** `left 0`, images still 523, and the layer-record gap is smaller. `mark 8.7 PASS`
- **Result:** run 20 Oct, store check finished 12:11:14 CST. **Not a pass.** The list was 288, and all 288 removals failed with `cannot remove snapshot with child`. `left 288`. Images stayed 523. The store check is unchanged: 4,567 layer records, 2,873 folders, gap 1,694, `records_without_blob` 52,588. The cleanup returned in 28 seconds and did not delete these records. Do not mark this step pass. Step 9 stays stopped.

### 8.8 Remove the child records under the unattached layers

- **Why:** containerd refuses to remove a layer record while a child record names it. Of the 288, 183 are parents of a layer that an active container still uses, so they stay. The other 105 are not in a kept image's chain and not in an active container's chain. Under those 105 is a closed tree of 4,441 records, children included. Nothing in that tree is a kept image layer or an active container layer.
- **Risk:** low for the 523 kept images and for the containers that are still active. The removal log keeps any name that fails.
- **How to run it:** the script is already the file `$R/bin/8.8.sh`. Do not paste the long block. The repair window is already recording into `$R/sessions/101924-main.log`. The short command also keeps a clean copy at `$R/out/8.8_console.txt`.

```bash
bash "$R/bin/8.8.sh" 2>&1 | tee "$R/out/8.8_console.txt"
```

The file contains this:

```bash
cd "$R/out"
python3 - << 'PY'
import struct, subprocess
from collections import defaultdict
path = "/data/containerd/io.containerd.metadata.v1.bolt/meta.db"
f = open(path, "rb")
def rd(off, n):
    f.seek(off); return f.read(n)
def u16(b, o): return struct.unpack_from("<H", b, o)[0]
def u32(b, o): return struct.unpack_from("<I", b, o)[0]
def u64(b, o): return struct.unpack_from("<Q", b, o)[0]
def read_page(pgid):
    b = rd(pgid * 4096, 4096)
    ov = u32(b, 12)
    if ov:
        b = rd(pgid * 4096, 4096 * (ov + 1))
    return b
def items(page):
    if u16(page, 8) & 0x01:
        acc = []
        for i in range(u16(page, 10)):
            acc.extend(items(read_page(u64(page, 16 + i * 16 + 8))))
        return acc
    acc = []
    for i in range(u16(page, 10)):
        base = 16 + i * 16
        flags, pos, ksize, vsize = u32(page, base), u32(page, base + 4), u32(page, base + 8), u32(page, base + 12)
        k = page[base + pos:base + pos + ksize]
        v = page[base + pos + ksize:base + pos + ksize + vsize]
        if flags & 0x01:
            acc.append(("b", k, u64(v, 0), v[16:] if len(v) > 16 else None))
        else:
            acc.append(("k", k, v))
    return acc
b0, b1 = read_page(0), read_page(1)
meta = b0 if u64(b0, 64) >= u64(b1, 64) else b1
top = items(read_page(u64(meta, 32)))
v1 = items(read_page(top[0][2]))
moby = next(r for r in v1 if r[1] == b"moby")
mb = items(read_page(moby[2]))
sn = next(r for r in mb if r[1] == b"snapshots")
ov = items(read_page(next(r for r in items(read_page(sn[2])) if r[1] == b"overlayfs")[2]))
children, parent = defaultdict(list), {}
for rec in ov:
    name = rec[1].decode()
    inner = items(rec[3] if rec[3] is not None else read_page(rec[2]))
    for it in inner:
        if it[0] == "k" and it[1] == b"parent":
            parent[name] = it[2].decode()
        elif it[0] == "b" and it[1] == b"children":
            for k in items(it[3] if it[3] is not None else read_page(it[2])):
                children[name].append(k[1].decode())
kind = {}
for line in subprocess.check_output(["ctr", "-n", "moby", "snapshots", "ls"], text=True).splitlines()[1:]:
    p = line.split()
    if len(p) >= 2:
        kind[p[0]] = p[-1]
usage = []
for img in open("keep_images.txt"):
    img = img.strip()
    if not img:
        continue
    lines = subprocess.check_output(["ctr", "-n", "moby", "images", "usage", img], text=True, stderr=subprocess.DEVNULL).splitlines()
    if len(lines) > 1:
        usage.append(lines[1].split()[0])
def ancestors(seeds):
    out, st = set(), list(seeds)
    while st:
        n = st.pop()
        if not n or n == "-" or n in out:
            continue
        out.add(n)
        if parent.get(n):
            st.append(parent[n])
    return out
active = [n for n, k in kind.items() if k == "Active"]
protected = ancestors(usage) | ancestors(active) | set(active)
left = set(open("8.7_left.txt").read().split())
seen, st = set(left - protected), list(left - protected)
while st:
    n = st.pop()
    for c in children.get(n, []):
        if c in protected or c in seen:
            continue
        seen.add(c)
        st.append(c)
order, remaining = [], set(seen)
while remaining:
    leaves = sorted(n for n in remaining if all(c not in remaining for c in children.get(n, [])))
    if not leaves:
        break
    order.extend(leaves)
    remaining -= set(leaves)
open("8.8_order.txt", "w").write("".join(n + "\n" for n in order))
print(len(order))
PY
n=$(wc -l < 8.8_order.txt)
echo "$n"
if [ "$n" -eq 4441 ]; then
  : > 8.8_snapshots_rm.txt
  : > 8.8_left.txt
  ok=0; fail=0
  while read -r k; do
    if ctr -n moby snapshots rm "$k" >> 8.8_snapshots_rm.txt 2>&1; then
      ok=$((ok + 1))
    else
      fail=$((fail + 1))
      echo "$k" >> 8.8_left.txt
    fi
    if [ "$ok" -eq 0 ] && [ "$fail" -eq 30 ]; then echo "STOP early"; break; fi
    if [ $(((ok + fail) % 400)) -eq 0 ]; then echo "progress $ok removed, $fail failed"; fi
  done < 8.8_order.txt
  echo "removed $ok failed $fail"
  echo "images $(ctr -n moby images ls -q | wc -l)"
  ctr -n moby images tag docker.io/library/busybox:1.36 repair.local/gc-trigger:1
  time ctr -n moby images rm --sync repair.local/gc-trigger:1
  bash "$R/bin/store_check.sh" docker.io/library/alpine:3.21 docker.io/library/busybox:1.36 2>&1 | tee 8.8_store_check.txt
else
  echo "STOP count is $n"
fi
```

- **Expected:** `4441`. Then a progress line every 400 removals, then `removed 4441 failed 0` and `images 523`. The trigger name prints twice. `records_without_blob` can stay at 52,588. The layer-record gap should fall. This can take 15 to 25 minutes. Stop if the first number is not 4441, or if the line is `STOP early`.
- **Pass check:** `failed 0`, images still 523, and the layer-record gap is smaller. `mark 8.8 PASS`
- **Result:** run 20 Oct 12:33, ended 12:33:39. Not a pass. Do not mark. Do not re-run. The file printed `4441`, then `STOP early`, `removed 0 failed 30`, `images 523`. All 30 failures are `snapshot ... does not exist`. The trigger name was created and removed in 7.7 s. Store check is the same as 8.7: records 4,567, folders 2,873, gap 1,694, `records_without_blob` 52,588, images 523, both canaries ok, both database pairs match. Console copy: `$R/out/8.8_console.txt`. Follow-up read of the same database: 127 of the 4,441 names are real `ctr` layer records, and 4,314 exist only as child names on a parent. Those 127 all still have a child entry, so another `ctr snapshots rm` of them fails the same way. Across the `moby` namespace, 917 layer records list 19,696 child names that have no layer record of their own. Steps 8.9 to 8.12 clear those names and then remove the records that become free.

## The fix before any pull

`ctr snapshots rm` cannot delete a child name that is not a layer record. containerd 2.2.6 refuses to remove a layer record while any name is left in its `children` list, and it does not check that the named record still exists (`core/metadata/snapshot.go`, `Remove`). Garbage collection deletes a record without always clearing that name on the parent, which is the state step 8.8 found. A pull is not safe while those records remain: containerd skips a layer whose record already exists, even when the folder is gone.

The 523 Docker images already match their files. A read-only `ctr -n moby images check` on 20 Oct, outside the repair log, listed 523 images, every one `complete` and `unpacked true`, in 1.3 seconds. Step 8.9 records that check. The error left to remove is the leftover names and the folderless records that no image uses.

`moby_history` and the `lolbench-recovery-*` namespaces are not Docker images. They stay. They are why `store_check.sh` still prints `records_without_blob` above 0. A Docker pull reads the `moby` namespace only, so that number does not by itself block step 9. The block is step 8.12.

### 8.9 Check every Docker image, and count the leftover child names

- **Why:** this is the test before the database edit. Every image in `moby` must already have its downloaded files and its unpacked layers. The same run counts child names that are not layer records. Step 8.10 deletes that many names and no others.
- **Risk:** none. It reads, and writes only into `$R/out`.

```bash
cd "$R/out"
ctr -n moby images check > 8.9_images_check.txt
python3 - << 'PY'
rows = bad = 0
for line in open("8.9_images_check.txt"):
    if not line.strip() or line.startswith("REF"):
        continue
    rows += 1
    parts = line.split()
    if "complete" not in parts or parts[-1] != "true":
        bad += 1
        print(line[:200].rstrip())
print("images", rows)
print("bad", bad)
PY
python3 - << 'PY'
import struct
from collections import defaultdict
path = "/data/containerd/io.containerd.metadata.v1.bolt/meta.db"
f = open(path, "rb")
def rd(off, n):
    f.seek(off); return f.read(n)
def u16(b, o): return struct.unpack_from("<H", b, o)[0]
def u32(b, o): return struct.unpack_from("<I", b, o)[0]
def u64(b, o): return struct.unpack_from("<Q", b, o)[0]
def read_page(pgid):
    b = rd(pgid * 4096, 4096)
    ov = u32(b, 12)
    if ov:
        b = rd(pgid * 4096, 4096 * (ov + 1))
    return b
def items(page):
    if u16(page, 8) & 0x01:
        acc = []
        for i in range(u16(page, 10)):
            acc.extend(items(read_page(u64(page, 16 + i * 16 + 8))))
        return acc
    acc = []
    for i in range(u16(page, 10)):
        base = 16 + i * 16
        flags, pos, ksize, vsize = u32(page, base), u32(page, base + 4), u32(page, base + 8), u32(page, base + 12)
        k = page[base + pos:base + pos + ksize]
        v = page[base + pos + ksize:base + pos + ksize + vsize]
        if flags & 0x01:
            acc.append(("b", k, u64(v, 0), v[16:] if len(v) > 16 else None))
        else:
            acc.append(("k", k, v))
    return acc
b0, b1 = read_page(0), read_page(1)
meta = b0 if u64(b0, 64) >= u64(b1, 64) else b1
top = items(read_page(u64(meta, 32)))
v1 = items(read_page(top[0][2]))
moby = next(r for r in v1 if r[1] == b"moby")
mb = items(read_page(moby[2]))
sn = next(r for r in mb if r[1] == b"snapshots")
ov = items(read_page(next(r for r in items(read_page(sn[2])) if r[1] == b"overlayfs")[2]))
real = set()
children = defaultdict(list)
for rec in ov:
    if rec[0] != "b":
        continue
    name = rec[1].decode()
    real.add(name)
    inner = items(rec[3] if rec[3] is not None else read_page(rec[2]))
    for it in inner:
        if it[0] == "b" and it[1] == b"children":
            for k in items(it[3] if it[3] is not None else read_page(it[2])):
                children[name].append(k[1].decode())
orphan = 0
parents = 0
for name, kids in children.items():
    missing = [c for c in kids if c not in real]
    if missing:
        parents += 1
        orphan += len(missing)
print("real_snapshots", len(real))
print("orphan_child_keys", orphan)
print("parents_with_orphan", parents)
open("8.9_orphan_count.txt", "w").write(str(orphan) + "\n")
PY
echo "orphan file $(cat 8.9_orphan_count.txt)"
```

- **Expected:** `images 523` and `bad 0`, with no image line printed before them. Then the three count lines, and the orphan file repeating the orphan count. The earlier 19,696 figure was only the child names under one part of the store. This step counts every leftover child name in `moby`.
- **Pass check:** `images 523`, `bad 0`, and `8.9_orphan_count.txt` holds the printed `orphan_child_keys` number. `mark 8.9 PASS`
- **If `bad` is not 0:** stop. Do not run 8.10. Send the printed image lines.
- **Achieves:** a recorded proof that all 523 Docker images match their files, and the exact number of leftover child names step 8.10 may delete.
- **Send me:** the `images` / `bad` lines and the three count lines.
- **Result:** run 20 Oct 12:57 CST. **Pass**, marked at 13:03:32. `images 523`, `bad 0`, `real_snapshots 4870`, `orphan_child_keys 25442`, `parents_with_orphan 2005`, orphan file `25442`. Step 8.10 deletes 25,442 names, the number in that file, and no others.

### 8.10 Clear the leftover child names

- **Why:** those names are keys in each parent's `children` bucket (`v1/moby/snapshots/overlayfs/<parent>/children`). containerd 2.2.6 has no command that deletes a key whose layer record is already gone. This step deletes only keys that are not themselves layer records, using the same bbolt library containerd 2.2.6 uses (`go.etcd.io/bbolt v1.4.3`), opened the same way containerd opens this file (`NoFreelistSync` true). It writes nothing unless the number it sees equals `8.9_orphan_count.txt`.
- **Risk:** **high** for running containers. Docker and containerd stop for a few minutes, as in step 3.1, then start again. The 523 images are not removed. If the image check after the start is not clean, the copy of `meta.db` is put back before step 8.11.

Build the program first, with Docker still running. The toolchain is unpacked under `/tmp` and removed at the end of the build. The program stays in `$R/bin`.

```bash
df -B1G --output=avail / | tail -n 1
want=$(cat "$R/out/8.9_orphan_count.txt")
echo "want $want"
cd /tmp
curl -fsSL -o go1.27.3.linux-amd64.tar.gz https://go.dev/dl/go1.27.3.linux-amd64.tar.gz
echo "65efbc80fdc1fa2cd2631cad5b481777a68e8725f34e89f50fc6ac714e379b40  go1.27.3.linux-amd64.tar.gz" | sha256sum -c
rm -rf /tmp/go
tar -C /tmp -xzf go1.27.3.linux-amd64.tar.gz
export PATH=/tmp/go/bin:$PATH
export GOPATH=/tmp/gopath
export GOCACHE=/tmp/gocache
rm -rf "$R/bin/clear-orphan-src"
mkdir -p "$R/bin/clear-orphan-src"
cd "$R/bin/clear-orphan-src"
go mod init clear-orphan-children
go get go.etcd.io/bbolt@v1.4.3
cat > main.go << 'EOF'
package main

import (
	"fmt"
	"os"
	"strconv"
	"time"

	bolt "go.etcd.io/bbolt"
)

func main() {
	if len(os.Args) != 3 {
		fmt.Fprintln(os.Stderr, "usage: clear-orphan-children meta.db EXPECTED_COUNT")
		os.Exit(2)
	}
	want, err := strconv.Atoi(os.Args[2])
	if err != nil || want < 1 {
		fmt.Fprintln(os.Stderr, "EXPECTED_COUNT must be a positive number")
		os.Exit(2)
	}
	opts := *bolt.DefaultOptions
	opts.NoFreelistSync = true
	opts.Timeout = 5 * time.Second
	db, err := bolt.Open(os.Args[1], 0644, &opts)
	if err != nil {
		fmt.Fprintf(os.Stderr, "open: %v\n", err)
		os.Exit(1)
	}
	defer db.Close()
	seen, err := countOrphans(db)
	if err != nil {
		fmt.Fprintf(os.Stderr, "count: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("seen %d\n", seen)
	if seen != want {
		fmt.Printf("STOP seen %d expected %d\n", seen, want)
		os.Exit(2)
	}
	deleted, err := deleteOrphans(db)
	if err != nil {
		fmt.Fprintf(os.Stderr, "delete: %v\n", err)
		os.Exit(1)
	}
	left, err := countOrphans(db)
	if err != nil {
		fmt.Fprintf(os.Stderr, "recount: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("deleted %d remaining %d\n", deleted, left)
	if deleted != want || left != 0 {
		fmt.Printf("STOP deleted %d remaining %d\n", deleted, left)
		os.Exit(1)
	}
}

func overlay(tx *bolt.Tx) (*bolt.Bucket, error) {
	b := tx.Bucket([]byte("v1"))
	if b == nil {
		return nil, fmt.Errorf("no v1 bucket")
	}
	b = b.Bucket([]byte("moby"))
	if b == nil {
		return nil, fmt.Errorf("no moby bucket")
	}
	b = b.Bucket([]byte("snapshots"))
	if b == nil {
		return nil, fmt.Errorf("no snapshots bucket")
	}
	b = b.Bucket([]byte("overlayfs"))
	if b == nil {
		return nil, fmt.Errorf("no overlayfs bucket")
	}
	return b, nil
}

type pair struct{ parent, child []byte }

func orphans(b *bolt.Bucket) (int, []pair, error) {
	real := map[string]struct{}{}
	if err := b.ForEach(func(k, v []byte) error {
		if v == nil {
			real[string(k)] = struct{}{}
		}
		return nil
	}); err != nil {
		return 0, nil, err
	}
	var drop []pair
	err := b.ForEach(func(k, v []byte) error {
		if v != nil {
			return nil
		}
		sb := b.Bucket(k)
		if sb == nil {
			return nil
		}
		ch := sb.Bucket([]byte("children"))
		if ch == nil {
			return nil
		}
		return ch.ForEach(func(ck, cv []byte) error {
			if ch.Bucket(ck) != nil {
				return fmt.Errorf("child %q of %q is a bucket", ck, k)
			}
			if _, ok := real[string(ck)]; ok {
				return nil
			}
			drop = append(drop, pair{append([]byte(nil), k...), append([]byte(nil), ck...)})
			return nil
		})
	})
	return len(drop), drop, err
}

func countOrphans(db *bolt.DB) (int, error) {
	var n int
	err := db.View(func(tx *bolt.Tx) error {
		b, err := overlay(tx)
		if err != nil {
			return err
		}
		n, _, err = orphans(b)
		return err
	})
	return n, err
}

func deleteOrphans(db *bolt.DB) (int, error) {
	var n int
	err := db.Update(func(tx *bolt.Tx) error {
		b, err := overlay(tx)
		if err != nil {
			return err
		}
		_, drop, err := orphans(b)
		if err != nil {
			return err
		}
		for _, d := range drop {
			ch := b.Bucket(d.parent).Bucket([]byte("children"))
			if err := ch.Delete(d.child); err != nil {
				return err
			}
			n++
		}
		return nil
	})
	return n, err
}
EOF
go build -o "$R/bin/clear-orphan-children" .
rm -rf /tmp/go /tmp/gopath /tmp/gocache /tmp/go1.27.3.linux-amd64.tar.gz
"$R/bin/clear-orphan-children" > "$R/out/8.10_usage.txt" 2>&1 || true
head -n 1 "$R/out/8.10_usage.txt"
```

- **Expected:** the free-space number is at least 2. `go1.27.3.linux-amd64.tar.gz: OK`. The last line is `usage: clear-orphan-children meta.db EXPECTED_COUNT`.
- **Result:** run 20 Oct 13:07 CST. **Pass** for the build. Free space `177`, `want 25442`, checksum `OK`, and the last line is `usage: clear-orphan-children meta.db EXPECTED_COUNT`. The stop-and-edit block is next.
- **If the checksum line is not `OK`:** stop. Do not stop Docker. Send the `sha256sum` line.

Then stop, edit, and start again:

```bash
cd "$R/out"
want=$(cat 8.9_orphan_count.txt)
date
systemctl stop docker.socket docker
systemctl stop containerd
date
systemctl is-active docker.socket docker containerd
systemctl show -p MainPID --value containerd
fuser -v /data/containerd/io.containerd.metadata.v1.bolt/meta.db 2>&1 || echo "no process has meta.db open"
cp -a /data/containerd/io.containerd.metadata.v1.bolt/meta.db 8.10_meta.db.before
sha256sum 8.10_meta.db.before | tee 8.10_meta_before.sha256
"$R/bin/clear-orphan-children" /data/containerd/io.containerd.metadata.v1.bolt/meta.db "$want" > 8.10_clear.txt 2>&1
rc=$?
cat 8.10_clear.txt
if [ "$rc" -eq 0 ]; then
  echo "EDIT APPLIED"
else
  cp -a 8.10_meta.db.before /data/containerd/io.containerd.metadata.v1.bolt/meta.db
  echo "RESTORED no edit kept"
fi
systemctl start containerd
systemctl start docker.socket
chgrp root /run/docker.sock
systemctl start docker
systemctl is-active docker.socket docker containerd
stat -c '%U:%G %a %n' /run/docker.sock
runuser -u toby -- docker version --format '{{.Server.Version}}' 2>&1 | tail -n 1
docker version --format '{{.Server.Version}}'
ctr -n moby images ls -q | wc -l
ctr -n moby images check > 8.10_images_check.txt
python3 - << 'PY'
rows = bad = 0
for line in open("8.10_images_check.txt"):
    if not line.strip() or line.startswith("REF"):
        continue
    rows += 1
    parts = line.split()
    if "complete" not in parts or parts[-1] != "true":
        bad += 1
        print(line[:200].rstrip())
print("images", rows)
print("bad", bad)
PY
```

- **Expected:** `inactive` three times, then pid `0`, then `no process has meta.db open`. The clear file prints `seen <the 8.9 count>`, then `deleted <same> remaining 0`, then the shell prints `EDIT APPLIED`. After the start: `active` three times, `root:root 660 /run/docker.sock`, `toby` gets `permission denied`, root gets `29.6.2`, then `523`, `images 523`, `bad 0`.
- **If the clear file prints `STOP`, or the shell prints `RESTORED`:** the copy was put back. Do not run 8.11. Send `8.10_clear.txt`.
- **If `bad` is not 0, or the image count is not 523:** stop Docker and containerd again, put the copy back, start them again, and stop the guide. **Skipped on this run.** The second block already printed `images 523` and `bad 0`, so the edit is the one to keep. Pasting the block below would put the old database back and undo step 8.10.

```bash
systemctl stop docker.socket docker
systemctl stop containerd
fuser -v /data/containerd/io.containerd.metadata.v1.bolt/meta.db 2>&1 || echo "no process has meta.db open"
cp -a "$R/out/8.10_meta.db.before" /data/containerd/io.containerd.metadata.v1.bolt/meta.db
echo RESTORED
systemctl start containerd
systemctl start docker.socket
chgrp root /run/docker.sock
systemctl start docker
```

- **Result of this third block:** skipped 20 Oct 13:12 CST. The second block's image check passed, so this restore must not be run.

- **Pass check:** `EDIT APPLIED`, `deleted` equals the 8.9 count, `remaining 0`, `images 523`, `bad 0`, and `toby` is refused. `mark 8.10 PASS`
- **Achieves:** no leftover child name in `moby`. Every Docker image still matches its files. Layer records that were blocked only by those names can now be removed.
- **Send me:** the `seen` / `deleted` lines, and the `images` / `bad` lines.
- **Result:** run 20 Oct 13:12:53 CST. **Pass.** Stop took the same second. `inactive` three times, pid `0`, `no process has meta.db open`. Copy checksum `000c75adb7e3cf2231a5f7c3c3300396774474a7f27ff447228bd04654ccbdca`. `seen 25442`, `deleted 25442 remaining 0`, `EDIT APPLIED`. After the start: `active` three times, `root:root 660 /run/docker.sock`, `toby` refused, root `29.6.2`, image count `523`, `images 523`, `bad 0`. Mark `mark 8.10 PASS`.

### 8.11 Remove folderless layer records that nothing still needs

- **Why:** with the leftover names gone, `ctr snapshots rm` can remove a layer record that has no real child. This step removes only committed records whose snapshot folder is already gone, and only when the record is not in the layer chain of any current Docker image and not in the chain of an active container. A later pull skips a layer while one of these records remains.
- **Risk:** low for the 523 images. Their chains are not in the list. An active container's chain is not in the list. A record that still has a real child is skipped and tried again after that child is gone.

```bash
cd "$R/out"
python3 - << 'PY'
import os, struct, subprocess
from collections import defaultdict
path = "/data/containerd/io.containerd.snapshotter.v1.overlayfs/metadata.db"
snapdir = "/data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots"
f = open(path, "rb")
def rd(off, n):
    f.seek(off); return f.read(n)
def u16(b, o): return struct.unpack_from("<H", b, o)[0]
def u32(b, o): return struct.unpack_from("<I", b, o)[0]
def u64(b, o): return struct.unpack_from("<Q", b, o)[0]
def read_page(pgid):
    b = rd(pgid * 4096, 4096)
    ov = u32(b, 12)
    if ov:
        b = rd(pgid * 4096, 4096 * (ov + 1))
    return b
def items(page):
    if u16(page, 8) & 0x01:
        acc = []
        for i in range(u16(page, 10)):
            acc.extend(items(read_page(u64(page, 16 + i * 16 + 8))))
        return acc
    acc = []
    for i in range(u16(page, 10)):
        base = 16 + i * 16
        flags, pos, ksize, vsize = u32(page, base), u32(page, base + 4), u32(page, base + 8), u32(page, base + 12)
        k = page[base + pos:base + pos + ksize]
        v = page[base + pos + ksize:base + pos + ksize + vsize]
        if flags & 0x01:
            acc.append((k, u64(v, 0), v[16:] if len(v) > 16 else None))
        else:
            acc.append((k, v))
    return acc
def fields(root, inline):
    page = inline if inline is not None else read_page(root)
    return {a: b for a, b in ((it[0], it[1]) for it in items(page) if len(it) == 2)}
def uvarint(b):
    x = s = 0
    for c in b:
        x |= (c & 0x7F) << s
        if c < 128:
            return x
        s += 7
b0, b1 = read_page(0), read_page(1)
meta = b0 if u64(b0, 64) >= u64(b1, 64) else b1
top = items(read_page(u64(meta, 32)))
v1 = items(read_page(top[0][1]))
recs = items(read_page([r for r in v1 if r[0] == b"snapshots"][0][1]))
missing = set()
for key, root, inline in recs:
    key_s = key.decode()
    if not key_s.startswith("moby/"):
        continue
    name = key_s.split("/", 2)[-1]
    sid = str(uvarint(fields(root, inline)[b"id"]))
    if not os.path.isdir(os.path.join(snapdir, sid)):
        missing.add(name)
parent, kind = {}, {}
for line in subprocess.check_output(["ctr", "-n", "moby", "snapshots", "ls"], text=True).splitlines()[1:]:
    p = line.split()
    if len(p) >= 2:
        parent[p[0]] = p[1]
        kind[p[0]] = p[-1]
usage = []
for img in subprocess.check_output(["ctr", "-n", "moby", "images", "ls", "-q"], text=True).splitlines():
    img = img.strip()
    if not img:
        continue
    lines = subprocess.check_output(["ctr", "-n", "moby", "images", "usage", img], text=True, stderr=subprocess.DEVNULL).splitlines()
    if len(lines) > 1:
        usage.append(lines[1].split()[0])
def ancestors(seeds):
    out, st = set(), list(seeds)
    while st:
        n = st.pop()
        if not n or n == "-" or n in out:
            continue
        out.add(n)
        if parent.get(n):
            st.append(parent[n])
    return out
active = [n for n, k in kind.items() if k == "Active"]
protected = ancestors(usage) | ancestors(active) | set(active)
kids = defaultdict(set)
for n, p in parent.items():
    if p and p != "-":
        kids[p].add(n)
removable = sorted(n for n in missing if kind.get(n) == "Committed" and n not in protected)
open("8.11_snapshots_to_remove.txt", "w").write("".join(n + "\n" for n in removable))
print("folderless", len(missing))
print("protected", len(protected))
print("removable", len(removable))
print("images", len(usage))
PY
n=$(wc -l < 8.11_snapshots_to_remove.txt)
echo "list $n"
if [ "$n" -eq 0 ]; then
  echo "nothing to remove"
else
  cp 8.11_snapshots_to_remove.txt 8.11_left.txt
  : > 8.11_snapshots_rm.txt
  for pass in 1 2 3 4 5 6 7 8; do
    [ -s 8.11_left.txt ] || break
    : > 8.11_next.txt
    while read -r k; do
      if ctr -n moby snapshots rm "$k" >> 8.11_snapshots_rm.txt 2>&1; then
        echo "$k" >> 8.11_snapshots_rm.txt
      else
        echo "$k" >> 8.11_next.txt
      fi
    done < 8.11_left.txt
    mv 8.11_next.txt 8.11_left.txt
    echo "pass $pass left $(wc -l < 8.11_left.txt)"
  done
  echo "left $(wc -l < 8.11_left.txt)"
fi
echo "images $(ctr -n moby images ls -q | wc -l)"
ctr -n moby images tag docker.io/library/busybox:1.36 repair.local/gc-trigger:1
time ctr -n moby images rm --sync repair.local/gc-trigger:1
ctr -n moby images check > 8.11_images_check.txt
python3 - << 'PY'
rows = bad = 0
for line in open("8.11_images_check.txt"):
    if not line.strip() or line.startswith("REF"):
        continue
    rows += 1
    parts = line.split()
    if "complete" not in parts or parts[-1] != "true":
        bad += 1
        print(line[:200].rstrip())
print("images", rows)
print("bad", bad)
PY
```

- **Expected:** `images 523` from the list builder. `removable` is the size of the list. Either `nothing to remove`, or eight `pass` lines ending in `left 0`. Then `images 523`, the trigger name twice, and again `images 523` and `bad 0`. Building the list reads every image, so this can take 10 to 25 minutes. The removals are a few minutes more.
- **Pass check:** `left 0` or `nothing to remove`; image count 523; `bad 0`. `mark 8.11 PASS`
- **If `left` is not 0:** do not mark. The names in `8.11_left.txt` still have a real child, so another pass of this same list will not remove them. Continue at 8.11b. Do not start step 9.
- **Achieves:** the folderless records that had no real child are gone. Records that still have a child wait for 8.11b.
- **Result:** run 20 Oct 13:23 CST. **Not a pass.** Do not mark. `folderless 526`, `protected 2984`, `removable 137`, `images 523`. Passes went 72, 48, 45, then `left 42` and stayed there through pass 8. Final `images 523`, `bad 0`. All 42 failures are `cannot remove snapshot with child`. Each of the 42 has one committed child. Those children are not in an image chain and not an active container layer; 8.11 did not remove them because they still have snapshot folders. Step 8.11b removes that unused child tree, then the 42.

### 8.11b Remove the unused children that keep the last 42 records

- **Why:** containerd will not remove a layer record while a real child still names it. The 42 that 8.11 left each have one committed child. Following those children reaches 64 records, 16 of which are also in the 42, so the whole unused tree is about 90 records. None of them is in a current Docker image or an active container: if one were, the 42 parents would have been protected and would not have been in the 8.11 list. Some of these children still have snapshot folders. Those folders are removed with the records. A later pull can then download the layer instead of skipping a folderless record.
- **Risk:** low for the 523 images. Their layer chains are checked again before anything is removed, and a hit stops the step with no removal. The folders that go are not used by those images or by an active container.

```bash
cd "$R/out"
python3 - << 'PY'
import subprocess
from collections import defaultdict
left = [n for n in open("8.11_left.txt").read().split() if n]
parent, kind = {}, {}
kids = defaultdict(list)
for line in subprocess.check_output(["ctr", "-n", "moby", "snapshots", "ls"], text=True).splitlines()[1:]:
    p = line.split()
    if len(p) >= 2:
        parent[p[0]] = p[1]
        kind[p[0]] = p[-1]
        if p[1] not in ("", "-"):
            kids[p[1]].append(p[0])
seen = set(left)
stack = list(left)
while stack:
    n = stack.pop()
    for c in kids.get(n, []):
        if c not in seen:
            seen.add(c)
            stack.append(c)
usage = []
for img in subprocess.check_output(["ctr", "-n", "moby", "images", "ls", "-q"], text=True).splitlines():
    img = img.strip()
    if not img:
        continue
    lines = subprocess.check_output(["ctr", "-n", "moby", "images", "usage", img], text=True, stderr=subprocess.DEVNULL).splitlines()
    if len(lines) > 1:
        usage.append(lines[1].split()[0])
def ancestors(seeds):
    out, st = set(), list(seeds)
    while st:
        n = st.pop()
        if not n or n == "-" or n in out:
            continue
        out.add(n)
        if parent.get(n):
            st.append(parent[n])
    return out
active = [n for n, k in kind.items() if k == "Active"]
protected = ancestors(usage) | ancestors(active) | set(active)
hit = sorted(seen & protected)
print("roots", len(left))
print("tree", len(seen))
print("protected_hit", len(hit))
if hit or len(left) != 42:
    print("STOP")
    open("8.11b_order.txt", "w").write("")
else:
    remaining = set(seen)
    order = []
    while remaining:
        leaves = sorted(n for n in remaining if not any(c in remaining for c in kids.get(n, [])))
        if not leaves:
            print("STOP cycle")
            order = []
            break
        order.extend(leaves)
        remaining -= set(leaves)
    open("8.11b_order.txt", "w").write("".join(n + "\n" for n in order))
    print("order", len(order))
PY
n=$(wc -l < 8.11b_order.txt)
echo "list $n"
if [ "$n" -lt 42 ] || [ "$n" -gt 150 ]; then
  echo "STOP count $n"
else
  : > 8.11b_snapshots_rm.txt
  : > 8.11b_left.txt
  ok=0; fail=0
  while read -r k; do
    if ctr -n moby snapshots rm "$k" >> 8.11b_snapshots_rm.txt 2>&1; then
      ok=$((ok + 1))
    else
      fail=$((fail + 1))
      echo "$k" >> 8.11b_left.txt
    fi
  done < 8.11b_order.txt
  echo "removed $ok failed $fail"
  if [ -s 8.11b_left.txt ]; then
    cp 8.11b_left.txt 8.11b_retry.txt
    for pass in 1 2 3 4 5 6 7 8 9 10 11 12; do
      [ -s 8.11b_retry.txt ] || break
      : > 8.11b_next.txt
      while read -r k; do
        if ctr -n moby snapshots rm "$k" >> 8.11b_snapshots_rm.txt 2>&1; then
          echo "$k" >> 8.11b_snapshots_rm.txt
        else
          echo "$k" >> 8.11b_next.txt
        fi
      done < 8.11b_retry.txt
      mv 8.11b_next.txt 8.11b_retry.txt
      echo "pass $pass left $(wc -l < 8.11b_retry.txt)"
    done
    echo "left $(wc -l < 8.11b_retry.txt)"
  else
    echo "left 0"
  fi
fi
echo "images $(ctr -n moby images ls -q | wc -l)"
ctr -n moby images check > 8.11b_images_check.txt
python3 - << 'PY'
rows = bad = 0
for line in open("8.11b_images_check.txt"):
    if not line.strip() or line.startswith("REF"):
        continue
    rows += 1
    parts = line.split()
    if "complete" not in parts or parts[-1] != "true":
        bad += 1
        print(line[:200].rstrip())
print("images", rows)
print("bad", bad)
PY
```

- **Expected:** `roots 42`, `tree` near 90, `protected_hit 0`, `order` the same as `tree`, `list` that number. Then `failed 0` and `left 0`. Then `images 523` and `bad 0`. Reading every image can take a few minutes. Stop if the line is `STOP` or `STOP count`.
- **Pass check:** `protected_hit 0`, `left 0`, `images 523`, `bad 0`. `mark 8.11b PASS`
- **If `left` is not 0:** do not mark. Send `8.11b_retry.txt` and the last 20 lines of `8.11b_snapshots_rm.txt`. Do not start step 9.
- **Achieves:** the 42 folderless records and the unused children that were holding them are gone. No current Docker image lost a layer.
- **Result:** run 20 Oct 13:36 CST. **Pass.** `roots 42`, `tree 90`, `protected_hit 0`, `order 90`, `list 90`, `removed 90 failed 0`, `left 0`, image count `523`, `images 523`, `bad 0`. Mark `mark 8.11b PASS`.

### 8.12 Match check before the pull

- **Why:** step 9 downloads the base images into this same store. It may start only when every Docker image still matches its files, no leftover child name remains, and no folderless layer record is left outside an image chain or an active container chain.
- **Risk:** none. It reads, and writes only into `$R/out`.

```bash
cd "$R/out"
ctr -n moby content active
ctr -n moby images ls -q | wc -l
ctr -n moby images check > 8.12_images_check.txt
python3 - << 'PY'
rows = bad = 0
for line in open("8.12_images_check.txt"):
    if not line.strip() or line.startswith("REF"):
        continue
    rows += 1
    parts = line.split()
    if "complete" not in parts or parts[-1] != "true":
        bad += 1
        print(line[:200].rstrip())
print("images", rows)
print("bad", bad)
PY
python3 - << 'PY'
import os, struct, subprocess
from collections import defaultdict
snapdir = "/data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots"
blobs = "/data/containerd/io.containerd.content.v1.content/blobs/sha256"
def load(path):
    f = open(path, "rb")
    def rd(off, n):
        f.seek(off); return f.read(n)
    def u16(b, o): return struct.unpack_from("<H", b, o)[0]
    def u32(b, o): return struct.unpack_from("<I", b, o)[0]
    def u64(b, o): return struct.unpack_from("<Q", b, o)[0]
    def read_page(pgid):
        b = rd(pgid * 4096, 4096)
        ov = u32(b, 12)
        if ov:
            b = rd(pgid * 4096, 4096 * (ov + 1))
        return b
    def items(page):
        if u16(page, 8) & 0x01:
            acc = []
            for i in range(u16(page, 10)):
                acc.extend(items(read_page(u64(page, 16 + i * 16 + 8))))
            return acc
        acc = []
        for i in range(u16(page, 10)):
            base = 16 + i * 16
            flags, pos, ksize, vsize = u32(page, base), u32(page, base + 4), u32(page, base + 8), u32(page, base + 12)
            k = page[base + pos:base + pos + ksize]
            v = page[base + pos + ksize:base + pos + ksize + vsize]
            if flags & 0x01:
                acc.append(("b", k, u64(v, 0), v[16:] if len(v) > 16 else None))
            else:
                acc.append(("k", k, v))
        return acc
    b0, b1 = read_page(0), read_page(1)
    meta = b0 if u64(b0, 64) >= u64(b1, 64) else b1
    return items, read_page, u64(meta, 32)
items, read_page, root = load("/data/containerd/io.containerd.metadata.v1.bolt/meta.db")
top = items(read_page(root))
v1 = items(read_page(top[0][2]))
moby = next(r for r in v1 if r[1] == b"moby")
mb = items(read_page(moby[2]))
sn = next(r for r in mb if r[1] == b"snapshots")
ov = items(read_page(next(r for r in items(read_page(sn[2])) if r[1] == b"overlayfs")[2]))
real, kids = set(), defaultdict(list)
for rec in ov:
    if rec[0] != "b":
        continue
    name = rec[1].decode()
    real.add(name)
    inner = items(rec[3] if rec[3] is not None else read_page(rec[2]))
    for it in inner:
        if it[0] == "b" and it[1] == b"children":
            for k in items(it[3] if it[3] is not None else read_page(it[2])):
                kids[name].append(k[1].decode())
orphan = parents = 0
for name, lst in kids.items():
    missing = [c for c in lst if c not in real]
    if missing:
        parents += 1
        orphan += len(missing)
print("orphan_child_keys", orphan)
print("parents_with_orphan", parents)
items, read_page, root = load("/data/containerd/io.containerd.snapshotter.v1.overlayfs/metadata.db")
def uvarint(b):
    x = s = 0
    for c in b:
        x |= (c & 0x7F) << s
        if c < 128:
            return x
        s += 7
def fields(rec):
    page = rec[3] if rec[3] is not None else read_page(rec[2])
    return {it[1]: it[2] for it in items(page) if it[0] == "k"}
top = items(read_page(root))
v1 = items(read_page(top[0][2]))
recs = items(read_page(next(r for r in v1 if r[1] == b"snapshots")[2]))
missing = set()
for rec in recs:
    if rec[0] != "b":
        continue
    key_s = rec[1].decode()
    if not key_s.startswith("moby/"):
        continue
    sid = str(uvarint(fields(rec)[b"id"]))
    if not os.path.isdir(os.path.join(snapdir, sid)):
        missing.add(key_s.split("/", 2)[-1])
parent, kind = {}, {}
for line in subprocess.check_output(["ctr", "-n", "moby", "snapshots", "ls"], text=True).splitlines()[1:]:
    p = line.split()
    if len(p) >= 2:
        parent[p[0]] = p[1]
        kind[p[0]] = p[-1]
usage = []
for img in subprocess.check_output(["ctr", "-n", "moby", "images", "ls", "-q"], text=True).splitlines():
    img = img.strip()
    if not img:
        continue
    lines = subprocess.check_output(["ctr", "-n", "moby", "images", "usage", img], text=True, stderr=subprocess.DEVNULL).splitlines()
    if len(lines) > 1:
        usage.append(lines[1].split()[0])
def ancestors(seeds):
    out, st = set(), list(seeds)
    while st:
        n = st.pop()
        if not n or n == "-" or n in out:
            continue
        out.add(n)
        if parent.get(n):
            st.append(parent[n])
    return out
active = [n for n, knd in kind.items() if knd == "Active"]
protected = ancestors(usage) | ancestors(active) | set(active)
print("folderless", len(missing))
print("folderless_unprotected", len([n for n in missing if n not in protected]))
digests = subprocess.check_output(["ctr", "-n", "moby", "content", "ls", "-q"], text=True).split()
print("moby_records_without_blob", sum(1 for d in digests if not os.path.exists(os.path.join(blobs, d.split(":", 1)[-1]))))
PY
```

- **Expected:** `content active` prints only its header. The image count is 523. `images 523` and `bad 0`. `orphan_child_keys 0`. `folderless_unprotected 0`. `moby_records_without_blob 0`. `folderless` may be above 0 when the only remaining folderless records belong to an active container. `store_check.sh` may still warn, because `moby_history` is untouched.
- **Pass check:** `bad 0`, `orphan_child_keys 0`, `folderless_unprotected 0`, `moby_records_without_blob 0`. `mark 8.12 PASS`
- **Achieves:** the gate for step 9. Every Docker image on the server matches its files, and no leftover record in `moby` can make a pull skip a missing file.
- **Send me:** the `images` / `bad` lines and the five count lines.
- **Result:** run 20 Oct 13:39 CST. **Not a pass.** Do not mark. `content active` printed only its header. Image count `523`, `images 523`, `bad 0`, `moby_records_without_blob 0`. `orphan_child_keys 2` on 2 parents. Both names are `…-init` children of real image layers, and no container still has those ids. `folderless_unprotected 42` is not a removal list: that part of this script misreads snapshot ids. Counted again with the step 8.11 reader: `folderless 389`, `folderless_unprotected 0`. Those 389 are in an image chain or an active container chain, which is the expected remainder. Step 8.12b clears only the 2 names.

### 8.12b Clear the two leftover init names

- **Why:** the two names are the same kind of leftover as step 8.10: a child key whose layer record is already gone. They sit on image layers that stay. No container uses those two ids. The program built in 8.10 deletes them only when it sees exactly 2, and it does not remove the image layers.
- **Risk:** **high** for running containers, for the same few seconds as step 8.10. Docker stops and starts again. The 523 images are not removed.

```bash
cd "$R/out"
date
systemctl stop docker.socket docker
systemctl stop containerd
date
systemctl is-active docker.socket docker containerd
systemctl show -p MainPID --value containerd
fuser -v /data/containerd/io.containerd.metadata.v1.bolt/meta.db 2>&1 || echo "no process has meta.db open"
cp -a /data/containerd/io.containerd.metadata.v1.bolt/meta.db 8.12b_meta.db.before
sha256sum 8.12b_meta.db.before | tee 8.12b_meta_before.sha256
"$R/bin/clear-orphan-children" /data/containerd/io.containerd.metadata.v1.bolt/meta.db 2 > 8.12b_clear.txt 2>&1
rc=$?
cat 8.12b_clear.txt
if [ "$rc" -eq 0 ]; then
  echo "EDIT APPLIED"
else
  cp -a 8.12b_meta.db.before /data/containerd/io.containerd.metadata.v1.bolt/meta.db
  echo "RESTORED no edit kept"
fi
systemctl start containerd
systemctl start docker.socket
chgrp root /run/docker.sock
systemctl start docker
systemctl is-active docker.socket docker containerd
stat -c '%U:%G %a %n' /run/docker.sock
runuser -u toby -- docker version --format '{{.Server.Version}}' 2>&1 | tail -n 1
docker version --format '{{.Server.Version}}'
ctr -n moby images check > 8.12b_images_check.txt
python3 - << 'PY'
rows = bad = 0
for line in open("8.12b_images_check.txt"):
    if not line.strip() or line.startswith("REF"):
        continue
    rows += 1
    parts = line.split()
    if "complete" not in parts or parts[-1] != "true":
        bad += 1
        print(line[:200].rstrip())
print("images", rows)
print("bad", bad)
PY
python3 - << 'PY'
import struct
from collections import defaultdict
path = "/data/containerd/io.containerd.metadata.v1.bolt/meta.db"
f = open(path, "rb")
def rd(off, n):
    f.seek(off); return f.read(n)
def u16(b, o): return struct.unpack_from("<H", b, o)[0]
def u32(b, o): return struct.unpack_from("<I", b, o)[0]
def u64(b, o): return struct.unpack_from("<Q", b, o)[0]
def read_page(pgid):
    b = rd(pgid * 4096, 4096)
    ov = u32(b, 12)
    if ov:
        b = rd(pgid * 4096, 4096 * (ov + 1))
    return b
def items(page):
    if u16(page, 8) & 0x01:
        acc = []
        for i in range(u16(page, 10)):
            acc.extend(items(read_page(u64(page, 16 + i * 16 + 8))))
        return acc
    acc = []
    for i in range(u16(page, 10)):
        base = 16 + i * 16
        flags, pos, ksize, vsize = u32(page, base), u32(page, base + 4), u32(page, base + 8), u32(page, base + 12)
        k = page[base + pos:base + pos + ksize]
        v = page[base + pos + ksize:base + pos + ksize + vsize]
        if flags & 0x01:
            acc.append(("b", k, u64(v, 0), v[16:] if len(v) > 16 else None))
        else:
            acc.append(("k", k, v))
    return acc
b0, b1 = read_page(0), read_page(1)
meta = b0 if u64(b0, 64) >= u64(b1, 64) else b1
top = items(read_page(u64(meta, 32)))
v1 = items(read_page(top[0][2]))
moby = next(r for r in v1 if r[1] == b"moby")
mb = items(read_page(moby[2]))
sn = next(r for r in mb if r[1] == b"snapshots")
ov = items(read_page(next(r for r in items(read_page(sn[2])) if r[1] == b"overlayfs")[2]))
real, kids = set(), defaultdict(list)
for rec in ov:
    if rec[0] != "b":
        continue
    name = rec[1].decode()
    real.add(name)
    inner = items(rec[3] if rec[3] is not None else read_page(rec[2]))
    for it in inner:
        if it[0] == "b" and it[1] == b"children":
            for k in items(it[3] if it[3] is not None else read_page(it[2])):
                kids[name].append(k[1].decode())
orphan = parents = 0
for name, lst in kids.items():
    missing = [c for c in lst if c not in real]
    if missing:
        parents += 1
        orphan += len(missing)
print("orphan_child_keys", orphan)
print("parents_with_orphan", parents)
PY
```

- **Expected:** `inactive` three times, pid `0`, `no process has meta.db open`. Then `seen 2`, `deleted 2 remaining 0`, `EDIT APPLIED`. After the start: `active` three times, `root:root 660 /run/docker.sock`, `toby` refused, root `29.6.2`, `images 523`, `bad 0`, `orphan_child_keys 0`.
- **If the shell prints `RESTORED` or `STOP`:** the new copy was put back. Do not start step 9. Send `8.12b_clear.txt`.
- **Pass check:** `EDIT APPLIED`, `deleted 2 remaining 0`, `images 523`, `bad 0`, `orphan_child_keys 0`. `mark 8.12b PASS`
- **Achieves:** no leftover child name in `moby`. Every Docker image still matches its files. This is the gate for step 9. The 389 folderless records that belong to an image or an active container stay.
- **Result:** run 20 Oct 13:52:03 CST. **Pass.** Stop took the same second. `inactive` three times, pid `0`, `no process has meta.db open`. Copy checksum `553e0f6d54f7d7b3430713c806d22a93f729ce274b6a5ec7bcec9c718cc8a255`. `seen 2`, `deleted 2 remaining 0`, `EDIT APPLIED`. After the start: `active` three times, `root:root 660 /run/docker.sock`, `toby` refused, root `29.6.2`, `images 523`, `bad 0`, `orphan_child_keys 0`, `parents_with_orphan 0`. Mark `mark 8.12b PASS`.

## Step 9: the 12 base images

Start step 9 only after 8.12b is PASS.

### 9.1 Pull

- **Why:** every benchmark is built on these. The list is the 8 failing bases of mentor report section 9.0, with the exact names this store had for them, including digest aliases. beacon (the project was named mac-k3d until the 20 Oct codebase migration) pins `alpine:3.19` by digest (`pipeline/lib/harbor_egress.py`). If Harbor is installed under `toby`, the `grep` adds Harbor's own pinned `alpine:3.23.4` digest.
- **Risk:** low. It downloads from Docker Hub and `ghcr.io`.

```bash
cd "$R/out"
cat > bases_pull.txt <<'EOF'
docker.io/library/alpine:3.19
docker.io/library/alpine:3.19@sha256:6baf43584bcb78f2e5847d1de515f23499913ac9f12bdf834811a3145eb11ca1
docker.io/library/alpine:3.20
docker.io/library/alpine:latest
docker.io/library/alpine:3.23.4
docker.io/library/ubuntu:22.04
docker.io/library/ubuntu:22.04@sha256:0e0a0fc6d18feda9db1590da249ac93e8d5abfea8f4c3c0c849ce512b5ef8982
docker.io/library/ubuntu:24.04
docker.io/library/ubuntu@sha256:08ea48a03a3e78ebc7cd526e6a275053223aadd88bfc09cc49b06d5281525fde
docker.io/library/debian:bookworm-slim
ghcr.io/astral-sh/uv:0.9.30-python3.13-bookworm
EOF
grep -rhoE 'alpine:3\.23\.4@sha256:[0-9a-f]{64}' /home/toby/.local/share/uv/tools 2>/dev/null | sort -u | sed 's#^#docker.io/library/#' >> bases_pull.txt
cat bases_pull.txt
while read -r i; do echo "== $i"; docker pull "$i"; done < bases_pull.txt > 9.1_pull.txt 2>&1
grep -E '^==|^Status|rror' 9.1_pull.txt
```

- **Expected:** 11 or 12 names. For each, `Status: Downloaded newer image for …`, or `Image is up to date` for an alias of an image pulled just before. No `Error`. The summary `grep` does not show `AlreadyExists`; the pull log does.
- **Pass check:** no `Error` and no `AlreadyExists`. A `not found` for one of the two digest aliases is an alias the registry no longer serves; note it and go on. `mark 9.1 PASS`
- **Result:** run 20 Oct 13:56 CST. **Not a pass.** Do not mark. Ten pulls printed `Status:`. `alpine:3.19`, `alpine:3.20`, `alpine:latest`, `ubuntu:22.04`, its digest, `ubuntu:24.04`, its digest, `debian:bookworm-slim` and `uv` downloaded. The `alpine:3.19` digest was already up to date. `alpine:3.23.4` and `alpine:3.23.4@sha256:5b10f432ef3da1b8d4c7eb6c487f2f5a8f096bc91145e68878dd4a5019afde11` both failed with `unable to prepare extraction snapshot: AlreadyExists: target snapshot "sha256:29df493baa13de438d6d2ece3a8333032e0b7b9b9d8cce4ee82194da255f61e1": already exists`. The blobs downloaded. The unpack did not. `ctr snapshots ls` does not show that snapshot, because its folder is gone, but the record is still in containerd's database. It has a tree of 37 records. None of them is a current image's top layer, and no container id matches the container-shaped names in that tree. Step 9.1a removes that tree and retries the two pulls.

### 9.1a Remove the hidden snapshot that blocked alpine 3.23.4, then pull it again

- **Why:** containerd refuses to unpack `alpine:3.23.4` while snapshot `sha256:29df493baa13de438d6d2ece3a8333032e0b7b9b9d8cce4ee82194da255f61e1` is still a record. That record's folder is gone, so `ctr snapshots ls` skips it. Under it is a closed tree of 37 records, children included. No current image uses one as its top layer, and no remaining container has one of those names.
- **Risk:** low for the images pulled in 9.1 and for the 523 that were already here. The step removes nothing if any name in the tree is an image's top layer. It removes the 37 records, then pulls only the two names that failed.

```bash
cd "$R/out"
python3 - << 'PY'
import struct, subprocess
from collections import defaultdict
target = "sha256:29df493baa13de438d6d2ece3a8333032e0b7b9b9d8cce4ee82194da255f61e1"
path = "/data/containerd/io.containerd.metadata.v1.bolt/meta.db"
f = open(path, "rb")
def rd(off, n):
    f.seek(off); return f.read(n)
def u16(b, o): return struct.unpack_from("<H", b, o)[0]
def u32(b, o): return struct.unpack_from("<I", b, o)[0]
def u64(b, o): return struct.unpack_from("<Q", b, o)[0]
def read_page(pgid):
    b = rd(pgid * 4096, 4096)
    ov = u32(b, 12)
    if ov:
        b = rd(pgid * 4096, 4096 * (ov + 1))
    return b
def items(page):
    if u16(page, 8) & 0x01:
        acc = []
        for i in range(u16(page, 10)):
            acc.extend(items(read_page(u64(page, 16 + i * 16 + 8))))
        return acc
    acc = []
    for i in range(u16(page, 10)):
        base = 16 + i * 16
        flags, pos, ksize, vsize = u32(page, base), u32(page, base + 4), u32(page, base + 8), u32(page, base + 12)
        k = page[base + pos:base + pos + ksize]
        v = page[base + pos + ksize:base + pos + ksize + vsize]
        if flags & 0x01:
            acc.append(("b", k, u64(v, 0), v[16:] if len(v) > 16 else None))
        else:
            acc.append(("k", k, v))
    return acc
b0, b1 = read_page(0), read_page(1)
meta = b0 if u64(b0, 64) >= u64(b1, 64) else b1
top = items(read_page(u64(meta, 32)))
v1 = items(read_page(top[0][2]))
moby = next(r for r in v1 if r[1] == b"moby")
mb = items(read_page(moby[2]))
sn = next(r for r in mb if r[1] == b"snapshots")
ov = items(read_page(next(r for r in items(read_page(sn[2])) if r[1] == b"overlayfs")[2]))
kids = defaultdict(list)
real = set()
for rec in ov:
    if rec[0] != "b":
        continue
    name = rec[1].decode()
    real.add(name)
    inner = items(rec[3] if rec[3] is not None else read_page(rec[2]))
    for it in inner:
        if it[0] == "b" and it[1] == b"children":
            for k in items(it[3] if it[3] is not None else read_page(it[2])):
                kids[name].append(k[1].decode())
seen = []
stack = [target]
while stack:
    n = stack.pop()
    if n in seen:
        continue
    seen.append(n)
    stack.extend(kids.get(n, []))
usage = set()
for img in subprocess.check_output(["ctr", "-n", "moby", "images", "ls", "-q"], text=True).split():
    lines = subprocess.check_output(["ctr", "-n", "moby", "images", "usage", img], text=True, stderr=subprocess.DEVNULL).splitlines()
    if len(lines) > 1:
        usage.add(lines[1].split()[0])
hit = sorted(set(seen) & usage)
print("tree", len(seen))
print("protected_hit", len(hit))
if target not in real or len(seen) != 37 or hit:
    print("STOP")
    open("9.1a_order.txt", "w").write("")
else:
    remaining = set(seen)
    order = []
    while remaining:
        leaves = sorted(n for n in remaining if not any(c in remaining for c in kids.get(n, [])))
        if not leaves:
            print("STOP cycle")
            order = []
            break
        order.extend(leaves)
        remaining -= set(leaves)
    open("9.1a_order.txt", "w").write("".join(n + "\n" for n in order))
    print("order", len(order))
PY
n=$(wc -l < 9.1a_order.txt)
echo "list $n"
if [ "$n" -ne 37 ]; then
  echo "STOP count $n"
else
  : > 9.1a_snapshots_rm.txt
  : > 9.1a_left.txt
  ok=0; fail=0
  while read -r k; do
    if ctr -n moby snapshots rm "$k" >> 9.1a_snapshots_rm.txt 2>&1; then
      ok=$((ok + 1))
    else
      fail=$((fail + 1))
      echo "$k" >> 9.1a_left.txt
    fi
  done < 9.1a_order.txt
  echo "removed $ok failed $fail"
  if [ -s 9.1a_left.txt ]; then
    cp 9.1a_left.txt 9.1a_retry.txt
    for pass in 1 2 3 4 5 6 7 8; do
      [ -s 9.1a_retry.txt ] || break
      : > 9.1a_next.txt
      while read -r k; do
        if ctr -n moby snapshots rm "$k" >> 9.1a_snapshots_rm.txt 2>&1; then
          :
        else
          echo "$k" >> 9.1a_next.txt
        fi
      done < 9.1a_retry.txt
      mv 9.1a_next.txt 9.1a_retry.txt
      echo "pass $pass left $(wc -l < 9.1a_retry.txt)"
    done
    echo "left $(wc -l < 9.1a_retry.txt)"
  else
    echo "left 0"
  fi
fi
```

The removal must finish with `left 0` before the two pulls. If the block printed `STOP` or `left` is not 0, do not paste the pull. If it printed `left 0`, paste this:

```bash
cd "$R/out"
for i in docker.io/library/alpine:3.23.4 docker.io/library/alpine:3.23.4@sha256:5b10f432ef3da1b8d4c7eb6c487f2f5a8f096bc91145e68878dd4a5019afde11; do
  echo "== $i"
  docker pull "$i"
done > 9.1a_pull.txt 2>&1
grep -E '^==|^Status|rror|AlreadyExists' 9.1a_pull.txt
```

- **Expected:** `tree 37`, `protected_hit 0`, `order 37`, `removed 37 failed 0`, `left 0`. Then both pulls print a `Status:` line and neither prints `AlreadyExists`.
- **Pass check:** `left 0`, and both pull lines have `Status:`. `mark 9.1a PASS`
- **If the first block prints `STOP` or `left` is not 0:** do not pull. Send the count lines and the last 20 lines of `9.1a_snapshots_rm.txt`.
- **Result of the removal:** run 20 Oct 14:05 CST. **Pass** for the removal. `tree 37`, `protected_hit 0`, `order 37`, `list 37`, `removed 37 failed 0`, `left 0`.
- **Result of the pulls:** run 20 Oct 14:06 CST. **Pass.** `alpine:3.23.4` printed `Status: Downloaded newer image`. The digest printed `Status: Image is up to date`. No `AlreadyExists`. Mark `mark 9.1a PASS`.
- **Achieves:** the stale `alpine:3.23.4` layer record is gone, and both names of that image are pulled.

### 9.2 Test

- **Why:** the first test of the method: these pulls ran on a store with no stale records. Includes the four bases that worked throughout.
- **Risk:** low.

```bash
cd "$R/out"
{ cat bases_pull.txt; printf '%s\n' docker.io/library/alpine:3.21 docker.io/library/alpine:3.22 docker.io/library/busybox:1.36 docker.io/library/debian:bookworm; } > bases_test.txt
bash "$R/bin/start_test.sh" bases_test.txt 9.2_bases_test.tsv
cut -f1,2 9.2_bases_test.tsv | sort
```

- **Expected:** `ok` on every line, 15 or 16 lines.
- **Pass check:** every line `ok`. `mark 9.2 PASS`
- **Achieves:** the 12 bases start, including Harbor's probe image (build #53), beacon's `alpine:3.20` (build #61; the project was named mac-k3d until the 20 Oct codebase migration) and the `uv` build image.
- **Result:** run 20 Oct 14:34 CST. **Pass.** 16 lines, every one `ok`, including `alpine:3.23.4`, both Ubuntu digests, `debian:bookworm-slim` and `uv`. Marked `9.2 PASS` at 14:37:30.

## Step 10: published DeepSWE (113)

### 10.1 Load (15 to 40 minutes)

- **Why:** the 113 come back from the tar file of 7.2. The load writes new download records and unpacks new layer folders.
- **Risk:** low. It adds images.

```bash
cd "$R/out"
df -h /data | tail -n 1
time docker load -i /data/cloud-worker-repair/saved/deepswe-published.tar > 10.1_load.txt 2>&1
grep -c '^Loaded image' 10.1_load.txt
grep -v '^Loaded image' 10.1_load.txt | head
```

- **Expected:** free space; the time; `113`; nothing else.
- **Only if the load fails:** pull them instead (about 24 GB from `public.ecr.aws`):

```bash
cd "$R/out"
xargs -a save_deepswe.txt -d '\n' -n 1 docker pull > 10.1_pull.txt 2>&1
grep -c '^Status' 10.1_pull.txt; grep -i error 10.1_pull.txt | head
```

- **Pass check:** `113` loaded, or `113` pulled with no error. `mark 10.1 PASS`
- **Result:** run 20 Oct 14:47 CST. **Pass.** `/data` had 801G free. Load took 8m44.637s. `grep -c` printed `113`. The second `grep` printed nothing. The pull fallback was not used. Mark `mark 10.1 PASS`.

### 10.2 Test

- **Why:** the second test of the method: these images failed only because of stale layer records.
- **Risk:** low.

```bash
cd "$R/out"
bash "$R/bin/start_test.sh" save_deepswe.txt 10.2_deepswe_test.tsv
cut -f1 10.2_deepswe_test.tsv | sort | uniq -c
```

- **Expected:** `113 ok`.
- **Pass check:** `113 ok`. A `layers_missing` here means a stale record survived step 8: stop and send me the lines. `mark 10.2 PASS`
- **Result:** run 20 Oct 14:57 CST. **Not a pass.** Do not mark. `113 layers_missing`. 95 failed while mounting, 18 failed with `failed to stat parent` on 16 missing snapshot folders. The records are from 10 Sep 2026, so the load reused them and did not unpack. One checked layer file is on disk (`sha256:7f4ab249…`, 3.4 MB). The 113 images use 464 snapshot names, 142 of them with no folder. No other image uses those names. Six exited containers from 8 weeks ago still name six of these images (`RWLayer … is unexpectedly nil` in step 6.2, so they were left in the review list). Continue at 10.2a.

### 10.2a Remove the stale DeepSWE layer records

- **Why:** a load skips unpack when the layer record already exists, even if its folder is gone. These records stayed because the six nil-layer containers still named the images. No current image and no live container uses this chain. The saved tar is still at `/data/cloud-worker-repair/saved/deepswe-published.tar`.
- **Risk:** medium. It removes 6 exited containers and 1,226 snapshot records that belong only to this DeepSWE chain, including leaked container layers whose containers are already gone. It removes nothing if another image or a live container is in the list. It does not remove the 113 image names.

```bash
cd "$R/out"
python3 - << 'PY'
import subprocess, sys
from collections import defaultdict
deepswe=set(l.strip() for l in open("save_deepswe.txt") if l.strip())
images=subprocess.check_output(["ctr","-n","moby","images","ls","-q"], text=True).split()
def usage_names(img):
    lines=subprocess.check_output(["ctr","-n","moby","images","usage",img], text=True, stderr=subprocess.DEVNULL).splitlines()[1:]
    return [ln.split()[0] for ln in lines if ln.strip()]
chain=set(); other=set(); deep_n=0
for img in images:
    names=set(usage_names(img))
    if img in deepswe or img.startswith("public.ecr.aws/d3j8x8q7/swe-bench-202605:"):
        chain |= names
        deep_n += 1
    else:
        other |= names
lines=subprocess.check_output(["ctr","-n","moby","snapshots","ls"], text=True).splitlines()[1:]
parent={}; kids=defaultdict(set)
for line in lines:
    p=line.split()
    if len(p) < 2:
        continue
    parent[p[0]]=p[1]
    if p[1] and p[1] not in ("-", "<none>"):
        kids[p[1]].add(p[0])
remove=set(chain)
changed=True
while changed:
    changed=False
    for n in list(remove):
        for c in kids.get(n, ()):
            if c not in remove:
                remove.add(c)
                changed=True
ids=set(subprocess.check_output(["docker","ps","-aq","--no-trunc"], text=True).split())
def ancestors(seeds):
    out=set(); st=list(seeds)
    while st:
        n=st.pop()
        if not n or n in ("-", "<none>") or n in out:
            continue
        out.add(n)
        if parent.get(n):
            st.append(parent[n])
    return out
live=[c for c in ids if c in parent]
cons=[]
for line in subprocess.check_output(["docker","ps","-a","--no-trunc","--format","{{.ID}}\t{{.Status}}\t{{.Image}}"], text=True).splitlines():
    cid, status, image = line.split("\t", 2)
    if image.startswith("public.ecr.aws/d3j8x8q7/swe-bench-202605:") and status.startswith("Exited"):
        cons.append(cid)
overlap_other=len(remove & other)
overlap_live=len(remove & ancestors(live))
print("images_deepswe", deep_n)
print("chain", len(chain))
print("remove", len(remove))
print("overlap_other", overlap_other)
print("overlap_live", overlap_live)
print("containers", len(cons))
ok = (deep_n == 113 and len(chain) == 464 and len(remove) == 1226
      and overlap_other == 0 and overlap_live == 0 and len(cons) == 6)
if not ok:
    print("STOP")
    sys.exit(2)
left=set(remove)
order=[]
while left:
    leaves=sorted(n for n in left if not (kids.get(n, set()) & left))
    if not leaves:
        print("STOP")
        sys.exit(2)
    order.extend(leaves)
    left.difference_update(leaves)
open("10.2a_containers.txt","w").write("".join(c+"\n" for c in cons))
open("10.2a_snapshots_to_remove.txt","w").write("".join(n+"\n" for n in order))
print("READY", len(order))
PY
if [ $? -ne 0 ]; then
  echo "stopped before any removal"
else
  docker rm $(cat 10.2a_containers.txt) | tee 10.2a_containers_rm.txt
  : > 10.2a_snapshots_rm.txt
  while read -r k; do
    if ctr -n moby snapshots rm "$k" >> 10.2a_snapshots_rm.txt 2>&1; then
      echo "ok $k" >> 10.2a_snapshots_rm.txt
    else
      echo "FAIL $k" | tee -a 10.2a_snapshots_rm.txt
      break
    fi
  done < 10.2a_snapshots_to_remove.txt
  echo "containers_removed $(wc -l < 10.2a_containers_rm.txt)"
  echo "snapshot_ok $(grep -c '^ok ' 10.2a_snapshots_rm.txt)"
  echo "snapshot_fail $(grep -c '^FAIL ' 10.2a_snapshots_rm.txt || true)"
  echo "images $(docker images --format '{{.Repository}}:{{.Tag}}' | grep -c '^public.ecr.aws/d3j8x8q7/swe-bench-202605:')"
fi
```

- **Expected:** `images_deepswe 113`, `chain 464`, `remove 1226`, `overlap_other 0`, `overlap_live 0`, `containers 6`, `READY 1226`. Then 6 container ids, `containers_removed 6`, `snapshot_ok 1226`, `snapshot_fail 0`, `images 113`.
- **Pass check:** those counts. `STOP` or `snapshot_fail` above 0 means nothing further: send the output. `mark 10.2a PASS` only when `snapshot_ok 1226` and `snapshot_fail 0`.
- **Result:** run 20 Oct 15:11 CST. **Pass.** `images_deepswe 113`, `chain 464`, `remove 1226`, `overlap_other 0`, `overlap_live 0`, `containers 6`, `READY 1226`. The six container ids were removed. `containers_removed 6`, `snapshot_ok 1226`, `snapshot_fail 0`, `images 113`. Marked `10.2a PASS` at 15:15:18.

### 10.2b Load again (15 to 40 minutes)

- **Why:** the image names are still there, and the stale layer records are gone. Loading the same tar unpacks new layer folders.
- **Risk:** low. It writes the DeepSWE layers again. The tar is not deleted.

```bash
cd "$R/out"
df -h /data | tail -n 1
time docker load -i /data/cloud-worker-repair/saved/deepswe-published.tar > 10.2b_load.txt 2>&1
grep -c '^Loaded image' 10.2b_load.txt
grep -v '^Loaded image' 10.2b_load.txt | head
```

- **Expected:** free space; the time; `113`; nothing else.
- **Pass check:** `113`, and the second `grep` prints nothing. `mark 10.2b PASS`
- **Result:** run 20 Oct 15:33 CST. **Pass.** `/data` had 832G free. Load took 13m39.795s. `grep -c` printed `113`. The second `grep` printed nothing. Marked `10.2b PASS` at 15:38:56.

### 10.2c Test again

- **Why:** the same start test as 10.2, after the unpack.
- **Risk:** low.

```bash
cd "$R/out"
bash "$R/bin/start_test.sh" save_deepswe.txt 10.2c_deepswe_test.tsv
cut -f1 10.2c_deepswe_test.tsv | sort | uniq -c
```

- **Expected:** `113 ok`.
- **Pass check:** `113 ok`. Any `layers_missing` line: stop and send it. `mark 10.2c PASS`
- **Result:** run 20 Oct 15:39 CST. **Pass.** `113 ok`. Marked `10.2c PASS` at 15:48:26.

## Step 11: the other saved images

### 11.1 Load (10 to 30 minutes)

- **Why:** the owners' images that could still be saved come back as they were.
- **Risk:** low.

```bash
cd "$R/out"
time docker load -i /data/cloud-worker-repair/saved/other-local.tar > 11.1_load.txt 2>&1
grep -c '^Loaded image' 11.1_load.txt; wc -l < save_other.txt
grep -v '^Loaded image' 11.1_load.txt | head
```

With the fallback of 7.3, load the single files instead:

```bash
cd "$R/out"
for t in /data/cloud-worker-repair/saved/one/*.tar; do docker load -i "$t"; done > 11.1_load.txt 2>&1
grep -c '^Loaded image' 11.1_load.txt; grep -vc FAILED 7.3_save_one.tsv
```

- **Expected:** the two numbers are equal.
- **Pass check:** equal. `mark 11.1 PASS`
- **Result:** run 20 Oct 16:18 CST. **Pass.** The load kept running in tmux after the local connection dropped. It finished in 29m17.150s. Both counts printed `178`. The last `grep` printed nothing. One of those lines is `Loaded image ID: sha256:60c4d5da…`, not a repository name. The single-file fallback was not used. Marked `11.1 PASS` at 16:21:52.

### 11.2 Test

- **Risk:** low.

```bash
cd "$R/out"
bash "$R/bin/start_test.sh" save_other.txt 11.2_other_test.tsv
cut -f1 11.2_other_test.tsv | sort | uniq -c
```

- **Expected:** every line `ok`.
- **Pass check:** every line `ok`. `mark 11.2 PASS`
- **Result:** run 20 Oct 16:22 CST. **Not a pass.** Do not mark. `177 ok` and `1 other`. The other line is `docker.io/misioslav/expressvpn@sha256:60c4d5da7d92c9cf2ae23677fa624ca841436be55d931ac552d91a23fccd991c`: `No such image`. The load stored that image with no repository name, id `sha256:60c4d5da…`, 166,594,810 bytes, `RepoTags` and `RepoDigests` empty. The other 177 names started. Step 11.2a starts this image by its id.

### 11.2a Start the untagged image

- **Why:** the layers were loaded. Docker did not keep the `misioslav/expressvpn@…` name, so the start test looked up a name that is not in the image list.
- **Risk:** low. One container, removed when it exits.

```bash
cd "$R/out"
timeout 120 docker run --rm --pull never --network none --entrypoint true sha256:60c4d5da7d92c9cf2ae23677fa624ca841436be55d931ac552d91a23fccd991c > 11.2a_expressvpn.txt 2>&1
echo "rc $?"
cat 11.2a_expressvpn.txt
```

- **Expected:** `rc 0`, and no error text.
- **Pass check:** `rc 0`. `mark 11.2a PASS`
- **Result:** run 20 Oct 16:27 CST. **Not a pass.** Do not mark. `rc 127`. `failed to stat parent` for snapshot folder `159017`. That record and two below it are from 10 Sep 2026 and have no folders. No other image uses them. Their layer files are on disk. One old container snapshot from the same day still sits on top, and that container is gone. The base layer `sha256:3edb2192…` still has its folder and stays. Step 11.2b removes the five stale records, then starts the image id again.

### 11.2b Remove the stale expressvpn records and start again

- **Why:** the load skipped unpack because these three layer records already existed. Removing them lets the next start unpack from the files already on disk. The base layer stays.
- **Risk:** low. Five records, none used by a current image or a live container. The step stops before deleting if the parent links are not the ones recorded here.

```bash
cd "$R/out"
python3 - << 'PY'
import subprocess, sys
from collections import defaultdict
order = [
 "ce173396e779bad2be95d2c2b15e41f50f7d63a44b704fcc42c70814d9c125be",
 "ce173396e779bad2be95d2c2b15e41f50f7d63a44b704fcc42c70814d9c125be-init",
 "sha256:7272790f7fe34d9081a4801ea2efa088a9fb5191939df5b4567cbcf5d18f061e",
 "sha256:4494538f68e10a23528b873e00a5c080a4e111926db906c13278bae06a3b92ee",
 "sha256:0c9320aa2282b493d04d7b324e083336df131794419d67a164f6e906c317bc78",
]
keep = "sha256:3edb2192497af6e965b9b7e57dc6dbdce1f3ea721d14a98110419d4ded523298"
lines=subprocess.check_output(["ctr","-n","moby","snapshots","ls"], text=True).splitlines()[1:]
parent={}; kids=defaultdict(set)
for line in lines:
    p=line.split()
    if len(p) < 2:
        continue
    parent[p[0]]=p[1]
    if p[1] not in ("-", "<none>"):
        kids[p[1]].add(p[0])
ids=set(subprocess.check_output(["docker","ps","-aq","--no-trunc"], text=True).split())
expect = {
    order[0]: order[1],
    order[1]: order[2],
    order[2]: order[3],
    order[3]: order[4],
    order[4]: keep,
}
if keep not in parent or any(n in ids for n in order):
    print("STOP")
    sys.exit(2)
for n, p in expect.items():
    if parent.get(n) != p:
        print("STOP")
        sys.exit(2)
if kids.get(order[0]):
    print("STOP")
    sys.exit(2)
for i, n in enumerate(order[1:], start=1):
    if kids[n] - {order[i-1]}:
        print("STOP")
        sys.exit(2)
open("11.2b_snapshots_to_remove.txt","w").write("".join(n+"\n" for n in order))
print("READY", len(order))
PY
if [ $? -ne 0 ]; then
  echo "stopped before any removal"
else
  : > 11.2b_snapshots_rm.txt
  while read -r k; do
    if ctr -n moby snapshots rm "$k" >> 11.2b_snapshots_rm.txt 2>&1; then
      echo "ok $k" >> 11.2b_snapshots_rm.txt
    else
      echo "FAIL $k" | tee -a 11.2b_snapshots_rm.txt
      break
    fi
  done < 11.2b_snapshots_to_remove.txt
  echo "snapshot_ok $(grep -c '^ok ' 11.2b_snapshots_rm.txt)"
  echo "snapshot_fail $(grep -c '^FAIL ' 11.2b_snapshots_rm.txt || true)"
  timeout 120 docker run --rm --pull never --network none --entrypoint true sha256:60c4d5da7d92c9cf2ae23677fa624ca841436be55d931ac552d91a23fccd991c > 11.2b_expressvpn.txt 2>&1
  echo "rc $?"
  cat 11.2b_expressvpn.txt
fi
```

- **Expected:** `READY 5`, `snapshot_ok 5`, `snapshot_fail 0`, `rc 0`, and no error text.
- **Pass check:** those counts and `rc 0`. `STOP` or `snapshot_fail` above 0 means nothing further: send the output. `mark 11.2b PASS`
- **Result:** run 20 Oct 16:31 CST. **Pass.** `READY 5`, `snapshot_ok 5`, `snapshot_fail 0`, `rc 0`. The error file was empty. Mark `mark 11.2b PASS`. Step 11.2 stays unmarked: the old `misioslav/expressvpn@sha256:60c4d5da…` name was not restored, and the image starts by its id.

## Step 12: final check of the repaired store

### 12.1 Start test of every image (10 to 20 minutes)

- **Risk:** low.

```bash
cd "$R/out"
ctr -n moby images ls -q > 12.1_images.txt
wc -l < 12.1_images.txt
time nice bash "$R/bin/start_test.sh" 12.1_images.txt 12.1_start_test.tsv
cut -f1 12.1_start_test.tsv | sort | uniq -c
```

- **Expected:** about 520 names or more (the kept ones, the bases, DeepSWE and the other saved images), every one `ok`.
- **Pass check:** every line `ok`. `mark 12.1 PASS`
- **Result:** run 20 Oct 16:39 CST. **Pass.** `932` names, `932 ok`, in 4m20.778s. Mark `mark 12.1 PASS`.

### 12.2 Store check

- **Risk:** low.

```bash
bash "$R/bin/store_check.sh" docker.io/library/alpine:3.21 docker.io/library/busybox:1.36 docker.io/library/alpine:3.20 docker.io/library/ubuntu:24.04 2>&1 | tee "$R/out/12.2_store_check_after.txt"
```

- **Expected:** `open_meta.db` the same pair; `records_without_blob 0`; `snapshot_records` not more than `snapshot_folders`; four canaries `ok`; `RESULT OK`, or `WARN` only for disk space.
- **Pass check:** all of these. `mark 12.2 PASS`
- **Result:** run 20 Oct 16:42 CST. **Not a pass.** Do not mark. `open_meta.db` is `66309:37131376` both open and on disk. `open_metadata.db` is `66309:37094252` both open and on disk. `blobs_without_record 0`. Images `932`. All four canaries `ok`. `RESULT WARN` for three lines:
  - `records_without_blob 52585`. This is `moby_history` and the recovery namespaces, the same leftover as step 8. `moby` images are not in that count. It stays.
  - `/data` is 96% full. The check warns at 85%. `/` is 65%.
  - `snapshot_records 3923` and `snapshot_folders 2710`, a gap of 1,213. `ctr` lists 2,566 snapshots in `moby` and 1,357 in `capbench-recovery-751ae692…`. That recovery namespace has a folder for every one of its 1,357 records. The gap is all in `moby`: 1,353 of its listed snapshots have folders, and 2,441 of the 2,566 are in a current image chain. Those images started in step 12.1. 125 listed snapshots are not used by any image. The snapshotter's own database has 45 `moby` records with no folder. Do not delete the image-chain records to make this number match.
- **Recount 20 Oct 17:02 CST.** Docker's own namespace, `moby`: 4,054 download records, 0 without a file. Blob files on disk: 4,151, and 0 of them lack a record. Snapshot folders: 2,710, and 0 of them lack a record. The 45 folderless snapshot records are all in `moby`, none is in a current image chain, and none is a live container. `moby_history` has 52,340 download records with no file. The lolbench recovery namespaces add the rest of the 52,585. Those two groups are not Docker images. They stay.

### 12.3 Before and after

- **Why:** the numbers for the report, and the list of containers that were running before 3.1 and are not running now, for their owners.
- **Risk:** none.

```bash
cd "$R/out"
k='^(content_records|blob_files|records_without_blob|blobs_without_record|snapshot_records|snapshot_folders|images|leases|disk_used)'
paste <(grep -E "$k" 2.3_store_check_before.txt) <(grep -E "$k" 12.2_store_check_after.txt | awk '{print $NF}') | tee 12.3_before_after.txt
comm -23 <(sort 2.4_running_before.txt) <(docker ps -q --no-trunc | sort) > 12.3_not_running_now.txt
grep -F -f 12.3_not_running_now.txt 2.4_containers_before.tsv | cut -f2,4 | tee 12.3_not_running_now_names.txt
```

- **Expected:** one line per number, with the value before and after. Then the names and images of containers that were running before and aren't now: ones without a restart policy, and any removed in 8.1.
- **Pass check:** recorded. `mark 12.3 PASS`
- **Send me:** the output.
- **Result:** run 20 Oct 16:46 CST. **Pass.** Before, then after: content records 82,010 then 56,736; blob files 4,278 then 4,151; records without a blob 77,732 then 52,585; blobs without a record 0 then 0; snapshot records 21,183 then 3,923; snapshot folders 10,409 then 2,710; images 8,057 then 932; leases 20,944 then 10,570; `/` 66% then 65%; `/data` 98% then 96%. Twelve containers that were running before are not running now: four `jiuwen*` dind containers on `swe-marathon-cpu/daemon-bootstrap:20261012-r3-v2`, `remain178-one-image-probe`, and seven `dgm-container-20261013_*` containers. Mark `mark 12.3 PASS`.

## Step 13: guards against a repeat

Steps 4 and 5 already removed the second store and made its folder immutable. This step adds the two guards that are only commands: a daily check, and a safe way to mount `/data` by hand.

### 13.1 The daily store check

- **Why:** the damage went unnoticed for a month. The same check that gated this repair now runs every morning as root. It writes a status file that everyone can read, and a journal error when something is wrong.
- **Risk:** low. Four new files and a timer. The check only reads, and starts three canary images with `--rm`. Undo: `systemctl disable --now store-check.timer`, then remove the four files.

```bash
install -m 755 -o root -g root "$R/bin/store_check.sh" /usr/local/sbin/store-check
cat > /usr/local/sbin/store-check-daily <<'EOF'
#!/bin/bash
# Daily run of store-check. Status for everyone: /var/lib/store-check/last.txt. History: /var/log/store-check.log.
install -d -m 755 /var/lib/store-check
/usr/local/sbin/store-check docker.io/library/alpine:3.21 docker.io/library/busybox:1.36 docker.io/library/alpine:3.20 > /var/lib/store-check/last.txt 2>&1
rc=$?
chmod 644 /var/lib/store-check/last.txt
cat /var/lib/store-check/last.txt >> /var/log/store-check.log
[ "$rc" -eq 0 ] || logger -p daemon.err -t store-check "store check WARN: see /var/lib/store-check/last.txt"
exit "$rc"
EOF
chmod 755 /usr/local/sbin/store-check-daily
cat > /etc/systemd/system/store-check.service <<'EOF'
[Unit]
Description=Read-only check of the Docker image store (records against files, canary images, free space)
After=containerd.service docker.service

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/store-check-daily
EOF
cat > /etc/systemd/system/store-check.timer <<'EOF'
[Unit]
Description=Daily read-only check of the Docker image store

[Timer]
OnCalendar=*-*-* 07:30:00
Persistent=true

[Install]
WantedBy=timers.target
EOF
systemctl daemon-reload
systemctl enable --now store-check.timer
systemctl start store-check.service; echo "exit $?"
cat /var/lib/store-check/last.txt
systemctl list-timers store-check.timer --no-pager
```

- **Expected:** `exit 1`. `last.txt` has the same three WARN lines as step 12.2: the 52,585 download records, the layer-record gap, and `/data` at about 96%. No new WARN. The timer is listed, next run tomorrow at 07:30.
- **Pass check:** the timer is listed, and `last.txt` matches those three warnings. `mark 13.1 PASS`
- **Result:** run 20 Oct 16:49 CST. **Pass.** `RESULT WARN` with the same three warnings as 12.2: 52,585 download records, 1,213 more layer records than folders, `/data` at 96%. Both database pairs match. The three canaries are `ok`. `store-check.timer` is enabled and active. Next run Wed 21 Oct 07:30 CST, 14h left. Marked `13.1 PASS` at 16:51:02.

### 13.2 The mount guard

- **Why:** the act that split the store on 12 Oct was mounting `/data` while containerd ran. `mount-data` refuses to mount while Docker or containerd is active, or while the system-disk folder isn't empty. The test run below must refuse, because Docker is running.
- **Risk:** low. One new file. It never unmounts anything; while Docker runs it only refuses.

```bash
cat > /usr/local/sbin/mount-data <<'EOF'
#!/bin/bash
# The way to mount /data by hand. A containerd that is already running keeps the database it opened, so mounting /data under it splits the store (12 Oct 2026).
for u in docker.socket docker.service containerd.service; do
  if systemctl is-active --quiet "$u"; then
    echo "refused: $u is active. First: systemctl stop docker.socket docker containerd" >&2
    exit 1
  fi
done
if mountpoint -q /data; then findmnt /data; exit 0; fi
if [ -n "$(ls -A /data)" ]; then
  echo "refused: /data on the system disk is not empty. Ask root." >&2
  exit 1
fi
mount /data && findmnt /data
EOF
chmod 755 /usr/local/sbin/mount-data
bash -n /usr/local/sbin/mount-data && /usr/local/sbin/mount-data; echo "exit $?"
```

- **Expected:** `refused: docker.socket is active. First: systemctl stop docker.socket docker containerd`, then `exit 1`.
- **Pass check:** that refusal. `mark 13.2 PASS`
- **Result:** run 20 Oct 16:51 CST. **Pass.** `refused: docker.socket is active. First: systemctl stop docker.socket docker containerd`, then `exit 1`. Nothing was mounted. Marked `13.2 PASS` at 16:52:04.

### 13.3 Check every guard

- **Why:** one record that all guards are in place. The read-only view of investigation step 6.4 shows the hidden folder again.
- **Risk:** low. The view is read-only and removed at once.

```bash
{
findmnt -no SOURCE,UUID /data
findmnt --verify 2>&1 | tail -n 1
systemctl cat containerd docker | grep -E '^(RequiresMountsFor|AssertPathIsMountPoint)'
mkdir -p /mnt/rootfs && mount -o bind,ro / /mnt/rootfs && lsattr -d /mnt/rootfs/data && ls -A /mnt/rootfs/data | wc -l
umount /mnt/rootfs; findmnt /mnt/rootfs || echo "view removed"
systemctl is-enabled store-check.timer
ls -l /usr/local/sbin/mount-data /usr/local/sbin/store-check /usr/local/sbin/store-check-daily
} 2>&1 | tee "$R/out/13.3_guards.txt"
```

- **Expected:** `/dev/nvme1n1p1 25454905-…`; `0 parse errors, 0 errors, 0 warnings`; the four guard lines; the `i` flag on `/mnt/rootfs/data` and `0`; `view removed`; `enabled`; the three files.
- **Pass check:** every line as expected. `mark 13.3 PASS`
- **Result:** run 20 Oct 16:52 CST. **Pass.** `/dev/nvme1n1p1` with UUID `25454905-c932-4246-baa7-e50351cb6798`. `findmnt --verify` printed `Success, no errors or warnings detected` (the current wording of a clean check). Containerd and Docker each have `RequiresMountsFor=/data` and `AssertPathIsMountPoint=/data`. The hidden folder shows `----i---------e-------` and `0` entries. `view removed`. `store-check.timer` is `enabled`. The three scripts are in `/usr/local/sbin`. Marked `13.3 PASS` at 16:53:24.

### 13.4 Guards that are decisions, not commands

These are in mentor report section 9.2. None is changed in this window, because each needs its own design and test.

| Guard | Why | Who decides |
|---|---|---|
| A memory ceiling for containers and for `user.slice`; `systemd-oomd` | The 11 and 12 Oct hang started with memory | Root |
| A registry as the source of truth for images built here | A damaged store becomes a refill, not weeks of rebuilds | Root and the mentor (report decision 3) |
| Personal admin accounts; `auditd` on `/etc/fstab`, `/etc/containerd`, `/etc/docker`, `/etc/systemd`, and on deletions in `/data/containerd` | Who did what is known next time | Root |
| Keep containerd's and Docker's logs for 30 days | The evidence of a next incident survives | Root |
| An alert from the daily check to the team, and the Jenkins health gate | Today the check writes a file and a journal line only | The mentor (beacon; named mac-k3d until the 20 Oct migration) |
| The watched reboot | It proves the fstab line and the guards at boot | Root, step 18 |
| The rules for direct use: one host owner, no global clean-up | Mentor report 9.3 | The team |

## Step 14: reopen Docker

### 14.1 Give the `docker` group its socket back, and restore the canary

- **Risk:** low.

```bash
chgrp docker /run/docker.sock
stat -c '%U:%G %a %n' /run/docker.sock
runuser -u toby -- docker version --format '{{.Server.Version}}'
crontab -u toby "$R/out/2.5_toby_crontab_before.txt"
crontab -u toby -l
```

- **Expected:** `root:docker 660 /run/docker.sock`; `29.6.2` for `toby`; the canary line.
- **Pass check:** all three. `mark 14.1 PASS`
- **Result:** run 20 Oct 16:53 CST. **Pass.** `root:docker 660 /run/docker.sock`. `toby` sees `29.6.2`. The canary crontab line is back: minute 17, `alpine:3.21`, `alpine:3.22`, `busybox:1.36`, `debian:bookworm`. Mark `mark 14.1 PASS` if the checklist does not already have it.

### 14.2 Set aside the stored environment checks (name before 20 Oct)

- **Why:** a stored "pass" from before the repair must not stand in for a check on the repaired store (mentor report 9.4). This block uses the path from before the 20 Oct codebase migration, when the project was still named mac-k3d.
- **Risk:** low. It renames `toby`'s folder. Renaming it back undoes it.

```bash
d=/home/toby/.local/share/mac-k3d/env-check
if [ -d "$d" ]; then mv "$d" "$d.before-repair-$(date +%F)" && ls -ld "$d".before-repair-*; else echo "no env-check folder"; fi
```

- **Expected:** the renamed folder, or `no env-check folder`.
- **Pass check:** one of the two. `mark 14.2 PASS`
- **Result:** run 20 Oct 16:55 CST. **Pass** for the old path. `no env-check folder`. Nothing was renamed. That path is the pre-migration name. Step 14.2b checks `beacon`.

### 14.2b Set aside beacon's stored environment checks

- **Why:** on 20 Oct the codebase was renamed from mac-k3d to beacon. The stored environment checks, the config directory, and the command are `beacon` now. Renaming this folder makes the next `beacon prepare` prove each image again.
- **Risk:** low. It renames `toby`'s folder. Renaming it back undoes it.

```bash
d=/home/toby/.local/share/beacon/env-check
if [ -d "$d" ]; then mv "$d" "$d.before-repair-$(date +%F)" && ls -ld "$d".before-repair-*; else echo "no env-check folder"; fi
```

- **Expected:** the renamed folder, or `no env-check folder`.
- **Pass check:** one of the two. `mark 14.2b PASS`
- **Result:** run 20 Oct 17:09 CST. **Pass.** `no env-check folder`. Nothing was renamed. Mark `mark 14.2b PASS`.

### 14.2c Remove the unused folderless snapshot records

- **Why:** 45 Docker snapshot records still have no folder. No current image and no live container uses them. Eight more records sit on top of them, also unused. Removing the 53 leaves every current image chain in place. `moby_history` is not touched.
- **Risk:** low. The step stops before deleting if the counts are not 45 and 53, or if an image or a live container is in the list.

```bash
cd "$R/out"
python3 - << 'PY'
import os, struct, subprocess, sys
from collections import defaultdict
snapdir="/data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots"
path="/data/containerd/io.containerd.snapshotter.v1.overlayfs/metadata.db"
f=open(path,"rb")
def rd(off,n):
    f.seek(off); return f.read(n)
def u16(b,o): return struct.unpack_from("<H", b, o)[0]
def u32(b,o): return struct.unpack_from("<I", b, o)[0]
def u64(b,o): return struct.unpack_from("<Q", b, o)[0]
def read_page(pgid):
    b=rd(pgid*4096,4096); ov=u32(b,12)
    if ov: b=rd(pgid*4096,4096*(ov+1))
    return b
def items(page):
    if u16(page,8)&1:
        acc=[]
        for i in range(u16(page,10)):
            acc.extend(items(read_page(u64(page,16+i*16+8))))
        return acc
    acc=[]
    for i in range(u16(page,10)):
        base=16+i*16
        flags,pos,ksize,vsize=u32(page,base),u32(page,base+4),u32(page,base+8),u32(page,base+12)
        k=page[base+pos:base+pos+ksize]
        v=page[base+pos+ksize:base+pos+ksize+vsize]
        if flags&1: acc.append((k,u64(v,0),v[16:] if len(v)>16 else None))
        else: acc.append((k,v))
    return acc
def fields(root,inline):
    page=inline if inline is not None else read_page(root)
    return {a:b for a,b in ((it[0],it[1]) for it in items(page) if len(it)==2)}
def uvarint(b):
    x=s=0
    for c in b:
        x|=(c&0x7F)<<s
        if c<128: return x
        s+=7
    return x
b0,b1=read_page(0),read_page(1)
meta=b0 if u64(b0,64)>=u64(b1,64) else b1
top=items(read_page(u64(meta,32)))
v1=items(read_page(top[0][1]))
recs=items(read_page([r for r in v1 if r[0]==b"snapshots"][0][1]))
miss=set()
for key,root,inline in recs:
    key_s=key.decode()
    if not key_s.startswith("moby/"):
        continue
    sid=str(uvarint(fields(root,inline)[b"id"]))
    if not os.path.isdir(os.path.join(snapdir,sid)):
        miss.add(key_s.split("/",2)[-1])
kids=defaultdict(set)
for line in subprocess.check_output(["ctr","-n","moby","snapshots","ls"], text=True).splitlines()[1:]:
    p=line.split()
    if len(p)>=2 and p[1] not in ("-","<none>"):
        kids[p[1]].add(p[0])
remove=set(miss)
changed=True
while changed:
    changed=False
    for n in list(remove):
        for c in kids.get(n,()):
            if c not in remove:
                remove.add(c)
                changed=True
images=subprocess.check_output(["ctr","-n","moby","images","ls","-q"], text=True).split()
used=set()
for img in images:
    lines=subprocess.check_output(["ctr","-n","moby","images","usage",img], text=True, stderr=subprocess.DEVNULL).splitlines()[1:]
    used.update(ln.split()[0] for ln in lines if ln.strip())
live=set(subprocess.check_output(["docker","ps","-aq","--no-trunc"], text=True).split())
live_hit=0
for n in remove:
    base=n
    for suf in ("-init-key","-init"):
        if base.endswith(suf):
            base=base[:-len(suf)]
    if n in live or base in live:
        live_hit+=1
print("folderless", len(miss))
print("remove", len(remove))
print("overlap_images", len(remove & used))
print("overlap_live", live_hit)
if not (len(miss)==45 and len(remove)==53 and len(remove & used)==0 and live_hit==0):
    print("STOP")
    sys.exit(2)
left=set(remove)
order=[]
while left:
    leaves=sorted(n for n in left if not (kids.get(n, set()) & left))
    if not leaves:
        print("STOP")
        sys.exit(2)
    order.extend(leaves)
    left.difference_update(leaves)
open("14.2c_snapshots_to_remove.txt","w").write("".join(n+"\n" for n in order))
print("READY", len(order))
PY
if [ $? -ne 0 ]; then
  echo "stopped before any removal"
else
  : > 14.2c_snapshots_rm.txt
  while read -r k; do
    if ctr -n moby snapshots rm "$k" >> 14.2c_snapshots_rm.txt 2>&1; then
      echo "ok $k" >> 14.2c_snapshots_rm.txt
    else
      echo "FAIL $k" | tee -a 14.2c_snapshots_rm.txt
      break
    fi
  done < 14.2c_snapshots_to_remove.txt
  echo "snapshot_ok $(grep -c '^ok ' 14.2c_snapshots_rm.txt)"
  echo "snapshot_fail $(grep -c '^FAIL ' 14.2c_snapshots_rm.txt || true)"
  echo "images $(ctr -n moby images ls -q | wc -l)"
fi
```

- **Expected:** `folderless 45`, `remove 53`, `overlap_images 0`, `overlap_live 0`, `READY 53`, then `snapshot_ok 53`, `snapshot_fail 0`, `images 932`.
- **Pass check:** those counts. `STOP` or `snapshot_fail` above 0 means nothing further: send the output. `mark 14.2c PASS`
- **Result:** run 20 Oct 17:16 CST. **Pass.** `folderless 45`, `remove 53`, `overlap_images 0`, `overlap_live 0`, `READY 53`, `snapshot_ok 53`, `snapshot_fail 0`, `images 932`. Mark `mark 14.2c PASS`.

### 14.3 Tell the team

- **Where:** terminal 2. Skipped for now. Send it, and mark 14.3, only after the remaining local steps (16.1, 16.2, 16.3) are done. Step 15 stays with the owner.

```text
Cloud worker: the image store is repaired and checked (20 Oct 16:53 CST).
- Docker is open again. Until the jiuwenswarm rebuild passes, only its owner uses the worker.
- 932 Docker images start.
- Docker download records: 4,054, and every one has its file. Every blob file on disk has a record. Every snapshot folder has a record. The 45 unused snapshot records that had no folder were removed, with 8 records that sat on them.
- 52,340 download records in moby_history have no file. That namespace is not Docker images and was left in place. About 245 more missing download records are in old lolbench recovery namespaces.
- Images that failed before the window were removed. If you need one, pull or rebuild it.
- Containers that were running before and are not running now (restart them if you need them):
  jiuwen-deepseek-paired15-dind-20261015    swe-marathon-cpu/daemon-bootstrap:20261012-r3-v2
  remain178-one-image-probe                 remain178-dind-probe:29.6.2-flat
  dgm-container-20261013_185857_203129      sha256:b666ea37cd79e190b824f146d50caa54967bf0200f39fba70414756b61e169e1
  dgm-container-20261013_182507_266137      sha256:5f0e700bed5dc9c214d8e4afc2446f376e025537e85941830814ff111af9cc90
  dgm-container-20261013_182507_220256      sha256:5f0e700bed5dc9c214d8e4afc2446f376e025537e85941830814ff111af9cc90
  dgm-container-20261013_182504_991015      sha256:5f0e700bed5dc9c214d8e4afc2446f376e025537e85941830814ff111af9cc90
  dgm-container-20261013_182504_889691      sha256:5f0e700bed5dc9c214d8e4afc2446f376e025537e85941830814ff111af9cc90
  dgm-container-20261013_182502_090845      sha256:5f0e700bed5dc9c214d8e4afc2446f376e025537e85941830814ff111af9cc90
  dgm-container-20261013_182502_031492      sha256:5f0e700bed5dc9c214d8e4afc2446f376e025537e85941830814ff111af9cc90
  jiuwen-marathon-pure-r2-dind-20261013     swe-marathon-cpu/daemon-bootstrap:20261012-r3-v2
  jiuwen-marathon-pact-dind-20261013        swe-marathon-cpu/daemon-bootstrap:20261012-r3-v2
  jiuwen-marathon-cpu-default-dind-20261012-r3    swe-marathon-cpu/daemon-bootstrap:20261012-r3-v2
- New on the server: a daily read-only store check (/var/lib/store-check/last.txt), and /usr/local/sbin/mount-data, the only way to mount /data by hand.
- On 20 Oct the codebase was renamed from mac-k3d to beacon. Commands are beacon. Config is ~/.config/beacon. The stored image check is ~/.local/share/beacon/env-check.
- Jenkins stays offline until root and the mentor sign off.
```

- **Pass check:** sent, after the server repair is finished. `mark 14.3 PASS`
- **Result:** waiting, 20 Oct 17:47 CST. Not sent. The local steps are done. Toby sends it when he is ready.

## Step 15: jiuwenswarm, by its owner

### 15.1 The owner's rebuild

- **Why:** the 418 jiuwenswarm images are built on this server (`docker.io/library/jiuwenswarm-…` names), so they can't be pulled. How they are built is the owner's; it is not in this guide.
- **Risk:** the owner's build adds images and changes nothing that exists.

Ask the owner to:

1. Write down the exact build command, and which image names it makes.
2. Run it in a recorded session: `script -q -a ~/jiuwenswarm-rebuild-$(date +%F).log` before the build, `exit` after it.
3. Tell you when it is finished, and send the log.

Record the start, in window 0:

```bash
mark 15.1 INFO "<owner>: <build command>, started <time>"
```

- **Pass check:** the owner reports the build finished, with the log. `mark 15.1 PASS`
- **Result:** waiting, 20 Oct 17:47 CST. The local steps are done. The owner runs this when they want those images. There is no command for Toby.

### 15.2 Check

- **Why:** the third test of the method, on images that had missing files shared with 625 other images.
- **Risk:** low.

```bash
cd "$R/out"
ctr -n moby images ls -q | grep -E '/library/jiuwenswarm-(bench|probe)-(agent|base):' > 15.2_jiuwenswarm.txt
wc -l < 15.2_jiuwenswarm.txt
grep -E '/library/jiuwenswarm-' /home/toby/snapshot-audit/inventory/images_list.txt | sort > 15.2_jiuwenswarm_1015.txt
comm -13 <(sort 15.2_jiuwenswarm.txt) 15.2_jiuwenswarm_1015.txt | wc -l
bash "$R/bin/start_test.sh" 15.2_jiuwenswarm.txt 15.2_jiuwenswarm_test.tsv
cut -f1 15.2_jiuwenswarm_test.tsv | sort | uniq -c
bash "$R/bin/store_check.sh" 2>&1 | grep -E '^(records_without_blob|snapshot_records|snapshot_folders|WARN|RESULT)'
```

- **Expected:** 418 names, or the owner's new set; the number of 15 Oct names not built again, `0` or explained by the owner; every line `ok`; `records_without_blob 0`, layer records not more than folders.
- **Pass check:** every line `ok`, the owner confirms the set, and the store lines hold. `mark 15.2 PASS`
- **Result:** waiting, 20 Oct 17:47 CST. It follows 15.1.

## Step 16: close the record and hand it over

### 16.1 The checklist and the service logs

- **Why:** containerd's and Docker's own logs for the window belong with the record. In window 1, end the watch with `Ctrl-c`, then type `exit` once, so its recording is closed.
- **Risk:** none.

```bash
cd "$R"
journalctl -u containerd -u docker -u docker.socket --since "$REPAIR_START" -o short-iso --no-pager > journal_window.txt
wc -l < journal_window.txt
column -t -s $'\t' checklist.tsv | tee checklist.txt
```

- **Expected:** a line count, then the checklist. Steps 14.3, 15.1, 15.2, 16.4 and 17 stay unmarked on purpose. Both files written is the pass.
- **Pass check:** both files written. `mark 16.1 PASS`
- **Result:** run 20 Oct 17:38 CST. **Pass**, marked at 17:39:18. `journal_window.txt` is 215,557,708 bytes, 321,121 lines. `checklist.txt` was 62 lines at 17:38, before this mark. Step 16.2 rewrites it so the sealed copy includes 16.1 and 16.2. `14.2c` is PASS, marked at 17:18:13 and again at 17:25:42. Steps 14.3, 15.1 and 15.2 are absent, as planned.

### 16.2 Stop recording, and seal the folder

- **Why:** after one `exit`, the recording stops and nothing more is written into `$R`. Rewriting `checklist.txt` first puts the 16.1 and 16.2 marks into the sealed copy. The checksums and the packed file make any later change visible.
- **Risk:** none.

```bash
mark 16.2 PASS "recording ends"
column -t -s $'\t' checklist.tsv | tee checklist.txt
exit
```

Then, outside the recording. On this host the outer shell also shows `[repair`, and its working directory is `~`. That is the right shell. `unset` stops that shell from appending the seal commands to `commands.history` between the checksum and the tar.

```bash
unset PROMPT_COMMAND
R=$(sed -n 's/^export R=//p' /root/cloud-worker-repair.env)
cd "$R" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS
tar -C /var/log/cloud-worker-repair -czf "$R.tar.gz" "$(basename "$R")"
sha256sum "$R.tar.gz"
ls -lh "$R.tar.gz"
```

- **Expected:** one hash line for `2026-10-16.tar.gz`, and its size.
- **Pass check:** the hash. Write it down for 16.3 and 16.4.
- **Result:** run 20 Oct 17:42:03 CST. **Pass.** `b629f4e14403f1f3067b3e847efc38a3039c377427c0388c39dc7d125e846373  /var/log/cloud-worker-repair/2026-10-16.tar.gz`, 116 MB. Next is 16.3.

### 16.3 Copy it to your PC

First, in terminal 1: `exit` (ends `tmux`), then `exit` (ends the SSH session). Then, in terminal 2:

```bash
mkdir -p ~/cloud-worker-evidence/repair
scp -i ~/.ssh/cloud-worker-root.pem -o IdentitiesOnly=yes 'root@47.84.16.18:/var/log/cloud-worker-repair/*.tar.gz' ~/cloud-worker-evidence/repair/
sha256sum ~/cloud-worker-evidence/repair/*.tar.gz
```

- **Expected:** the same hash as in 16.2.
- **Pass check:** the same hash. The copy stays outside the beacon repository: it has user names and root's commands. The project was named mac-k3d until the 20 Oct codebase migration.
- **Result:** run 20 Oct 17:47 CST. **Pass.** `b629f4e14403f1f3067b3e847efc38a3039c377427c0388c39dc7d125e846373  /home/Toby/cloud-worker-evidence/repair/2026-10-16.tar.gz`. The `tmux` session `repair` had already ended (`[exited]`). This pass is recorded here, not with `mark` on the server, because the archive is already sealed.

### 16.4 Report to root and the mentor

```text
Cloud worker repair finished, <date> <start> to <end>.
- Every step passed; checklist in <R>/checklist.txt on the worker.
- Before and after: <paste 12.3_before_after.txt>
- Images: <n> start (12 bases, 113 DeepSWE, <n> restored, <n> kept, <n> jiuwenswarm). <n> failing names were removed.
- Guards: hidden /data folder empty and immutable, one store, daily store check, mount-data.
- Logs: /var/log/cloud-worker-repair/<date>.tar.gz on the worker, sha256 <hash>; a copy on my PC.
- Kept on the worker until you close the review: /root/cloud-worker-repair-backup/ (database copies, old hidden store)
  and /data/cloud-worker-repair/saved/ (the tar files, about 52 GB).
- For your decision: Jenkins node back online (step 17), beacon's AGENT_LABEL, and the watched reboot (step 18). The project was named mac-k3d until the 20 Oct codebase migration.
```

- **Pass check:** sent.
- **Result:** waiting, 20 Oct 17:47 CST. The local steps are done. Toby sends this with the team message when he is ready. It is not marked on the server.

## Step 17: back to Jenkins (after sign-off)

- **Where:** terminal 2, Jenkins.
- **Why:** the node goes back only when root and the mentor have read the report of 16.4.
- **Risk:** low.

In Jenkins: **Manage Jenkins → Nodes → `linux-iZt4ndd2dff7gqjta7mppaZ` → Bring this node back online**.

- beacon jobs stay on `AGENT_LABEL=linux-Michael-Ubuntu` until the mentor moves them (mentor report, decision 2). The project was named mac-k3d until the 20 Oct codebase migration.
- The first beacon run on this node is the mentor's decision. The command is `beacon`.
- **Pass check:** written sign-off from root and the mentor; the node is online.
- **Result:** not run yet.

## Step 18: later, in its own window: the watched reboot

- **Where:** root at the Alibaba Cloud console, you in an SSH session. Announce it like step 0.2. Start a recording as in steps 1.2 to 1.4; it gets a folder of its own date.
- **Why:** the `UUID=` fstab line and the two guards were written on 12 Oct, and no boot has tested them. This proves that the boot mounts `/data` before containerd starts.
- **Risk:** **high.** Every container stops. The fstab line has no `nofail`, so if `/data` does not mount, the boot stops in emergency mode and SSH is not available: only the console can help. Whether to add `nofail` (boot goes on without `/data`, containerd refuses to start, SSH works) is root's decision. This guide does not change fstab.

Before the reboot:

```bash
findmnt --verify 2>&1 | tail -n 1
/usr/local/sbin/store-check --quick
systemctl stop docker.socket docker containerd
systemctl reboot
```

After the boot, logged in again:

```bash
findmnt -no SOURCE,UUID /data
systemctl show -p ActiveEnterTimestamp data.mount containerd.service docker.service
/usr/local/sbin/store-check docker.io/library/alpine:3.21 docker.io/library/busybox:1.36
```

- **Expected:** before: `0 parse errors, 0 errors, 0 warnings` and `RESULT OK`. After: `/dev/nvme1n1p1 25454905-…`; the time for `data.mount` is earlier than the time for `containerd.service`; `open_meta.db` the same pair; canaries `ok`; `RESULT OK`, or `WARN` only for disk space.
- **If the console shows emergency mode:** root runs `journalctl -b -u data.mount --no-pager | tail -n 30` and sends it. Don't edit fstab from memory, and don't paste a system command copied from escaped text.
- **Pass check:** `/data` mounted before containerd; the store check holds.
- **Result:** not run yet.

## Step 19: published images Toby can pull, in need order

The consistency fix is finished. This step refills published images that are still missing. Pull in this order. The 12 evaluation bases and the 113 published DeepSWE images are already back. Local builds stay with their owners: jiuwenswarm (416), chrys-bench (534), local `deepswe*` (486), deepswe-ab-verifier (113), capbench other than the four `jdk` images (63), and the task images. Toby does not have those build commands.

| Order | Need | What | Time |
|---|---|---|---|
| 19.1 | Must. Shared bases other images are built on. The evaluation tags are already back | `python` slim tags, `eclipse-temurin` 8, 11, 17, three `maven` tags | 45 to 90 min |
| 19.2 | Likely. `docker build` uses buildkit. lolbench's published conda base is mambaforge; one local parent tag already starts | `moby/buildkit:v0.24.0`, `condaforge/mambaforge:24.7.1-2` | 15 to 40 min |
| 19.3 | Likely. These eight tags are the lolbench images the investigation named as stale. The other 235 lolbench images already start | `TypeScript_2`, `aspnetcore_1`, `cargo_1`, `rust_1` on `smartdub26/lolbench` and `lolbench-linux` | 30 to 90 min |
| 19.4 | Likely. The rest of the published lolbench tags | `arrow_2`, `cpython_5`, `jdk_1`, `jdk_2`, `jdk_3`, `jdk_5` | 30 to 60 min |
| 19.5 | Likely when swebenchpro runs. beacon's pipeline pulls `sweap-images` | 40 tags of `jefzda/sweap-images`. Two other tags already start | 2 to 4 h |
| 19.6 | Likely when classic SWE-bench runs | 42 `swebench/sweb.eval.x86_64.*` images | 4 to 8 h |
| 19.7 | Later. One-off images. Run only if root asks | `curl`, `verus`, `dafny`, two `xinzhou1997` tags, four terminal-bench digests | 30 to 90 min |

The 16 Oct log is sealed. The pulls on 21 Oct are in `/var/log/cloud-worker-repair/2026-10-20-pulls/out`. Docker is open. A large group uses disk and bandwidth.

The times are reserves. Step 9 pulled the small base images in about 10 minutes. A tag the registry no longer has fails in seconds.

Before 19.5 or 19.6:

```bash
df -h /data
```

At 16:42, `/data` was 96% full on the 14 TB disk, about 560 GB free. Start a large group only when at least 200 GB is free.

### 19.1 Must: official bases still missing, about 45 to 90 minutes

These are the shared bases that are still gone. `python:3.10-slim-bookworm` is already on the host. The `3.10-slim` name and its digest are one image. Pull the JDK and Maven tags. The October digests may have moved, so the tags are what get pulled.

```bash
mkdir -p /var/log/cloud-worker-repair/2026-10-20-pulls/out
cd /var/log/cloud-worker-repair/2026-10-20-pulls/out
cat > 19.1_pull.txt <<'EOF'
docker.io/library/python:3.10-slim
docker.io/library/python:3.10-slim@sha256:c1e4e6c01eb489c422288b2de34b0761ca316f7a2d98e2c33f47659a73ed108a
docker.io/library/python:3.11-slim@sha256:db3ff2e1800a8581e2c48a27c3995339d47bdf046da21c7627accd3d51053a93
docker.io/library/python:3.12-slim
docker.io/library/python:3.6-slim-bullseye@sha256:2cfebc27956e6a55f78606864d91fe527696f9e32a724e6f9702b5f9602d0474
docker.io/library/python:3.9-slim-bullseye@sha256:b9e06687fbfc57f6fe563e94e4c8751e39513dde89afc120dc6f56afe5ffc761
docker.io/library/eclipse-temurin:8-jdk
docker.io/library/eclipse-temurin:11-jdk
docker.io/library/eclipse-temurin:17-jdk
docker.io/library/maven:3-eclipse-temurin-17
docker.io/library/maven:3-eclipse-temurin-8
docker.io/library/maven:3.9-eclipse-temurin-11
EOF
while read -r i; do echo "== $i"; docker pull "$i"; done < 19.1_pull.txt > 19.1_pull.log 2>&1
grep -E '^==|^Status|rror|not found' 19.1_pull.log
```

- **Pass check:** each line has `Status:`, or one old digest prints `not found`. Send the `grep` if a line is `Error` for another reason.
- **Result:** done, with one follow-up. At 09:03, done: `python:3.10-slim`, its digest `sha256:c1e4e6c0…`, `python:3.12-slim`, `python:3.6-slim-bullseye@sha256:2cfebc27…`, `python:3.9-slim-bullseye@sha256:b9e06687…`, and `mirror.gcr.io/curlimages/curl:8.17.0`. `python:3.11-slim@sha256:db3ff2e1…` stopped with `AlreadyExists` on snapshot `sha256:b9b037ff…`, because that name was still in `meta.db` and the overlay snapshot was gone. At 09:08, all six of `eclipse-temurin:8-jdk`, `11-jdk`, `17-jdk`, `maven:3-eclipse-temurin-17`, `maven:3-eclipse-temurin-8`, and `maven:3.9-eclipse-temurin-11` printed `Status: Downloaded newer image`. At 09:21 the 1,623 names of that kind were removed and `moby_history` was deleted. At 09:22 the python 3.11 digest printed `Status: Downloaded newer image`. Log: `/var/log/cloud-worker-repair/2026-10-20-pulls/out/19.1c_python311.log`. `/data` has 526 GB free.

### 19.2 Likely: buildkit and mambaforge, about 15 to 40 minutes

`moby/buildkit:v0.24.0` is what `docker build` uses. `condaforge/mambaforge:24.7.1-2` is the published conda base. One local lolbench parent built from it already starts.

```bash
cd /var/log/cloud-worker-repair/2026-10-20-pulls/out
cat > 19.2_pull.txt <<'EOF'
docker.io/moby/buildkit:v0.24.0
docker.io/condaforge/mambaforge:24.7.1-2
EOF
while read -r i; do echo "== $i"; docker pull "$i"; done < 19.2_pull.txt > 19.2_pull.log 2>&1
grep -E '^==|^Status|rror|not found|AlreadyExists' 19.2_pull.log
```

- **Pass check:** each line is `Status:` or `not found`.
- **Result:** run 21 Oct 09:13 CST. Both printed `Status: Downloaded newer image`. `moby/buildkit:v0.24.0` digest `sha256:6eceb897…`. `condaforge/mambaforge:24.7.1-2` digest `sha256:84afcc24…`. Log: `/var/log/cloud-worker-repair/2026-10-20-pulls/out/19.2_pull.log`.

### 19.3 Likely: the eight lolbench tags the investigation named, about 30 to 90 minutes

The investigation named these eight as the stale lolbench images. The other 235 lolbench images already start.

```bash
cd /var/log/cloud-worker-repair/2026-10-20-pulls/out
cat > 19.3_pull.txt <<'EOF'
docker.io/smartdub26/lolbench:TypeScript_2-1.0.0
docker.io/smartdub26/lolbench-linux:TypeScript_2-1.0.0
docker.io/smartdub26/lolbench:aspnetcore_1-1.0.0
docker.io/smartdub26/lolbench-linux:aspnetcore_1-1.0.0
docker.io/smartdub26/lolbench:cargo_1-1.0.0
docker.io/smartdub26/lolbench-linux:cargo_1-1.0.0
docker.io/smartdub26/lolbench:rust_1-1.0.0
docker.io/smartdub26/lolbench-linux:rust_1-1.0.0
EOF
while read -r i; do echo "== $i"; docker pull "$i"; done < 19.3_pull.txt > 19.3_pull.log 2>&1
grep -E '^==|^Status|rror|not found|AlreadyExists' 19.3_pull.log
```

- **Pass check:** eight lines, each `Status:` or a platform error.
- **Result:** run 21 Oct 09:27 CST. Done: `smartdub26/lolbench-linux:TypeScript_2-1.0.0` (`sha256:d206ccf2…`), `lolbench-linux:aspnetcore_1-1.0.0` (`sha256:bb95c42a…`), `lolbench-linux:cargo_1-1.0.0` (`sha256:8637e8c0…`), `lolbench-linux:rust_1-1.0.0` (`sha256:92260ffb…`). Not pulled: the four `smartdub26/lolbench` tags of those names. Each returned `no matching manifest for linux/amd64/v4`. The registry index for `TypeScript_2-1.0.0` lists `linux/arm64` only. This host is `x86_64`. Do not pull those four again. Log: `/var/log/cloud-worker-repair/2026-10-20-pulls/out/19.3_pull.log`.

### 19.4 Likely: the other published lolbench tags, about 30 to 60 minutes

```bash
cd /var/log/cloud-worker-repair/2026-10-20-pulls/out
cat > 19.4_pull.txt <<'EOF'
docker.io/smartdub26/lolbench-linux:arrow_2-1.0.0
docker.io/smartdub26/lolbench:cpython_5-1.0.0
docker.io/smartdub26/lolbench:jdk_1-1.0.0
docker.io/smartdub26/lolbench:jdk_2-1.0.0
docker.io/smartdub26/lolbench:jdk_3-1.0.0
docker.io/smartdub26/lolbench:jdk_5-1.0.0
EOF
while read -r i; do echo "== $i"; docker pull "$i"; done < 19.4_pull.txt > 19.4_pull.log 2>&1
grep -E '^==|^Status|rror|not found|AlreadyExists' 19.4_pull.log
```

- **Pass check:** six lines, each `Status:` or a platform error.
- **Result:** run 21 Oct 09:38 CST. Done: `smartdub26/lolbench-linux:arrow_2-1.0.0` and `smartdub26/lolbench:cpython_5-1.0.0` (`sha256:81c5d064…`). Not pulled: `lolbench:jdk_1-1.0.0`, `jdk_2`, `jdk_3`, and `jdk_5`. Each returned `no matching manifest for linux/amd64/v4`. `jdk_1` was inspected: `linux/arm64` only. `lolbench-linux:jdk_1-1.0.0` is `linux/amd64` and was already on the server, as were `jdk_2`, `jdk_3`, and `jdk_5`. Do not pull the four `lolbench` jdk tags again. Log: `/var/log/cloud-worker-repair/2026-10-20-pulls/out/19.4_pull.log`.

### 19.4b lolbench digests still published for this host

These 15 names were on the server as digests, with no tag stored. Each index is still on Docker Hub as `linux/amd64`. Checked `sha256:02339f11…` on 21 Oct. This pull is before any other benchmark.

```bash
cd /var/log/cloud-worker-repair/2026-10-20-pulls/out
cat > 19.4b_pull.txt <<'EOF'
docker.io/smartdub26/lolbench-linux@sha256:02339f1116c0f80c333124bbc18c2f5c709b83c6593fe05ef42767fc0ce8f830
docker.io/smartdub26/lolbench-linux@sha256:1413f3b0b71a503e0639f471518b80b25afb5589c911e1b693cb3e63f2dbb4a9
docker.io/smartdub26/lolbench-linux@sha256:24e71e1fd8525b91c9a812d9890281f05da5e50dd13151b9a3ac1981e9fd937c
docker.io/smartdub26/lolbench-linux@sha256:3e27de8e3478b17f158aa35ed3352f52af77668ab2c194ef4768e707b39e88a4
docker.io/smartdub26/lolbench-linux@sha256:488d09fc11b5da03b828184e47eabd6fa14d5234f72bbcd0862254a8e1486d02
docker.io/smartdub26/lolbench-linux@sha256:4d9965e8289d3a458589d4d31a24732a0cc883bf6e609ecd06790cf47876a742
docker.io/smartdub26/lolbench-linux@sha256:6a617f82786170bda5a829457de940bab9ddf920d1d687bfe1422f73bf5de7a0
docker.io/smartdub26/lolbench-linux@sha256:73301fc8f48c7ff247678f27c2cdf78f8d22dc0976b35c487f4913a16019fadc
docker.io/smartdub26/lolbench-linux@sha256:76d5ff062f557febf3d3a6b8a33e4b2bae7380372697cc394e5062bcb0869226
docker.io/smartdub26/lolbench-linux@sha256:8ef278e184459b7c7855fdcea0f473cc1c1cbc561c6920c538ab0602f7747e5a
docker.io/smartdub26/lolbench-linux@sha256:9a9f1b83315ffb71799cbd70d6935af1644e72dd11d9363447ac7bd19101df01
docker.io/smartdub26/lolbench-linux@sha256:a7c0da52255250f8ba72a8ef44050900fac9790d7b32b5bc1d8c1d8adcc40a1e
docker.io/smartdub26/lolbench-linux@sha256:be48d9dd04d56c5bd9e91420a59699fd5b99712a2f48d99477881564e9109279
docker.io/smartdub26/lolbench-linux@sha256:cec9f7d0d9368db539dfaedce213b936dc387beab3af9eeff60727fed3f1a54c
docker.io/smartdub26/lolbench-linux@sha256:e7f7a47822522cbf7f5584add822698a48875831e6ca7dd238ad85cc3d962088
EOF
while read -r i; do echo "== $i"; docker pull "$i"; done < 19.4b_pull.txt > 19.4b_pull.log 2>&1
grep -E '^==|^Status|rror|not found|AlreadyExists' 19.4b_pull.log
```

- **Pass check:** 15 lines, each `Status:` or `not found`.
- **Result:** run 21 Oct 10:54–11:11, retry at 11:23. 14 digests printed `Status: Downloaded newer image` in the first pass. `sha256:cec9f7d0…` stopped with `503 Service Unavailable` and was not stored. The retry printed `Status: Downloaded newer image`. All 15 are on the server. Log: `/var/log/cloud-worker-repair/2026-10-20-pulls/out/19.4b_pull.log`.

### 19.5 Likely: sweap images, about 2 to 4 hours

40 tags of `docker.io/jefzda/sweap-images`. beacon's swebenchpro pipeline pulls images named `sweap-images`. Two other tags of this repository already start. Check free space first. At 16:42 about 560 GB was free. Start when at least 200 GB is free.

```bash
df -h /data
cd /var/log/cloud-worker-repair/2026-10-20-pulls/out
grep '^docker.io/jefzda/sweap-images:' /var/log/cloud-worker-repair/2026-10-16/out/remove_images.txt > 19.5_pull.txt
wc -l < 19.5_pull.txt
while read -r i; do echo "== $i"; docker pull "$i"; done < 19.5_pull.txt > 19.5_pull.log 2>&1
grep -c '^Status:' 19.5_pull.log
grep -E 'rror|not found' 19.5_pull.log | head
```

- **Expected:** `40` from `wc`.
- **Pass check:** `Status:` count plus `not found` count equals 40.
- **Result:** not run yet.

### 19.6 Likely: published SWE-bench eval images, about 4 to 8 hours

42 images, for classic SWE-bench. Pull them as `swebench/sweb.eval.x86_64.<id>:latest`. Forty were stored on this host as `library/sweb.eval…`. A pull of that `library/` name looks for a Docker Official Image and will not find one. `library/sweb.openlux.*`, `sweb.env.*` and `sweb.base.py.x86_64` are local names and are not in this list. `swebench-pro-opencode` (424 names) stays with its owner.

```bash
df -h /data
cd /var/log/cloud-worker-repair/2026-10-20-pulls/out
grep 'sweb\.eval\.x86_64\.' /var/log/cloud-worker-repair/2026-10-16/out/remove_images.txt \
  | sed -E 's#^docker.io/library/#docker.io/swebench/#' > 19.6_pull.txt
wc -l < 19.6_pull.txt
while read -r i; do echo "== $i"; docker pull "$i"; done < 19.6_pull.txt > 19.6_pull.log 2>&1
grep -c '^Status:' 19.6_pull.log
grep -E 'rror|not found' 19.6_pull.log | head
```

- **Expected:** `42` from `wc`. Start when at least 200 GB is free.
- **Pass check:** `Status:` count plus `not found` count equals 42.
- **Result:** not run yet.

### 19.7 Later, only if root asks, about 30 to 90 minutes

These are single published images. They are not a shared base and not one of the named benchmarks above. If the first terminal-bench digest is `not found`, stop that list.

```bash
cd /var/log/cloud-worker-repair/2026-10-20-pulls/out
cat > 19.7_pull.txt <<'EOF'
mirror.gcr.io/curlimages/curl:8.17.0
ghcr.io/principled-systems/verus:latest
docker.io/xtrm0/dafny:latest
docker.io/xinzhou1997/spec-agent-v17-clarify:0.135.0-py311
docker.io/xinzhou1997/swe_rpg_java:mockito__mockito-3335_linux
docker.io/harborframework/terminal-bench@sha256:2879e29fdb7a997c4dd32e12b026a9f95f17670e4b5619b52c62b3cdbb5a71de
docker.io/harborframework/terminal-bench@sha256:73e931f5311398037c2230a4aa1a179fde8619942ecdaa9d98f4fdea548dba47
docker.io/harborframework/terminal-bench@sha256:988e1ff362f361ceb1fc93e99cafc1d90f5b6c5d8fed13a5695c23b3669a127c
docker.io/harborframework/terminal-bench@sha256:ad16a57e6a031a5937dc77e9e7cc3c7aada8159ccf192731f5de783fa7667624
EOF
while read -r i; do echo "== $i"; docker pull "$i"; done < 19.7_pull.txt > 19.7_pull.log 2>&1
grep -E '^==|^Status|rror|not found' 19.7_pull.log
```

- **Pass check:** each line is `Status:` or `not found`.
- **Result:** not run yet.

## If something goes wrong

| Situation | What to do |
|---|---|
| The connection drops | Run the `ssh` line of 1.2, then `tmux attach -t repair`. The step kept running. If there is no `tmux` session, follow "If `tmux` itself is gone" in 1.4 |
| Output differs from **Expected**, or a **Pass check** fails | `mark <step> FAIL "<what you saw>"`, stop, send me the output |
| 0.3: `tmux` is `MISSING` | Stop. Root decides: `screen`, or installing `tmux` |
| 2.1: a Docker client is running, or an owner asks to wait | Wait, or ask root. Don't stop Docker under work that wasn't agreed |
| 3.1: a stop takes more than 3 minutes | Don't kill anything. In window 1: `systemctl status docker containerd --no-pager`, and send it |
| 3.2: a process has a database open | Stop. Don't copy. Send the `fuser` output |
| 4.1: the hidden folder has anything besides `containerd`, `docker` and `hanxu` | `umount /mnt/rootfs`, stop, send the listing |
| 4.2: `mv` prints an error | Stop. Nothing was deleted. Leave the view mounted, and send the output |
| 4.3: `chattr` says `Operation not supported` | `umount /mnt/rootfs`, stop, send `findmnt -no FSTYPE /` |
| 5.1: containerd does not start | Send `systemctl status containerd --no-pager` and `findmnt /data`. Don't mount anything |
| 5.1, 12.2, 18: `open_meta.db` shows two different pairs | The split-brain signal. At once: `systemctl stop docker.socket docker containerd`. Send the output |
| 6.1, 6.2: many `timeout` or `other` | Send `cut -f1,3 review_images.tsv` or `cut -f1,3,6 review_containers.tsv`. They are not removed. We test them again one at a time |
| 6.3: counts more than about 10% off the 15 Oct numbers | Send the output before 6.4 |
| 7.1: less than 200 GB free | Stop. Root decides |
| 8.1: `Error` lines | Send them. Don't use `docker rm -f` |
| 8.2: the second `Total` is not `0B` | Send `8.2_builder_prune.txt` |
| 8.3: `--sync` runs for hours | Normal while the counts in window 1 still fall. Don't interrupt. If they stop falling for 30 minutes, send window 1's numbers and `journalctl -u containerd --since "-30 min" --no-pager \| tail -n 50` |
| 8.5: `records_without_blob` is not 0, or more layer records than folders, or a kept image fails | Don't continue. Send what 8.5 lists |
| 8.9: `bad` is not 0 | Don't run 8.10. Send the printed image lines |
| 8.10: `STOP`, `RESTORED`, or `bad` is not 0 after the start | The copy was put back, or the restore block in 8.10 must be run. Don't run 8.11. Send `8.10_clear.txt` and the `images` / `bad` lines |
| 8.11: `left` is not 0 | Do not mark 8.11. Continue at 8.11b. Do not start step 9 |
| 8.11b: `STOP`, `protected_hit` is not 0, or `left` is not 0 | Don't start step 9. Send the count lines and, if removals ran, `8.11b_retry.txt` |
| 8.12: `orphan_child_keys` is not 0, or `folderless_unprotected` is not 0 | The 42 from this script is a bad parse. Do not remove those 42. Continue at 8.12b. Do not start step 9 |
| 8.12b: `STOP`, `RESTORED`, or `orphan_child_keys` is not 0 | Don't start step 9. Send `8.12b_clear.txt` |
| 9.1: `AlreadyExists` on `sha256:29df493b…` | Do not mark 9.1. Continue at 9.1a. Do not run 9.2 |
| 9.2: `alpine:3.23.4` or `uv` still fails with `already exists` | Send the line. The likely next step is removing that one layer record (`ctr -n moby snapshots rm <key>`), decided with root. It is not part of this guide |
| 10.2: DeepSWE `layers_missing` again | Do not mark 10.2. Continue at 10.2a. Do not run step 11 |
| 10.2a: `STOP`, or `snapshot_fail` is not 0 | Do not load again. Send the count lines and the `FAIL` line |
| 10.2b: the second `grep` prints anything | Do not run 10.2c. Send those lines |
| 10.2c: `layers_missing` again | Stop and send the lines |
| 12.2: `RESULT WARN` | Do not mark 12.2. The three warnings are recorded there. Continue at 12.3. Do not delete image-chain records to close the gap |
| 14.2c: `STOP`, or `snapshot_fail` is not 0 | Do not continue. Send the count lines and the `FAIL` line |
| 14.3 | Pending. Do not send the team message until the server repair is finished |
| 11.2: a line is not `ok` | Do not mark 11.2. The expressvpn digest name is step 11.2a. Send any other result |
| 11.2a: `failed to stat parent` on snapshot `159017` | Do not mark 11.2a. Continue at 11.2b |
| 11.2b: `STOP`, or `snapshot_fail` is not 0 | Do not start the image again. Send the output |
| 15: the jiuwenswarm build fails with `blob not found` or `already exists` | Send the owner's log and the store check lines of 15.2 |
| 18: emergency mode at boot | Root at the console; see step 18 |
