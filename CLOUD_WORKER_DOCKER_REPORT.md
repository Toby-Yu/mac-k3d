# Cloud worker Docker store: investigation record

- **Host:** cloud worker `iZt4ndd2dff7gqjta7mppaZ` (Jenkins node `linux-iZt4ndd2dff7gqjta7mppaZ`)
- **Prepared by:** Toby. Findings from 14 and 15 Oct 2026; investigation on 15 and 16 Oct 2026.
- **For:** the mentor and the root account holder.
- **Scope:** finding the cause only. Nothing is repaired, removed or restarted during the investigation. A proposed repair plan is in the mentor report, section 9. Nothing in it is done until the mentor and root have approved it.
- **This document is the record:**
  - what was found (sections 1 to 3);
  - the access used (section 4);
  - the questions (section 5);
  - what was done, and when (section 6);
  - the answers (section 7);
  - where each piece of evidence is (Appendix B).
- **The commands are in a separate guide:** [`CLOUD_WORKER_INVESTIGATION_STEPS.md`](CLOUD_WORKER_INVESTIGATION_STEPS.md). It has every step, with its purpose, risk, copy-paste command and expected result.
- **The summary for the mentor** is [`CLOUD_WORKER_MENTOR_REPORT.md`](CLOUD_WORKER_MENTOR_REPORT.md): cause, flowcharts, evidence map, open questions and the proposed solution.
- **Status (16 Oct):** the read-only investigation is finished. Nothing in the store had been repaired when this record was written. Appendix C, added 20 Oct after the repair, lists which missing images are published and which ones Toby can pull.
  - Guide steps 0 to 9.5, 9.7 and 9.8 are done. Steps 9.3 and 9.4 were run with `root.pem`, read-only, and the view was removed.
  - The PC copy is in `~/cloud-worker-evidence/`: `logcopy/` 3.3 GB, `logs/` 1.4 GB, `inventory/` 50 MB, `sessions/` 164 KB.
  - Step 9.6 is the written questions. No answer is recorded, and the repair does not wait.
  - Step 8.3, root removing the investigation access, is a decision in the mentor report, section 10. The sudo rule's timer still removes it on 18 Oct at about 12:03.

## Summary

- The shared image store (containerd, in `/data/containerd`) is damaged for images pulled before 12 Oct. All 113 DeepSWE images and 7 of 11 common images fail; images pulled since then work.
- Most of the damage has a clear cause: a chain of events around the restart on 12 Oct.
  1. From 11 Oct 15:21, the host ran out of memory again and again until 12 Oct 10:36. The system journal was killed at 15:23 and stopped recording. At 10:38 on 12 Oct the host was restarted without a clean shutdown: both disks had to recover their filesystem journals.
  2. The `/data` line in `/etc/fstab` had literal double quotes around it: `"/dev/nvme1n1p1 /data ext4 defaults 0 0"`. That isn't valid, so at the 10:38 boot the line was skipped and `/data` wasn't mounted.
  3. containerd started anyway, at 10:38 and again at 10:40, on the `/data` folder of the system disk. That folder already held a containerd store from 4 Aug. containerd opened that database and kept it open.
  4. At 10:54, `/data` was mounted while containerd was still running. containerd kept the August database, now hidden under the mount, but its new folders went to the real disk. The cleanup removes layer folders its open database doesn't list, so it removed the real store's layer folders.
- The fstab line was rewritten at 12:09 on 12 Oct to the disk's UUID, and containerd and Docker were set to require `/data`. Root said on 16 Oct that this has not been tested, because the host has not been rebooted. Step 9.8, run the same day as `toby`, shows the line parses (`0 parse errors, 0 errors`, three permission warnings). A line that parses today can still be skipped at the next boot. A reboot that proves it is a later, announced step, not part of this investigation.
- **The missing downloaded files come from two separate losses** (steps 9.1 to 9.1c, sections 3.2 and 7.1):
  - **Most of them from the same 12 Oct event.** Step 9.4 confirmed it.
    - Every file recorded after the 12 Oct restart is present (634), and 5,791 of the 5,795 recorded on 2 to 11 Oct are missing.
    - Files made on this host are almost all gone (41,170 missing, 80 present). Pulled files often survived (1,745 missing, 2,550 present), including all of DeepSWE's.
    - The survivors are spread evenly in the cleanup's order, so the cleanup finished its pass. The hidden database listed 786 images; the real database records 65 of them, and all 65 files are present.
    - The cleanup that runs against the wrong database is the only known process that deletes files in bulk. Step 9.4 confirmed the keep-list: the files the old database names are the ones still on disk.
  - **An earlier loss before 13 Sep, most likely an earlier split brain of the same kind.**
    - The 586 missing files that the logs name before 12 Oct were all recorded before mid-August, and no layer folder older than 10 Sep survives.
    - The hidden database already held at least 786 images on 12 Oct, and the real database doesn't record 721 of them. Yet no file was ever written to the hidden store: its `containerd` folder hasn't changed since 4 Aug 20:44:36, and it has no folder for downloaded files. So containerd had used this database before, while the real disk was mounted on top.
    - The images it lists include DeepSWE images that were pulled in September. The host did not boot in September: step 9.2c dates five boots, and none between 27 Aug 21:35 and 12 Oct 10:38. The disk was mounted by hand on that running host.
    - It left a different trace from 12 Oct (step 9.1d). From 14 Sep, layers fail because their record is gone together with their folder (`missing parent`). Only after 12 Oct do layers also fail with just the folder gone (`failed to stat parent`).
    - The boots are dated (step 9.2c), and the record days are dated (step 9.3): the old database from 4 Aug through 10 Sep, the 14 TB database again from 10 Sep. Still not proven: which boot the earlier loss followed, who mounted `/data` on 10 Sep, and why the hidden layer database had 0 snapshot folders on 12 Oct.
- **Stale records first seen on 6 and 7 Oct are most likely older, from the earlier loss** (step 9.1d). `alpine:3.20`'s missing parent already failed on 30 Sep, and `alpine:latest`'s on 29 Sep. Docker logged no error on 2 to 4 Oct, so 6 and 7 Oct are when the builds met them. Still open, and not blocking the repair: the `already exists` kind, which the log never records, and what xu's `--apply` scripts changed.
- A DeepSWE image's downloaded files are still on disk; one image exported in full. Only its unpacked layer folders are missing.

**Today's plan** (guide steps 0 to 8): about 2 hours of commands, plus a test that runs 2 to 4 hours in the background. The result is ready for the mentor at the end of the day.

| Guide step | What | Time |
|---|---|---|
| 0 | Secure `root.pem` on Toby's PC, and test it | 5 min |
| 1 | Install the investigation-only sudo rule, with `root.pem`, after root's OK | 10 min |
| 2 | Log in as `toby`, check access, start recording, save the two scripts | 5 min |
| 3 | Copy the logs, and start the long jobs: two log scans and the image start test | 15 min, then 2 to 4 hours in the background |
| 4 | The logs: boots, memory, logins, 12 and 14 Oct | 30 min |
| 5 | containerd's records against the disk | 20 min |
| 6 | What only root can read: keys, history, crontabs, the hidden store, two containers | 20 min |
| 7 | Results of the long jobs | 15 min |
| 8 | Finish; fill in sections 6 and 7 | 30 min |

None of the commands is high risk, except opening the root shell in step 1.1, which is used only to install the rule. The only low-risk actions are a temporary read-only view of the system disk, and test containers that start and are removed at once.

## 1. What was checked

As `toby`, without sudo:

1. Host facts: versions, settings, disk and permissions.
2. Eleven common images: pulled if missing, then started once with no network.
3. All 113 DeepSWE images at the benchmark commit mac-k3d pins (`0b9fabbb`): started once with no network, without pulling.
4. Possible causes, from these sources:
   - systemd's timestamps;
   - `/etc/fstab` and its backup;
   - the mount table and container creation times;
   - scheduled jobs, other Docker daemons, filesystem error counters, log sizes and the package history;
   - on 15 Oct, the boot list, the kernel log and the syslog around the 12 Oct restart.

Host facts:

- Ubuntu 26.04 LTS, kernel 7.0.0-22, 96 CPUs, 182 GB of memory. Docker 29.6.2 with containerd 2.2.6 and the containerd image store.
- The store holds 7,089 images and 497 containers, 43 of them running. `live_restore` is false, so restarting Docker stops every container. Restarting containerd does not; it was restarted three times on 14 Oct with all containers still running.
- containerd keeps its data in `/data/containerd`. `/data` is a separate 14 TB ext4 disk, 88% used, with 1.7 TB free. The system disk is 66% used, with 165 GB free.
- `toby` is in the `docker` group. Since 15 Oct he is also in `adm` and `systemd-journal`, so he can read the system journal and `/var/log` without sudo. He has no sudo yet, and his password is locked. containerd's socket is root-only (`root:root`, mode 0660).
- `sudo` on this host is `sudo-rs` 0.2.13, Ubuntu's default. Password logins over SSH are off for every account (`/etc/ssh/sshd_config.d/00-disable-password.conf`). Root may log in with a key only (`PermitRootLogin prohibit-password`).
- **How far back the logs go:**
  - The journal is capped at 3.9 GB. The log it still keeps starts on 24 Jul, then 17 Sep, then the 12 Oct boot. Step 9.2c shows the 17 Sep line is not a boot: the host stayed up from 27 Aug 21:35 until 12 Oct 10:38. containerd's entries start on 10 Oct at 02:08.
  - `/var/log/syslog*`, `auth.log*` and `kern.log*` start on 13 Sep.
  - Nothing older than 13 Sep can be checked from the logs.
  - These files rotate weekly, keeping four weeks. At the next rotation, on Sunday 18 Oct, the week of 13 to 20 Sep is deleted. Guide step 3.1 copies the logs first.

## 2. Findings

### 2.1 DeepSWE: none of the 113 images starts

All 113 images are on the host, and all 113 fail to start for the same reason: folders that hold their layers are missing.

```text
failed to stat parent: stat /data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots/154281/fs: no such file or directory
failed to mount ... lowerdir=.../snapshots/154273/fs:...:.../snapshots/153120/fs ... err: no such file or directory
```

All 113 images are built on the same 23 base layers (folders 153120 to 153142).

The images were pulled between 10 and 29 Sep. `docs/testing/question-log.md` records two runs with them on this worker:

- 8 Oct: 40 questions run and scored (`deepswe_some_task` #9).
- 9 Oct: three more passed (`deepswe_one_task` #93 and `deepswe_some_task` #11).

Their downloaded files appear to be intact. One of them (`abs-module-cache-flags`) exported in full with `docker save`: 840 MB, with no error. That export reads every downloaded file of the image. So for DeepSWE, only the unpacked layer folders are missing.

### 2.2 Common images: 7 of 11 fail

| Image | Used for | Result |
|---|---|---|
| `ghcr.io/astral-sh/uv:0.9.30-python3.13-bookworm` | Building the JiuwenSwarm bundle | Fails: layer record already exists (`sha256:51e78f04…`) |
| `alpine:3.23.4` | Harbor's egress probe | Fails: layer record already exists (`sha256:29df493b…`), first seen on 6 Oct (build #53) |
| `alpine:3.20` | mac-k3d's fallback for deleting files that containers left owned by root | Fails: parent layer missing (`sha256:08bc4e…`), seen on 7 Oct (build #61). The same missing parent is already in the log on 30 Sep (step 9.1d) |
| `alpine:3.19@sha256:6baf4358…` | mac-k3d's last egress-probe fallback | Fails: downloaded file missing |
| `debian:bookworm-slim` | Common base image | Fails: downloaded file missing |
| `ubuntu:22.04` | Common base image | Fails: downloaded file missing |
| `ubuntu:24.04` | Common base image | Fails: downloaded file missing |
| `alpine:3.21`, `alpine:3.22`, `busybox:1.36`, `debian:bookworm` | (control images) | Work |

"Downloaded file missing" is this error: `blob sha256:… expected at /data/containerd/io.containerd.content.v1.content/blobs/sha256/…: blob not found`. containerd's database lists the file, but the file is not on disk.

All four images that work were pulled on 14 Oct. `alpine:3.20`, pulled on 24 Jul, fails.

An hourly canary in `toby`'s crontab starts those four images. By 15 Oct at 09:49 it hadn't failed once (`canary_fail.log` doesn't exist), so nothing new has broken since 14 Oct.

### 2.3 Listing images fails

`docker image ls` fails on this host, each time on a different missing file:

```text
Error response from daemon: rpc error: code = NotFound desc = blob sha256:69735e6a… expected at /data/containerd/io.containerd.content.v1.content/blobs/sha256/69735e6a…: blob not found: not found
```

Ten tries named ten different missing files. Their digests are spread across the whole range, from `09b515bf…` to `fffbbb45…`. So no user can easily see which of their images are broken.

### 2.4 What happened on 11 and 12 Oct

| Time (CST) | Event | Source |
|---|---|---|
| Before the restart | `/etc/fstab` has the `/data` line inside literal quotes: `"/dev/nvme1n1p1 /data ext4 defaults 0 0"` | `/etc/fstab.bak`. `findmnt --verify --tab-file /etc/fstab.bak` reports `parse error at line 12 -- ignored` |
| 11 Oct, 15:18 to 15:24 | Messages arrive late, some by 7 hours, and Tailscale loses its connection: the host is barely responsive | Journal, last lines of the boot |
| 11 Oct, 15:21 | The kernel runs out of memory and kills a `java` process. More kills follow until 12 Oct 10:36; all of them hit `python` or `java` processes | `/var/log/kern.log`, `/var/log/syslog` |
| 11 Oct, 15:23 and 15:31 | The system journal stops answering its watchdog and is killed twice. Its last entry is at 15:24; syslog keeps recording | `/var/log/syslog` |
| 12 Oct, 10:38 | Host boots, without a clean shutdown before it. The system disk runs `orphan cleanup` on mount. The `/data` disk is present at 10:38:10, and the boot finishes mounting local filesystems at 10:38:11 without `/data` | Kernel log, `systemctl show` |
| 10:38:15 to 10:40:15 | containerd starts, times out at 10:39:45, and starts again at 10:40:15. `/data` isn't mounted yet | Journal |
| 10:54:02 | `/etc/fstab.bak` is saved | File time |
| 10:54:05 | `/data` is mounted. ext4 reports `recovery complete` and `1386 orphan inodes deleted`. It has not been unmounted since | Kernel log, `systemctl show data.mount` |
| 11:33 and 11:41 | Two containers are created on the main Docker with layer folders 37 and 54. containerd is numbering from the hidden August database; the real one is past 350,000 | Mount table, `docker inspect`, `logs/hidden_store.txt` |
| 11:48:51 | Docker and containerd are both stopped | `logs/boots.txt` (guide step 4.1) |
| 12:09:49 | `/etc/fstab` is rewritten as `UUID=25454905-… /data ext4 defaults 0 0`. Drop-ins `RequiresMountsFor=/data` and `AssertPathIsMountPoint=/data` are added to `containerd.service` and `docker.service` | File times |
| 12:54:32 | containerd starts and is stopped one second later, at 12:54:34 | `logs/day_1012.txt` |
| 13:07:52 | containerd starts and stays up. Docker starts at 13:07:55 and is running at 13:09:24. This is the return to the real database | `logs/day_1012.txt` |
| 14:41 | The next container gets layer folder 354686, so containerd is back on the real database | Mount table, `docker inspect` |

The layer folder of each container mount on the main Docker, with the container's creation time (UTC, which is CST minus 8 hours):

```text
37       2026-10-12T03:33:21 f870b22117e6
54       2026-10-12T03:41:19 6afbbec81e30
354686   2026-10-12T06:41:54 fd19d30764d0
354842   2026-10-12T16:26:34 755ae3d88b91
```

containerd was restarted again on 14 Oct at 11:11, 17:36 and 20:08. The container processes kept running. The store count afterwards was 7,089 images; it is 7,750 on 15 Oct.

What the restart means:

- **"Without a clean shutdown"** means the host was powered off or reset while it was hung. A normal shutdown unmounts the disks, and ext4 then has nothing to recover.
- **The orphan inodes** are files that had already been deleted while a program still had them open. After a crash, ext4 finishes deleting them. That is normal and isn't, by itself, a sign of data loss.
- **Who restarted the host** isn't in the host's logs. The Alibaba Cloud console records instance restarts; root can check it (section 5.2).

### 2.5 Ruled out

- **Filesystem damage:** ext4's error counters for `/data` are 0. The recovery at 10:54 replayed ext4's journal, which is what ext4 does after any unclean stop.
- **A slow disk:** the `/data` disk was present at 10:38:10, and the boot never entered emergency mode.
- **Full disks:** the system disk is 66% used, and `/data` is 88% used.
- **Scheduled jobs:** no systemd timer, system cron job or tmpfiles rule touches Docker or `/data`. Per-user crontabs and user timers need root to read (Q8).
- **Containers with the host disk:** no container on the main Docker bind-mounts `/`, `/data` or containerd's folders.
- **Other daemons:**
  - The TaskAnchor runtime, set up on 10 Sep, keeps its own store in its own folder on `/data` and already requires the `/data` mount.
  - The daemons serving `/experiment-run/docker.sock` and `/isolated/docker` run inside containers of the main Docker.
- **Configuration changes:** containerd's and Docker's config files haven't changed since 24 Jul, when `/data` was set up.
- **Package upgrades:** Docker 29.6.2 and containerd 2.2.6 were installed on 24 Jul at 16:00 and haven't been upgraded since (`/var/log/apt/history.log*`). None of the October upgrades is Docker, containerd, runc, systemd or a library they load.
- **containerd being killed for memory:** none of the processes killed on 11 and 12 Oct was containerd or Docker.

## 3. Cause, as far as known

Every failure has the same shape: containerd's records point at data that is gone.

| What is missing | Error | Examples |
|---|---|---|
| A layer's folder on disk, while the layer store still lists it | `no such file or directory` | all 113 DeepSWE images |
| A downloaded file, while containerd's database still lists it | `blob not found` | `alpine:3.19`, `debian:bookworm-slim`, `ubuntu:22.04`, `ubuntu:24.04`, `docker image ls` |
| A layer's data, while containerd's main database still lists the layer | `AlreadyExists` or `missing parent` | uv, `alpine:3.23.4`, `alpine:3.20` |

### 3.1 The 12 Oct event (explains the missing layer folders)

1. The host had run out of memory and was restarted without a clean shutdown. The boot skipped the quoted `/data` line, so `/data` wasn't mounted. There was no boot on 11 Sep (Q5). The line was already in `fstab.bak` at 10:54, so it was written before 12 Oct.
2. containerd and Docker had no rule to wait for `/data` then, so they started on the `/data` folder of the system disk. A containerd store from 4 Aug was already there. containerd opened its database and kept that file open. Step 6.4 found it still hidden under the mount: `meta.db` is 865 MB and was last written at 12 Oct 11:48:43, eight seconds before containerd stopped.
3. `/data` was mounted at 10:54 while containerd was running. From then on, containerd's new folders went to the real disk; folders 37 and 54 are there. Its records stayed in the August database, now hidden under the mount.
   - The mount is very likely root's one-liner at history line 469. `/etc/fstab.bak` was saved at 10:54:02 and the kernel mounted the disk at 10:54:05: the one-liner's `cp /etc/fstab /etc/fstab.bak`, then its `mount`.
   - Line 303 is an earlier run of the same command, on an unknown date. Its `mount` succeeded, because the `&&` chain then appended the quoted line. So `/data` was not mounted at that moment either.
4. containerd's routine cleanup removes every layer folder that its open database doesn't list. That August database does not use the real store's numbering, which is past 350,000, so the cleanup removed real layer folders. It runs whenever a layer record is removed, for example when a container is removed. So anyone using Docker in that window could have triggered it without knowing.
   - It ran three times before the mount, at 10:41 to 10:43, and failed each time with `bucket does not exist`: the hidden layer database held no layer records at all (step 9.1c). Runs that succeed log nothing at this log level.
   - After the mount, Docker kept working with the hidden database: two pulls at 11:20 and 11:23, the two containers, and a build at 11:44.
5. containerd and Docker were stopped at 11:48:51. containerd was started for one second at 12:54:32, then both were started for good at 13:07:52, with `/data` already mounted, so containerd opened the real database. The real records pointed at deleted folders. The next container, at 14:41, got folder 354686.

This fits the evidence. The DeepSWE folders are gone while their downloaded files remain, folders 37 and 54 come from a second numbering, and everything pulled since 12 Oct works.

The step that turned the wrong database into data loss was step 3: mounting `/data` under a running containerd. Had containerd and Docker been stopped first, the real store would have been untouched.

### 3.2 Not explained yet

- **Missing downloaded files** (`blob not found`). Steps 9.1 to 9.1c split them into two losses (Q13, Q14):
  - **Most of them very likely went in the 12 Oct window.** Nothing recorded after the restart is missing, and nearly everything recorded on 2 to 11 Oct is.
    - containerd's cleanup of downloaded files deletes every file that its open database doesn't list. Its open database was the hidden August one, so this explains the loss if that database lists the files that survived.
    - The cleanup goes through the files in alphabetical order of digest. The survivors are spread evenly in that order (5.3 to 6.5 % in each of the 16 groups), so it was not cut off partway (step 9.1b).
    - Docker listed 786 images from the hidden database during the split brain. The real database records 65 of them, and all 65 files are present; none is missing (step 9.1c). The full list is checked in steps 9.3 and 9.4.
  - **The rest went before 13 Sep, most likely in an earlier split brain.**
    - All 586 missing files that the logs name before 12 Oct were recorded before mid-August. No layer folder older than 10 Sep 09:22 survives.
    - The hidden database held at least 786 images on 12 Oct, and the real database doesn't record 721 of them.
    - No file was ever written to the hidden store itself: its `containerd` folder hasn't changed since 4 Aug 20:44:36, and it has no folder for downloaded files. So those images were pulled while containerd had the hidden database open and the real disk was mounted on top: a split brain.
    - The images it lists include 44 from DeepSWE's registry, recorded in the real database 4 to 5 weeks ago. Only two pulls are logged on 12 Oct, too few to have added them then. So the hidden database was also in use after DeepSWE was pulled in September. That was during the uptime that began at the 27 Aug 21:35 boot, not after an 11 Sep boot.
    - The real database has no file record created from about 16 Aug to 3 Sep, as if containerd used another database then too.
    - Doesn't fit yet: the hidden layer database was empty on 12 Oct. A split brain with Docker running would normally leave layer records there.
      - One possible reason: containerd may open the layer database only when it first needs it, not at start. If `/data` was mounted before that moment, containerd would have used the real layer database with the hidden main database.
      - The layer errors fit this (step 9.1d). Before 12 Oct, the only layer error is `missing parent`: the layer database has no record of a layer that the main database lists, so the record went together with its folder. `failed to stat parent`, a folder gone while its record is kept, appears only after 12 Oct.
      - containerd has two layer cleanups. The main database's cleanup removes a layer through the layer store, record and folder together. The layer store's own cleanup removes only folders that its database doesn't list. So in the earlier loss, the hidden main database's cleanup worked through the real layer database. On 12 Oct, the layer store's cleanup worked against the hidden layer database, which held only that hour's layers.
      - To be checked in containerd's source and in step 9.3.
- **Stale records first seen on 6 and 7 Oct:** most likely older (Q17).
  - `alpine:3.20` (pulled 24 Jul) fails with a `missing parent` that was already in the log on 30 Sep, and `alpine:latest`'s on 29 Sep. `missing parent` errors run from 14 Sep.
  - Docker logged no error at all on 2 to 4 Oct, and one on 5 Oct. So 6 and 7 Oct are when the builds met these records, not necessarily when they broke.
  - Still open: `alpine:3.23.4`'s `already exists` (6 Oct), a kind the log never records, and whether xu's `--apply` scripts changed any records.
- **15 GB of syslog:** `/var/log/syslog.1`, covering 4 to 11 Oct, is 15 GB. The days since have produced far less.
- **Disk write stalls on 14 Oct:** from 20:21, the kernel reported `dockerd` blocked for more than 2 minutes, waiting for disk writes to finish. It doesn't delete data, but it shows that the `/data` disk can stall under load.

The candidates, and what each one predicts:

| Candidate | It predicts | Checked by |
|---|---|---|
| A clean-up that deleted files by age, for example `find … -mtime +30 -delete` run on `/data` to free space | Missing files are all older than some age; files still present are younger. Ruled out for the whole loss on 15 Oct: 1,861 files that are 2 months old are still present, while files only 2 to 3 weeks old are almost all missing (Q7). Still open for the earlier loss: the files reported missing before 12 Oct are all 2 months old (Q14) | Q7, Q8, Q14 |
| The 12 Oct split brain, for the downloaded files as well as the folders | Files recorded after the restart are present, and files recorded shortly before it are missing; the files that survive are the ones the hidden database lists. The first part fits (Q13). The second is checked in step 9.3 | Q13 |
| Files deleted by hand in `/data/containerd`, for example to free space (only root can write there) | A deletion in root's history, a root login or a sudo command at that time. The retained history has no `rm`, `prune` or `find -delete` of the store (Q8). It does have xu's Docker repair scripts, several run with `--apply` | Q2, Q8 |
| Files lost in the forced restart on 12 Oct | Only files written shortly before 12 Oct 10:38 are missing; the old images' files aren't. Ruled out for the missing downloaded files on 15 Oct: most missing files are weeks or months old (Q7). The restart still explains the missing layer folders | Q7, Q9 |
| An earlier memory crisis or hang, around 6 and 7 Oct | Out-of-memory kills or hung tasks in the kernel log on those days. Ruled out on 15 Oct: none on either day (Q9) | Q6, Q9 |
| An earlier split brain: the same fstab problem at the 4 Aug boot or one of the two 27 Aug boots, with `/data` mounted by hand on the host that then stayed up until 12 Oct | There was no boot on 11 Sep (step 9.2c). fstab had no valid `/data` line, so the disk was mounted by hand. The hidden database held at least 786 images whose files were written to the real disk, including September's DeepSWE images (Q15). Which of the three boots, and when the disk was mounted, are still open | Q5, Q15, Q16 |

### 3.3 What it means on a shared host

- The 12 Oct event wasn't caused by any user's work. It came from a configuration mistake, a host that ran out of memory, and a recovery step done while the services were running. Still, every user's images were affected.
- 12 accounts are in the docker group, which is root-equivalent: any of them could reach the store through a container that mounts the host. So the eventual prevention will need a team rule as well as technical guards.

## 4. Access for the investigation

### 4.1 Risk levels

Every command in the guide has one of these labels:

- **None:** it only reads. It writes nothing except files in Toby's own folder `~/snapshot-audit/`.
- **Low:** it makes a small change that removes no data and can be undone, for example a temporary read-only view of the system disk, or a test container that starts and is removed at once.
- **High:** no limits. The only high-risk command is opening the root shell in guide step 1.1, which is used only to install the rule.

### 4.2 The investigation-only sudo rule

Root gave Toby `root.pem` on 15 Oct. Toby uses it once, after root's OK, to install a sudo rule for `toby` (guide step 1). The rule is in `/etc/sudoers.d/90-toby-investigate`; the guide shows its exact text.

The rule lets `toby` run 17 exact commands as root, with fixed arguments. sudo refuses anything else, including the same program with other arguments. So `sudo -i`, `sudo bash`, `sudo rm`, `sudo systemctl`, `sudo ctr … rm` and any other `sudo mount` are refused. The rule parses with this host's `visudo` (`sudo-rs`) and with the classic `visudo.ws`; checked on 15 Oct.

| Allowed command | What it reads | Guide step |
|---|---|---|
| `ctr -n moby images ls -q` | Every image name | 3.3 |
| `ctr -n moby images check` | For each image: are all its downloaded files present, and is it unpacked | 5.1 |
| `ctr -n moby snapshots ls` | Every layer record | 5.1 |
| `ctr -n moby content ls` | Every downloaded-file record, with its age | 5.1 |
| `ctr -n moby content active` | Downloads in progress | 5.1 |
| `ls -l --full-time …/overlayfs/snapshots` | The layer folders on disk | 5.1 |
| `ls -U …/blobs/sha256` | The downloaded files on disk | 5.1 |
| `ls -ld --full-time …/snapshots/37 …/snapshots/54` | The two folders from the split brain | 6.5 |
| `ssh-keygen -lf /root/.ssh/authorized_keys` | The fingerprints of the keys that may log in as root | 6.1 |
| `cat /root/.bash_history` | Root's shell history | 6.2 |
| `grep -rH . /var/spool/cron/crontabs` | Every user's crontab | 6.3 |
| `ls -laR` of naiyu's, yun's and zihan's `~/.config/systemd/user` | File names of the three users' timers and services | 6.3 |
| `mkdir -p /mnt/rootfs`, `mount -o bind,ro / /mnt/rootfs`, `ls -la --full-time /mnt/rootfs/data …`, `du -sh /mnt/rootfs/data`, `umount /mnt/rootfs` (5 commands) | A temporary read-only view of the system disk, to see the store hidden under `/data` | 6.4 |

Why this is safe:

- **Only reading.** Every allowed command lists or reads, except the read-only view, which is mounted and removed in the same step. `ctr` has no read-only switch, so the rule fixes each command's arguments exactly.
- **Jenkins jobs can't use it.** The Jenkins agent also runs as `toby`. sudo asks for the password, and `sudo-rs` remembers it for 15 minutes only in the terminal where it was typed. A job has no terminal and no password.
- **Every sudo command is logged** in `/var/log/auth.log` and the journal (section 4.5).
- **It ends.** A timer removes the rule after 3 days, and root removes it by hand at the end (guide step 8.3).
- **Privacy:** the rule lets Toby read root's shell history and every crontab, but no other user's history or files. For three users who run background services (naiyu, yun and zihan), it lists only the file names of their user timers.

### 4.3 `root.pem`

`root.pem` is the private key that logs in as root on the worker. It gives unrestricted root access, so:

- It is kept on Toby's PC only, in `~/.ssh/cloud-worker-root.pem`, readable by Toby only (guide step 0.1). It is never copied to the worker, Jenkins or any git repository, and never pasted anywhere.
- It is used once, for guide step 1, after root's OK, and not again during the investigation.
- After the work, root decides whether Toby deletes it, and whether root removes the key from `/root/.ssh/authorized_keys`.

| Item | Value |
|---|---|
| Fingerprint (guide step 0.2) | `2048 SHA256:QHhVAmPVaTRus4W2bIv1v5ekDbwRct4041vLrXOJfuc no comment (RSA)` |
| Comment in `/root/.ssh/authorized_keys` (step 6.1) | `skp-t4n2qiu4tscnr8ui08sf`. The key file on Toby's PC has no comment; this label is on the server |
| Root's OK to use it for step 1 (who, when) | Not recorded in this chat. Toby started step 1 at 11:55 |
| Used for step 1 (from, to) | 15 Oct 11:55:56 to 12:03. Logged out with `exit` |
| Rule installed, and `sudo -l -U toby` checked (time) | 15 Oct, about 12:01. Exactly the 17 commands, each as `(root)`, and nothing else |

### 4.4 Rules Toby follows

1. Only the commands in the guide, copied exactly, one step at a time, from his own SSH terminal.
2. sudo only interactively. Never `sudo -i`, `sudo su` or `sudo bash`, and never sudo from cron, Jenkins, `nohup` or a background job. The rule would refuse the first three anyway.
3. If a command fails, or its result differs from the guide's "Expected", stop and ask. No improvising, and no "quick fix".
4. **Never run these during the investigation, even though some don't need sudo:**
   - `rm`, `mv` or `chmod` on anything under `/data`;
   - `docker rmi`, `docker image rm`, `docker system prune`, `docker image prune` or `docker builder prune`;
   - `docker pull`, `docker load`, `docker tag`, `docker stop` or `docker rm` on other users' images and containers;
   - `ctr … rm`, `ctr … remove`, `ctr … delete` or `ctr … prune`;
   - `systemctl stop`, `restart` or `kill` for `containerd`, `docker` or `data.mount`;
   - editing `/etc/fstab`, any file in `/etc/containerd`, `/etc/docker` or `/etc/systemd`, or mounting or unmounting `/data`.
5. Other users' data is read only as far as the guide says. Anything more needs root's and the owner's agreement.

### 4.5 How root can follow

- **Every sudo command** is in the journal: `journalctl _COMM=sudo --since today --no-pager` lists them, one line each, with the user and the command.
- **Every session** is recorded with `script` (guide step 2.3). The recordings are in `~toby/snapshot-audit/sessions/`, and root can read them. Passwords aren't shown on screen, so they aren't recorded.
- **What was done, and when,** is in the investigation log (section 6).

## 5. What the investigation must answer

### 5.1 The questions

| # | Question | Why it matters | Guide step | Evidence (in `~/snapshot-audit/`) |
|---|---|---|---|---|
| Q1 | At the 12 Oct boot, did containerd start before `/data` was mounted? | Confirms section 3.1, steps 1 and 2. Already seen on 15 Oct | 4.1 | `logs/boots.txt` |
| Q2 | Who wrote the quoted fstab line, and when? Who mounted `/data` at 10:54? | Shows which step failed: writing the line, checking it, or mounting under a running service | 4.3, 6.1, 6.2 | `logs/logins.txt`, `logs/root_logins_by_day.txt`, `logs/around_1012.txt`, `logs/root_keys.txt`, `logs/root_history.txt` |
| Q3 | Is the empty store from 12 Oct 10:38 still under the `/data` mount point? | Direct proof of the split brain (section 3.1, step 2) | 6.4 | `logs/hidden_store.txt` |
| Q4 | When did containerd switch back to the real database on 12 Oct, and who restarted it? | Bounds the window in which folders were deleted | 4.4 | `logs/day_1012.txt` |
| Q5 | Did the 11 Sep boot also run containerd without `/data`? | Would mean an earlier split brain | 4.1, 5.3, 5.3b | `logs/boots.txt`, `inventory/low_numbered_dirs.txt`, `inventory/snapshot_dirs.txt` |
| Q6 | When did each kind of error first appear in the logs? | Dates the older damage, so the other logs can be read around that time | 3.2, 7.1 | `logs/damage_first_seen.txt` |
| Q7 | Do the missing downloaded files share an age? | Old files only means an age-based clean-up; files from 11 and 12 Oct only means the restart; no pattern rules out both | 5.2 | `inventory/blob_ages.txt` |
| Q8 | Did anyone delete files under `/data` by hand, or run a clean-up from a crontab or a user timer? | The other candidate for the missing files | 4.3, 4.6, 6.2, 6.3 | `logs/root_history.txt`, `logs/crontabs.txt`, `logs/user_timers.txt`, `logs/sudo_commands.txt` |
| Q9 | When did the host run out of memory or stall on disk writes, what was killed, and was there an earlier episode, for example on 6 and 7 Oct? | The 12 Oct restart came from it, and an earlier one could explain the 6 and 7 Oct damage | 4.2 | `logs/kernel.txt`, `logs/oom_per_day.txt`, `logs/oom_victims.txt`, `logs/hung_tasks.txt` |
| Q10 | What wrote 15 GB to syslog on 4 to 11 Oct? | It may be the same event, seen from another side | 3.2, 7.1 | `logs/syslog_volume.txt` |
| Q11 | Who or what restarted containerd on 14 Oct at 11:11 and 17:36? | A restart doesn't delete data, but an unexplained one means someone works on the store without telling the others | 4.5 | `logs/restarts_1014.txt` |
| Q12 | Do the two containers created during the split brain (folders 37 and 54) exist in the real database? | If not, they will fail at their next start, and their owners should know | 6.5 | `logs/split_brain_containers.txt` |

Added after step 8.2, from the analysis of the PC copy:

| # | Question | Why it matters | Guide step | Evidence (in `~/cloud-worker-evidence/`) |
|---|---|---|---|---|
| Q13 | When were the missing downloaded files lost, and which kind of file was lost? | Dates the loss from the files themselves, and splits it into separate events | 9.1, 9.1b | `analysis/summary.txt`, `analysis/files_by_age.txt`, `analysis/hex_spread.txt` |
| Q14 | What was lost before 13 Sep, and how? | A second loss with its own cause, which a repair alone would not prevent | 9.1, 9.2, 9.3, 9.6 | `analysis/missing_first_reported.txt`, `analysis/folders_by_date.txt` |
| Q15 | How often did containerd start without `/data`, when was each store created, and when was the hidden database in use? | Every such start, followed by a mount, is a split brain | 9.1c, 9.2, 9.3 | `analysis/split_brain_window.txt`, `logs/reboots.txt`, `logs/store_history.txt`, `logs/db_record_days.txt` |
| Q16 | Does the hidden August database list the downloaded files that survived? | If it does, the split brains deleted the files too, and Q13 is explained | 9.1c, 9.3, 9.4 | `analysis/split_brain_window.txt`, `inventory/hidden_db_digests.txt`, `analysis/hidden_db_match.txt` |
| Q17 | Which kind of layer damage did each loss leave, and were the stale records first seen on 6 and 7 Oct new on those days? | Shows which cleanup removed the layers, and whether 6 and 7 Oct need a cause of their own | 9.1d | `analysis/layer_errors.txt` |

The aim is to find which step of the process failed, not to blame a person.

### 5.2 Ask first: the fastest answers

Several questions are answered fastest by asking. Toby asks root these today, in writing:

1. Was the host restarted on 12 Oct at about 10:37, from the Alibaba Cloud console or otherwise? By whom, and why? The console's instance history shows it. (Q9)
2. Who added the `/data` line to `/etc/fstab`, and when? (Q2)
3. Who mounted `/data` at 10:54 and rewrote fstab at 12:09? Was containerd stopped first? (Q2, Q4)
4. Has anyone deleted files in `/data/containerd` or `/data/docker`, for example to free space? When? (Q8)
5. Who restarted containerd on 14 Oct at 11:11 and 17:36? (Q11)
6. When was the 14 TB disk attached and first mounted on `/data`? Was the store moved or copied, or old files or folders deleted to free space? The host booted on 4 Aug at 20:44 and twice on 27 Aug (20:25 and 21:35), and then not again until 12 Oct 10:38. Who mounted `/data` after the 27 Aug 21:35 boot, and was containerd stopped first? (Q14, Q15)

And xu, about the scripts in `/home/xu/check_docker/` that root ran:

1. What do the scripts run with `--apply` change in the store? Do they delete or re-create images, layers or downloaded files? (Q8, Q14)
2. What did `evidence/20261007/missing-targets.txt` list, and when was it made? (Q14)

And the users of the docker group, in the team chat:

1. Did any job use a lot of memory on 11 Oct from 15:00? (Q9)
2. Does anyone run a clean-up script or service that deletes files under `/data`? (Q8)
3. Whose are the two split-brain containers, `clarify-selection4-runtime-v3-20261012` and the Harbor sidecar of `bandit-incremental-cache-control`? (Q12)

The logs then confirm or correct the answers.

| Question | Answer | From, when |
|---|---|---|
| Root 1: restart on 12 Oct | | |
| Root 2: the fstab line | The retained root history has the one-liner twice (lines 303 and 469): copy fstab to `fstab.bak`, mount `/dev/nvme1n1p1` on `/data`, and append `"/dev/nvme1n1p1 /data ext4 defaults 0 0"` with the quotes included. No timestamp, so the history does not date it | `logs/root_history.txt`, 15 Oct 14:01 |
| Root 3: mount at 10:54, fstab at 12:09 | The same one-liner is the mount. `fstab.bak` saved at 10:54:02, and the kernel's mount at 10:54:05, match its `cp` and `mount` to the second, so the 10:54 run is very likely line 469. The 12:09 rewrite to the UUID line, and the 11:48 stop, are not in this history | `logs/root_history.txt`, `logs/kernel.txt`, 15 Oct 14:01 and 15:30 |
| Root 4: deletes in `/data` | No store deletion in the retained history, and no crontab or user timer is named clean, prune or gc. Still open: what xu's `--apply` scripts did | `logs/root_history.txt`, `logs/crontabs.txt`, `logs/user_timers.txt`, 15 Oct 14:12 |
| Root 5: containerd restarts on 14 Oct | | |
| Users 1: memory on 11 Oct | | |
| Users 2: clean-up scripts | No crontab line deletes the store. zihan runs `capbench_linux_sync.py` from `/data` every 6 hours. Toby's hourly canary uses `docker run --rm`, which removes that one container. Three user timers, none named clean, prune or gc | `logs/crontabs.txt`, `logs/user_timers.txt`, 15 Oct 14:12 |
| Root 6: the disk, the store, 4 Aug, 27 Aug, 10 to 13 Sep | | |
| xu 1 and 2: the `--apply` scripts, `missing-targets.txt` | | |
| Users 3: the two split-brain containers | | |

## 6. Investigation log

One row per guide step: when it was done, the result in one line, and where the evidence is. "Sent" means the step's output was shared for review.

| Guide step | What | Done (date, time) | Result | Evidence | Sent |
|---|---|---|---|---|---|
| (before) | Read-only checks of 14 and 15 Oct (Appendix A) | 14 Oct; 15 Oct 10:48 to 11:00 | Sections 2 and 3 | `facts.txt`, `base_status.tsv`, `deepswe_status.tsv`, `cause_checks.txt` | yes |
| 0.1 | `root.pem` moved to `~/.ssh`, mode 600 | 15 Oct, before 11:54 | `-rw------- … /home/Toby/.ssh/cloud-worker-root.pem` | the file itself | yes |
| 0.2 | Key fingerprint | 15 Oct, before 11:54 | `2048 SHA256:QHhVAmPVaTRus4W2bIv1v5ekDbwRct4041vLrXOJfuc no comment (RSA)` | section 4.3 | yes |
| 0.3 | Test login as root | 15 Oct 11:54:36 | `root`, `iZt4ndd2dff7gqjta7mppaZ` | terminal output | yes |
| 1.1 to 1.6 | Root shell, password, rule written, checked, installed, and listed | 15 Oct 11:55 to about 12:01 | `passwd: password updated successfully`; `parsed OK`; `sudo -l -U toby` lists the 17 commands as `(root)` only | terminal output | yes |
| 1.7 | Automatic removal after 3 days | 15 Oct, about 12:03 | `Running timer as unit: toby-investigate-expire.timer` | terminal output | yes |
| 1.8 | Leave the root shell | 15 Oct, about 12:03 | `logout`, then the prompt of Toby's PC | terminal output | yes |
| 2.1 | Logged in as `toby` | 15 Oct 12:03 | Prompt `toby@iZt4ndd2dff7gqjta7mppaZ:~$` | terminal output | yes |
| 2.2 | Groups and rule checked as `toby` | 15 Oct, about 12:05 | Groups `adm`, `docker`, `systemd-journal`. `sudo -l` lists the 17 commands. `sudo true` refused: `I'm sorry toby. I'm afraid I can't do that` | terminal output | yes |
| 2.3 | Recording started | 15 Oct 12:05 | Prompt changed to `~/snapshot-audit/logs` with no other output | `sessions/2026-10-15_1205.log` | yes |
| 2.4 | Scripts saved | 15 Oct, about 12:10 | `scripts OK` | `long_scans.sh`, `layer_test.sh` | yes |
| 3.1 | Logs copied | 15 Oct, finished 12:38 | 165 GB free. 12 files, 3.3 GB: three `syslog`, three `auth.log` and three `kern.log` archives, `auth.log.1`, `kern.log.1`, and the journal export (341 MB). The 15 GB `syslog.1` was left in place, as planned | `logcopy/` | yes |
| 3.2 | Slow log scans started | 15 Oct 12:42:12, process 3899174 | Finished at 12:45:28. Q6 at 12:44:43, Q10 at 12:45:28. awk warned about one non-text line in `syslog.1` and continued | `logs/long_scans.log`, `logs/damage_first_seen.txt`, `logs/syslog_volume.txt` | yes |
| 3.3 | Image list; start test started | 15 Oct, about 12:46, process 3954694 | 7,750 images (7,089 on 14 Oct). Start test running: 1,043 of 7,750 done in the first 3 minutes | `inventory/images_list.txt`, `inventory/layer_test.log` | yes |
| 4.1 | Boots (Q1, Q5) | 15 Oct 12:54 | Three boots: 24 Jul, the 11 Sep boot (journal now starts 17 Sep), and 12 Oct 10:38. Boot 0 confirms Q1. Boot -1 ends 11 Oct 15:24 with no shutdown | `logs/boots.txt` | yes |
| 4.2 | Memory, stuck processes, disk (Q9) | 15 Oct, about 13:01 | OOM kills: 1 on 18 Sep, 20 on 11 Oct, 6 on 12 Oct. None on 6 or 7 Oct. No disk errors. Disk stalls on 14-15 Sep, 18 Sep, 21 Sep, and 14-15 Oct | `logs/kernel.txt`, `logs/oom_per_day.txt`, `logs/oom_victims.txt`, `logs/hung_tasks.txt`, `logs/journald_watchdog.txt` | yes |
| 4.3 | Logins and sudo (Q2, Q8, Q11) | 15 Oct, about 13:13 | 262,506 SSH logins, 5 sudo commands. Two root keys. On 12 Oct the active key is the same one as `root.pem`. No sudo command edited fstab or mounted `/data` | `logs/logins.txt`, `logs/sudo_commands.txt`, `logs/root_logins_by_day.txt`, `logs/around_1012.txt` | yes |
| 4.4 | 12 Oct in detail (Q4) | 15 Oct, about 13:17 | Services stopped 11:48:51. One-second containerd start at 12:54:32. Lasting start at 13:07:52; Docker running at 13:09:24. No systemd "Mounted" line | `logs/day_1012.txt` | yes |
| 4.5 | 14 Oct restarts (Q11) | 15 Oct, about 13:22 | Three restarts, not two: 11:11:31, 17:36:00 and 20:08:52. Each stop left the container shims running. A root login from 178.15.100.199 at 11:01:59, nine minutes before the first. No root login in the 20 minutes around 17:36 | `logs/restarts_1014.txt` | yes |
| 4.6 | Toby's own timers and crontab (Q8) | 15 Oct, about 13:24 | One timer, Ubuntu's `launchpadlib-cache-clean`, which cleans a package cache. The only crontab line is the hourly image canary | session recording | yes |
| 5.1 | containerd's lists | 15 Oct 13:26 | 7,750 images: 507 complete and unpacked, 27 complete but not unpacked, 7,216 in error. 19,581 layer records and 10,164 layer folders. 46,179 file records and 3,361 files. No download in progress | `inventory/images_check.txt`, `inventory/snapshots_db.txt`, `inventory/content_records.txt`, `inventory/content_active.txt`, `inventory/snapshot_dirs.txt`, `inventory/blob_files.txt` | yes |
| 5.2 | Records against disk; ages (Q7) | 15 Oct, about 13:32 | 42,915 file records have no file. 97 files are on disk but not recorded. Missing and present files share the same ages, so this was not an age-based deletion and not only the 12 Oct restart | `inventory/blobs_missing.txt`, `inventory/blobs_unrecorded.txt`, `inventory/blob_ages.txt` | yes |
| 5.3 | Low-numbered folders (Q5) | 15 Oct, about 13:37 | The count was 0. The guide expected folders 37 and 54 from 12 Oct, so this was not an answer on its own | `inventory/low_numbered_dirs.txt` | yes |
| 5.3b | Column check of `snapshot_dirs.txt` (Q5) | 15 Oct, about 13:39 | Field 9 is the folder name. Zero folders are numbered under 1000. The lowest folder still on disk is 152997, dated 2026-09-10. Folders 37 and 54 from the 12 Oct empty database are gone | session recording; `inventory/snapshot_dirs.txt` | yes |
| 6.1 | Root's keys (Q2) | 15 Oct, about 13:54 | Two keys. `SHA256:Sw9bNRuC…` comment `zihanwu@MacBook-Pro ali-server-root` (ED25519). `SHA256:QHhVAmPV…` comment `skp-t4n2qiu4tscnr8ui08sf` (RSA), the same key as `root.pem` | `logs/root_keys.txt` | yes |
| 6.2 | Root's history (Q2, Q8) | 15 Oct, about 14:01 | 247 matching lines. No timestamps. The quoted fstab line and the `/data` mount are one command, saved twice. No `rm`, `prune` or `find -delete` of the store. The 12:09 UUID rewrite is not in this file. xu's Docker scripts from `/home/xu/check_docker/` were run as root, including `--apply`, and one input path is `evidence/20261007/missing-targets.txt` | `logs/root_history.txt` | yes |
| 6.3 | Crontabs and user timers (Q8) | 15 Oct, about 14:12 | Two matching crontab lines: zihan's `capbench_linux_sync.py` every 6 hours under `/data`, and Toby's hourly canary. No timer named clean, prune or gc. zihan has three timers, the newest from 6 and 8 Oct. yun has no unit file in the filtered list. naiyu has `factor-mining-loopx-only-002.service` | `logs/crontabs.txt`, `logs/user_timers.txt` | yes |
| 6.4 | The hidden store (Q3) | 15 Oct, about 14:15 | The view was read-only, then removed. Hidden `/data` is 1.7 GB. `containerd` dates from 4 Aug 2026. `meta.db` is 865 MB, last written 12 Oct 11:48:43. `docker` was created 12 Oct 10:39:46. `hanxu` was created 12 Oct 10:38:15 | `logs/hidden_store.txt` | yes |
| 6.5 | The split-brain containers (Q12) | 15 Oct, about 14:19 | Both exited, and neither is in the real snapshot database. `f870b22117e6` is `/clarify-selection4-runtime-v3-20261012` (`docker:29-dind`). `6afbbec81e30` is a Harbor egress sidecar for `bandit-incremental-cache-control`. Folders 37 and 54 are gone | `logs/split_brain_containers.txt` | yes |
| 7.1 | Slow log scans (Q6, Q10) | 15 Oct, about 14:23 | Same result as the 12:45 scan. First `blob not found` 13 Sep 01:37, `missing parent` 14 Sep 12:05, `no space left` 18 Sep 17:27, `failed to stat parent` 14 Oct 17:01, `already exists` never. `dockerd` wrote 22,284,226 lines. Syslog rose from 41 MB on 5 Oct to 4,568 MB on 9 Oct, then 55 MB on 11 Oct. The awk warning is one non-text line; the scan finished | `logs/long_scans.log`, `logs/damage_first_seen.txt`, `logs/syslog_volume.txt` | yes |
| 7.2 | Start test (impact) | 15 Oct, about 14:30 | 254 ok, 6,313 blob_missing, 253 layers_missing, 27 stale_record. Those four counts sum to 6,847; the image list is 7,750, and the script skips `sha256:` and `moby-dangling@` lines. All 113 DeepSWE images are layers_missing. 158 containers were created from a broken image | `inventory/layer_test.tsv`, `inventory/broken_by_repo.txt`, `inventory/containers_on_broken.tsv` | yes |
| 8.1 | Recording stopped | 15 Oct, about 14:34 | `eixt` was typed first and did nothing. `exit` then left the recording. The prompt returned at `~/snapshot-audit/logs`. The recording is in `sessions/` | `sessions/2026-10-15_1205.log` | yes |
| 8.2 | Evidence copied to Toby's PC | 15 Oct, about 14:40 | `logs/` 1.4 GB, of which `damage_errors.txt` is 1,286 MB. `inventory/` 50 MB. Together about 1.45 GB, in `~/cloud-worker-evidence/` | `~/cloud-worker-evidence/` on Toby's PC | yes |
| 8.3 | Access removed by root | | | | |
| 9.1 | Analysis of the PC copy (Q13, Q14) | 15 Oct, about 15:28 | Files recorded after the restart: 0 missing, 634 present. Recorded 2 to 12 Oct: 5,791 missing, 4 present. Made on this host, before the restart: 41,170 missing, 80 present; pulled: 1,745 missing, 2,550 present. The log names 930 missing files: 586 first before 12 Oct 10:54, all 2 months old; 344 after, all from September or October. 57 missing folders named by the start test, 153149 to 159017, among folders of 10 Sep. 7,826 folders dated before the 12 Oct boot. The lowest folder is 152997 (10 Sep 09:22) | `analysis/summary.txt`, `analysis/files_by_age.txt`, `analysis/missing_first_reported.txt`, `analysis/errors_per_day.txt`, `analysis/folders_by_date.txt` | yes |
| 9.1b | Order of the cleanup (Q13) | 15 Oct, 15:43 | No cut-off: 5.3 to 6.5 % of the files recorded before the restart are present in each of the 16 first-digit groups, pulled and made-here alike | `analysis/hex_spread.txt` | yes |
| 9.1c | The split-brain hour in the logs (Q13, Q15, Q16) | 15 Oct, 15:52 | Layer cleanup failed at 10:41:44, 10:43:01 and 10:43:28 (`bucket does not exist`). Pulls at 11:20:47 and 11:23:41, a build at 11:44:14. 786 images listed from the hidden database: 721 not in the real database, 65 in it with all files present. The 14 TB disk mounted only at 12 Oct 10:54:05 between 13 Sep and 15 Oct; grown on 21 Sep and 5 Oct | `analysis/split_brain_window.txt` | yes |
| 9.1d | The two kinds of layer error (Q14, Q17) | 15 Oct, about 16:15 | `missing parent`: 221 lines, 14 Sep to 14 Oct, the first on 14 Sep 12:05. `failed to stat parent`: 59 lines, all on 14 Oct. `alpine:3.20`'s missing parent first failed on 30 Sep, `alpine:latest`'s on 29 Sep | `analysis/layer_errors.txt` | yes |
| 9.2 | Every boot and shutdown in `wtmp` (Q15) | 15 Oct, about 17:49 | No boot lines. `/var/lib/wtmpdb` does not exist, and `last` is not installed. The two `apt install wtmpdb` lines are the shell's suggestion. Nothing was installed | `logs/reboots.txt` | no, see 9.2b |
| 9.2b | The same records, read with Python (Q5, Q15) | 15 Oct, about 17:52 | 1,029 records from 24 Jul 15:15:32 to 15 Oct 17:48:09: 1,019 terminal logins and 10 prompts. No boot line. On 4 Aug, 27 Aug and 12 Oct the first login after each boot is on `pts/0`, 8 to 77 s later. On 11 Sep the next terminal was `pts/13`, so sessions from before 09:14 were still open | `logs/reboots_wtmp.txt` | yes |
| 9.2c | Login prompts and console logins: the boots (Q5, Q15) | 15 Oct, about 17:52 | Five boots, each a prompt on `tty1` and `ttyS0`: 24 Jul 15:15:32, 4 Aug 20:44:37, 27 Aug 20:25:23, 27 Aug 21:35:55, 12 Oct 10:38:21. No boot on 11 Sep, and none between 27 Aug 21:35 and 12 Oct 10:38. No console login | `logs/reboots_console.txt` | yes |
| 9.3 | Store history and the hidden database's file list, by root (Q15, Q16) | 16 Oct | Read-only, with `root.pem`, and the view was removed. Old `meta.db` last written 12 Oct 11:48:43. Old layer database born 27 Aug, 0 snapshot folders. 14 TB filesystem created 24 Jul 15:54, mount count 4, last mount 12 Oct 10:54. Record days: old database 4 Aug–10 Sep and 12 Oct; 14 TB database 24 Jul–4 Aug, then from 10 Sep, and both on 12 Oct. 180,168 digests in the old file | `logs/store_history.txt`, `logs/db_record_days.txt`, `inventory/hidden_db_digests.txt` | yes |
| 9.4 | Hidden database against the files on disk (Q16) | 16 Oct | Before 2 Oct: 2,624 of 2,626 files still on disk are named in the old database; 36,969 of 37,124 missing are not. 2–12 Oct: 5,791 missing, none named. The cleanup kept what the old database listed | `analysis/hidden_db_match.txt` | yes |
| 9.5 | Docker's 22 million log lines of 4 to 11 Oct (Q10) | 15 Oct, finished; SSH then dropped | 20,275,470 lines are `checking availability of platform specific manifest failed`. 1,346,163 are `failed to check image labels`. `image pulled` is 9. The flood is re-checks of images whose files are already missing | `logs/dockerd_messages.txt` | yes |
| 9.6 | Questions to root, xu and the users, in writing (Q2, Q8, Q11, Q12, Q14) | | | | |
| 9.7 | `logcopy/` and `sessions/` copied to the PC | 16 Oct | `logcopy` 3.3 GB, `sessions` 164 KB, `logs` 1.4 GB. The PC had 530 GB free | `~/cloud-worker-evidence/` | yes |
| 9.8 | Does the fstab line parse, without a reboot | 16 Oct, as `toby` | `0 parse errors, 0 errors, 3 warnings`. The warnings are permission denied on `/`, `/boot/efi` and `/data`. `/data` is `/dev/nvme1n1p1` ext4, read-write. The line is valid while the machine is up. No reboot has tested it | terminal output | yes |

Anything that didn't go as the guide expected:

| Time | Step | What happened | What was decided |
|---|---|---|---|
| 15 Oct, about 12:00 | 1.5 | After `rm -f` (which printed nothing, as expected), the letter `z` was typed. The shell answered `z: command not found` | Harmless. No file was changed. Continue with 1.7 |
| 15 Oct 11:55 | 1.1 | The login banner reported 366 zombie processes, and the previous root login was 15 Oct 10:15:39 from 119.8.240.30 | Noted. The zombies are not part of this investigation. The earlier root login is a lead for Q2, to be matched in step 4.3 |
| 15 Oct, about 13:37 | 5.3 | `low_numbered_dirs.txt` is empty. The guide expected folders 37 and 54 from 12 Oct | Step 5.3b, run at 13:39, showed field 9 is the folder name. The empty list is real. The lowest folder is 152997. Q5 cannot be answered from these folders |
| 15 Oct, about 14:34 | 8.1 | `eixt` was typed. The shell answered `eixt: command not found` | Harmless. The following `exit` stopped the recording. No file was changed |

## 7. Investigation result

The read-only steps are finished. Status is confirmed, ruled out or open.

### 7.1 Answers

| # | Answer | Evidence | Status |
|---|---|---|---|
| Q1 | Recorded. On 12 Oct containerd started at 10:38:15, timed out at 10:39:45, and started at 10:40:15. Docker started at 10:42:54. `/data` was mounted at 10:54:05, so both were already running | `logs/boots.txt` | confirmed |
| Q2 | Partly answered. Root logins use two keys, and step 6.1 names both. `SHA256:QHhVAmPV…` is the same key as `root.pem`. Its comment on the server is `skp-t4n2qiu4tscnr8ui08sf`, a key-pair id, not a person's name. It was used on almost every day since 14 Sep, including all 8 root logins in the 12 Oct repair window. `SHA256:Sw9bNRuC…` has the comment `zihanwu@MacBook-Pro ali-server-root`. It was used heavily on 29-30 Sep and 8-10 Oct, and not on 12 Oct. On 12 Oct: four logins at 10:38:40-47 from 178.15.100.199, then 10:41, 10:46, 10:50 and 11:28 from 119.8.178.37. No root login on 11 Oct, and none at the 12:54 containerd start, so that start came from a session already open. No sudo command edited fstab or mounted `/data`. Root's bash history has the command that mounts `/dev/nvme1n1p1` on `/data` and appends the quoted fstab line. It is saved twice, at history lines 303 and 469, with no timestamp, so the history does not say when or who typed it. The 12:09 rewrite of fstab to the UUID line is not in this file. A session that is still open would not have written its commands yet | `logs/around_1012.txt`, `logs/root_logins_by_day.txt`, `logs/sudo_commands.txt`, `logs/root_keys.txt`, `logs/root_history.txt` | partly confirmed |
| Q3 | Confirmed, with a correction. A store is hidden under the `/data` mount, and the view was removed afterwards. It is not a database created empty at 10:38. The `containerd` directory dates from 4 Aug 2026. Its `meta.db` is 906,985,472 bytes and was last written at 12 Oct 11:48:43, eight seconds before containerd stopped, so containerd held that file open through the 10:54 mount. The hidden `docker` directory was created at 10:39:46, and `hanxu` at 10:38:15. The hidden tree is 1.7 GB. The real disk's database is the one containerd opened again at 13:07 | `logs/hidden_store.txt` | confirmed |
| Q4 | containerd ran on the empty store from 10:40:15 until both services were stopped at 11:48:51. It started for one second at 12:54:32 and was stopped at 12:54:34. The lasting start is 13:07:52, and Docker was running at 13:09:24. `/data` was already mounted, and this start took one second, so it opened the real database. No new root login at these times, so an already-open root session did it. The journal has no systemd "Mounted" line; the 10:54 mount is in the kernel log, which fits a manual mount | `logs/day_1012.txt` | confirmed |
| Q5 | The retained journal for the 11 Sep boot has no containerd or `/data` lines, and its oldest remaining entry moved from 11 Sep (seen at 10:48 today) to 17 Sep. It ends 11 Oct 15:24 with no shutdown. No layer folder numbered under 1000 is still on disk. The lowest is 152997, dated 2026-09-10, and the 12 Oct empty-database folders 37 and 54 are gone as well. Their absence does not show whether an earlier split brain happened, because those folders would have been removed once the real database was back. Images pulled on 10 Sep worked on 8 Oct. Added at 15:30: fstab had no valid `/data` line before 12 Oct 12:09, unless one was removed in between, because the 12 Oct boot found none. So `/data` could not have been mounted from fstab at the 11 Sep boot either; it was mounted by hand. Whether containerd was stopped first is not known. Added at 17:52: there was no boot on 11 Sep. The login prompts (step 9.2c) give five boots, and no others: 24 Jul 15:15:32, 4 Aug 20:44:37, 27 Aug 20:25:23, 27 Aug 21:35:55 and 12 Oct 10:38:21. The host stayed up from 27 Aug 21:35 until 12 Oct 10:38. 09:14 was the oldest journal line still kept, not a boot | `logs/boots.txt`, `logs/reboots_console.txt`, `inventory/low_numbered_dirs.txt`, `inventory/snapshot_dirs.txt` | confirmed |
| Q6 | In the retained daemon log: `blob not found` on 13 Sep 01:37 (the oldest log day, so it may be older), `missing parent` on 14 Sep 12:05, `no space left` on 18 Sep 17:27, `failed to stat parent` on 14 Oct 17:01. `already exists` does not appear. The 14 Oct line is the missing-layer failure. The September lines show those other errors were already happening weeks before the 12 Oct restart | `logs/damage_first_seen.txt`, `logs/long_scans.log` | confirmed |
| Q7 | 42,915 of 46,179 file records have no file, and 97 files are on disk but not recorded. Missing files are 4 days to 2 months old. Present files cover the same range, including 1,861 files that are 2 months old. Files from the last 2 days are almost all present. Files aged 2 to 3 weeks are almost all missing (10,457 missing and 73 present at 2 weeks; 5,613 missing and 13 present at 3 weeks), while older files survived more often. An age-based deletion would have removed the older files first, and the 12 Oct restart would have removed only files a few days old. Neither fits. Steps 6.2 and 6.3 found no hand delete and no scheduled job named as a cleanup. xu's `--apply` scripts remain a lead. Added at 15:30: this table mixes two losses, which step 9.1 separates (Q13, Q14) | `inventory/blob_ages.txt` | confirmed |
| Q8 | Partly answered. The retained root history has no `docker prune`, no `rm` of the store, and no `find -delete`. The only `rm -rf` is `/home/zihan/lhs`. The only `find` on `/data` changes directory permissions. No crontab line deletes the store. zihan's crontab runs `/data/capbench-linux-sync/bin/capbench_linux_sync.py` every 6 hours; the line itself is not a prune. Toby's hourly canary uses `docker run --rm`, which removes only that container. The filtered user units list naiyu's `factor-mining-loopx-only-002.service` and zihan's services, including the expected `contextgraph-release-…` units. yun has no unit file in that list. The only timers are `cg-verified-review.timer` (5 Sep), `jiuwen-tb4-experience-20261006.timer` (6 Oct) and `jiuwen-tb4-experiment-experience-pilot6-r3.timer` (8 Oct). None is named clean, prune or gc. Still open: what xu's `--apply` scripts did | `logs/root_history.txt`, `logs/crontabs.txt`, `logs/user_timers.txt` | partly confirmed |
| Q9 | OOM kills: 1 on 18 Sep, 20 on 11 Oct, 6 on 12 Oct. Victims are `java`, `python`, `python3`, `opencode`, `rg` and `harbor`, never containerd or Docker. The journal failed repeatedly on 11 Oct from 01:47 to 15:36. Disk stalls on 14, 15, 18 and 21 Sep, and from 14 Oct 13:00 to 15 Oct 00:00. No OOM kill and no disk stall on 6 or 7 Oct. No `I/O error`. The 12 Oct boot recovered both disks. Still open: who restarted the host | `logs/oom_per_day.txt`, `logs/oom_victims.txt`, `logs/hung_tasks.txt`, `logs/journald_watchdog.txt`, `logs/kernel.txt` | confirmed, except who restarted it |
| Q10 | `dockerd` wrote 22,284,226 of the lines in `syslog.1`. Volume jumped from 41 MB on 5 Oct to 506 MB on 6 Oct, and peaked at 4,568 MB on 9 Oct. 11 Oct was only 55 MB. Step 9.5 counted them: 20,275,470 lines are a failed platform-manifest check, and 1,346,163 are a failed image-label check. The flood is re-checks of images whose files are already missing. `image pulled` is 9 | `logs/syslog_volume.txt`, `logs/dockerd_messages.txt` | confirmed |
| Q11 | Three restarts on 14 Oct: 11:11:31 (back at 11:11:33), 17:36:00 (back at 17:36:01) and 20:08:52 (back at 20:09:22). Each was a clean stop. The "unclean termination" lines are the container shims, which stay running on purpose, so the containers kept running. Root key `SHA256:QHhV…` (comment `skp-t4n2qiu4tscnr8ui08sf`) logged in from 178.15.100.199 at 11:01:59. No root login in the 20 minutes around 17:36. The 20:08 restart is just before the disk stalls at 20:21. Those restart commands are not in the retained root history | `logs/restarts_1014.txt`, `logs/root_keys.txt`, `logs/root_history.txt` | partly confirmed |
| Q12 | Neither container is in the real snapshot database (0 records each), and both are exited. `f870b22117e6` is named `/clarify-selection4-runtime-v3-20261012` and uses `docker:29-dind`. `6afbbec81e30` is named `/bandit-incremental-cache-control__rzkxcbu__env-harbor-docker-egress-control-sidecar-1` and uses the Harbor egress sidecar image. Folders 37 and 54 are not on disk. A next start of either container will fail, and the owners of those two names should be told | `logs/split_brain_containers.txt` | confirmed |
| Q13 | Most of the loss falls in the 12 Oct window. Files recorded after the restart: 0 missing, 634 present. Recorded 2 to 12 Oct, before the restart: 5,791 missing, 4 present. Recorded before 2 Oct: 37,124 missing, 2,626 present. So files written on 10 and 11 Oct were deleted after they were written, and nothing written after 13:07 on 12 Oct was. Made on this host (no registry label), before the restart: 41,170 missing, 80 present. Pulled from a registry: 1,745 missing, 2,550 present. All of DeepSWE's ECR files are present | `analysis/summary.txt`, `analysis/files_by_age.txt` | confirmed |
| Q14 | An earlier loss happened. The error log names 930 of the missing files. 586 were first reported before 12 Oct 10:54 (532 of them on 14 to 16 Sep), and all 586 are 2 months old (463 made here, 123 pulled). The 344 first reported later are all from September or October. No layer folder created before 10 Sep 09:22 is on disk, although 6,338 file records are 2 months old. No file record is 6 to 8 weeks old: none was created, or none is kept, from about 16 Aug to 3 Sep. Most likely cause, added at 16:00: an earlier split brain (Q15). The 14 TB disk wasn't mounted again between 13 Sep and 12 Oct, so it happened before 13 Sep | `analysis/missing_first_reported.txt`, `analysis/folders_by_date.txt`, `analysis/files_by_age.txt`, `logs/damage_first_seen.txt`, `logs/kernel.txt` | likely |
| Q15 | Partly answered. The hidden store was created on 4 Aug at 20:44:35 (all its folders within one second) and has no downloaded-files folder. Its layer database folder changed on 27 Aug at 20:26:32, and containerd used it again on 12 Oct. Each date may be a start without `/data`. Added at 16:00: containerd had used the hidden database before 12 Oct, with the real disk mounted on top. It held at least 786 images, 721 of which the real database doesn't record. No file was ever written to the hidden store (its `containerd` folder is unchanged since 4 Aug 20:44:36), so their files went to the real disk. 44 are from DeepSWE's registry, pulled in September, so it was used after that. Added at 17:52: the use was not after an 11 Sep boot, because the host did not boot between 27 Aug 21:35 and 12 Oct 10:38. The 4 Aug 20:44:37 boot created the hidden store (20:44:35), and the 27 Aug 20:25:23 boot is 69 seconds before the layer folder changed. A second boot the same evening, 21:35:55, is not in the journal. Its layer database was empty on 12 Oct, which doesn't fit yet (section 3.2). Added at 16:20: the layer errors fit one explanation, a hidden main database working with the real layer database (Q17) | `logs/hidden_store.txt`, `analysis/split_brain_window.txt` | likely; dates open |
| Q16 | Answered on 16 Oct. Of 2,626 files recorded before 2 Oct and still on disk, 2,624 are named in the old database. Of 37,124 missing from that period, 36,969 are not named (155 are, 0.4%). Of 5,791 missing from 2 to 12 Oct, none are named. The cleanup kept the files the old database listed and deleted the ones it did not. The earlier log check still stands: of 786 images Docker listed from the hidden database, the real database records 65, and all 65 files are present | `analysis/hidden_db_match.txt`, `analysis/split_brain_window.txt` | confirmed |
| Q17 | Before 12 Oct the only layer error is `missing parent`: 202 lines from 14 Sep to 11 Oct, and 221 up to 14 Oct. The layer database has no record of a parent that the main database lists, so the record went together with its folder. `failed to stat parent`, a folder gone while its record is kept, appears only on 14 Oct (59 lines), after the 12 Oct split brain. `alpine:3.20`, pulled on 24 Jul, fails with a `missing parent` already logged on 30 Sep, and `alpine:latest`'s was logged on 29 Sep. Docker logged no error on 2 to 4 Oct. So these stale records existed before 6 Oct; 6 and 7 Oct are when the builds met them. `already exists` is never logged, so `alpine:3.23.4` can't be dated this way | `analysis/layer_errors.txt`, `analysis/errors_per_day.txt` | likely |

| Inventory | Count | File |
|---|---|---|
| Images in the store | 7,750 (7,089 on 14 Oct) | `inventory/images_list.txt` |
| `images check`: complete and unpacked / complete, not unpacked / error | 507 / 27 / 7,216 | `inventory/images_check.txt` |
| Start test: `ok` / `layers_missing` / `blob_missing` / `stale_record` / `other` | 254 / 253 / 6,313 / 27 / 0 | `inventory/layer_test.tsv` |
| Layer records / layer folders | 19,581 / 10,164 | `inventory/snapshots_db.txt`, `inventory/snapshot_dirs.txt` |
| Downloaded-file records / files on disk / recorded but missing / on disk but not recorded | 46,179 / 3,361 / 42,915 / 97 | `inventory/content_records.txt`, `inventory/blob_files.txt`, `inventory/blobs_missing.txt`, `inventory/blobs_unrecorded.txt` |
| Containers created from a broken image | 158 | `inventory/containers_on_broken.tsv` |

### 7.2 How to decide the cause of the missing downloaded files and the stale records

| What the evidence shows | Conclusion |
|---|---|
| `blob_ages.txt`: every `missing` line is older than every `present` line, for example missing only at "2 months" and older | Deleted by age. The deleting job or person is in Q8's evidence or root's answer |
| `blob_ages.txt`: the missing files are mostly 3 to 4 days old (11 and 12 Oct) | Lost in the forced restart |
| `blob_ages.txt`: missing and present files have the same spread of ages | Neither age nor the restart. Look at `root_history.txt` and root's answer for deletes by hand |
| `oom_per_day.txt` or `hung_tasks.txt` shows out-of-memory kills or hung tasks on 6 or 7 Oct | The stale records of 6 and 7 Oct probably come from an earlier memory crisis or disk stall |
| `damage_first_seen.txt` shows an error type well before 12 Oct | That damage predates the restart; read `logins.txt`, `root_history.txt` and `kernel.txt` around that date |

### 7.3 Conclusions

- Cause of the missing layer folders: confirmed. The quoted fstab line made the 12 Oct boot skip `/data`. containerd opened the August database on the system disk and kept it open. `/data` was mounted at 10:54 underneath that running process, and the cleanup removed real layer folders that the August database does not list.
- Cause of the missing downloaded files: two losses. 6,313 images fail this way.
  - Most files went in the same 12 Oct window (Q13, Q16): nothing recorded after the restart is missing, and nearly everything recorded on 2 to 11 Oct is. Step 9.4 confirmed the keep-list. Of 2,626 files from before 2 Oct still on disk, 2,624 are named in the old database. Of 5,791 missing from 2 to 12 Oct, none are named.
  - An earlier loss before 13 Sep took files recorded before mid-August, and every layer folder older than 10 Sep (Q14). Most likely it was an earlier split brain of the same kind, during the 4 Aug boot or one of the two 27 Aug boots (Q15). The host did not boot on 11 Sep. Step 9.3 dated the records: only the old database was written from 4 Aug through 9 Sep, and the 14 TB database resumed on 10 Sep. Which boot, and who mounted `/data` on 10 Sep, are open and do not block the repair.
  - The retained history and the crontabs show no prune of the store.
- So both losses most likely have one cause: containerd running on the old store on the system disk while the 14 TB disk was mounted on top of it. The broken fstab line made every boot need a manual mount, and each manual mount under a running containerd was a split brain.
- Cause of the stale records first seen on 6 and 7 Oct: most likely the earlier loss (Q17). 27 images fail this way. At least two of them were already failing on 29 and 30 Sep, and the 6 and 7 Oct dates follow days on which Docker logged no error. There was no out-of-memory kill and no disk stall on those days. Still open, and not blocking the repair: the `already exists` kind (`alpine:3.23.4`), and what xu's `--apply` scripts changed. The `dockerd` syslog flood is re-checks, not a third deletion (Q10, step 9.5).
- What started the 12 Oct chain: the host was out of memory on 11 Oct, the journal stopped, and the host restarted at 10:38 on 12 Oct without a clean shutdown. The quoted fstab line then skipped `/data`. Who restarted the host is still open.
- Who and which jobs are affected: the counts that add up, each image once, are in the mentor report, section 9.0. Of 7,750 names, 254 start. Published DeepSWE is 113, all missing the snapshot folder, blobs still on disk. The 42,915 missing downloaded-file records add up to 481 GB. The 14 TB disk was 88% used on 15 Oct, with 1.7 TB free, so it was not wiped. 158 containers were created from a broken image.
- Still open, and not a reason to run another command: who restarted the host; who mounted `/data`; what xu's `--apply` scripts did; who owns the two split-brain containers; which of the three early boots the earlier loss followed; the `already exists` stale records. The record days themselves are dated (steps 9.3 and 9.4).

A proposed repair and prevention plan is in [the mentor report, section 9](CLOUD_WORKER_MENTOR_REPORT.md#9-proposed-solution-needs-the-mentor-s-and-root-s-approval). It is a proposal only. Until the mentor and root approve it, nothing in the store is changed, and mac-k3d jobs run on the local worker (`AGENT_LABEL=linux-Michael-Ubuntu`).

### 7.4 Still unknown

No further command on the worker is part of this investigation. These do not block the repair in the mentor report, section 9. The decisions are in that report, section 10.

| Unknown | What would answer it | Where it sits | Who |
|---|---|---|---|
| Why did the old layer database have 0 snapshot folders, while its database file was written on 27 Aug and again on 12 Oct 11:48? | containerd 2.2's source: does the overlayfs snapshotter open its database at start, or at first use? | mentor report, section 8, check 8. Not a worker command | optional |
| Who mounted `/data` on 10 Sep, when the records moved back to the 14 TB database? Who restarted the host, mounted `/data` at 10:54 on 12 Oct, rewrote fstab at 12:09, and restarted containerd on 14 Oct? | Root's answers; the Alibaba Cloud console | guide step 9.6. No answer is recorded | Root |
| Whose are the two split-brain containers, and what did xu's `--apply` scripts change? | Team chat, and xu | guide step 9.6. No answer is recorded | Toby, xu |

## Appendix A: checks already run as `toby` (no sudo)

All read-only. The 12 Oct chain (sections 2.1 to 2.4):

```bash
# boot, /data mount, fstab history and the guards
uptime -s
systemctl show data.mount -p ActiveEnterTimestamp -p ActiveExitTimestamp
systemctl show emergency.target local-fs.target -p Id -p ActiveEnterTimestamp
systemctl show docker.service -p ActiveExitTimestamp -p InactiveExitTimestamp
ls -l --time-style=full-iso /etc/fstab /etc/fstab.bak /etc/systemd/system/containerd.service.d/ /etc/systemd/system/docker.service.d/
diff /etc/fstab.bak /etc/fstab
findmnt --verify --tab-file /etc/fstab.bak
cat /sys/fs/ext4/nvme1n1p1/errors_count
# layer folder of each container mount on the main Docker, with the container's creation time (UTC)
grep ' - overlay ' /proc/self/mountinfo | grep ' /data/docker/rootfs/overlayfs/' | while read -r line; do
  sid=$(grep -oE 'upperdir=[^,]+' <<<"$line" | grep -oE 'snapshots/[0-9]+' | cut -d/ -f2)
  cid=$(awk '{print $5}' <<<"$line"); cid=${cid##*/}
  printf '%-8s %s %s\n' "$sid" "$(docker inspect --format '{{.Created}}' "$cid" 2>/dev/null | cut -c1-19)" "${cid:0:12}"
done | sort -n
# one DeepSWE image: are its downloaded files still there? (reads about 840 MB, writes nothing)
docker save public.ecr.aws/d3j8x8q7/swe-bench-202605:kh75679ajj3b8dtd7se3h7z0a1833y6r-v1.1 | wc -c
# missing downloaded files named by docker image ls, ten tries
for n in $(seq 10); do docker image ls -q 2>&1 >/dev/null | grep -oE 'blob sha256:[0-9a-f]{64}' | head -n 1; done | sort -u
# Docker and containerd package history
zgrep -hE 'Start-Date|Commandline|containerd|docker' /var/log/apt/history.log*
```

The checks behind section 2.5. They write `~/snapshot-audit/cause_checks.txt`:

```bash
cd ~/snapshot-audit
{
  echo "== boot and config changes"
  uptime -s
  stat -c '%y  %n' /etc/containerd/config.toml /etc/docker/daemon.json
  ls -ld --time-style=long-iso /var/lib/containerd /data/containerd /etc/taskanchor-runtime 2>&1
  echo "== which service or container runs each daemon"
  for p in $(pgrep -x dockerd) $(pgrep -x containerd); do
    printf '%s\t%s\t%s\n' "$p" "$(sed -n 's/^0:://p' "/proc/$p/cgroup")" "$(tr '\0' ' ' <"/proc/$p/cmdline" | cut -c1-90)"
  done
  echo "== storage settings of the other runtime"
  grep -E '"(data-root|containerd)"' /etc/taskanchor-runtime/daemon.json 2>&1
  grep -nE '^[[:space:]]*(root|state)[[:space:]]*=' /etc/taskanchor-runtime/containerd.toml 2>&1
  echo "== scheduled jobs and tmpfiles rules that mention Docker, /data or deleting"
  systemctl list-timers --all --no-pager
  grep -rlsE 'containerd|/data|prune|rm -rf|-delete' /etc/crontab /etc/cron.d /etc/cron.hourly /etc/cron.daily /etc/cron.weekly
  grep -rns '/data' /etc/tmpfiles.d /usr/lib/tmpfiles.d /run/tmpfiles.d || echo "no tmpfiles rule mentions /data"
  echo "== containers that bind-mount the host's /, /data or containerd's folders"
  docker ps -aq | xargs -r docker inspect --format '{{.Name}} {{.State.Status}}{{range .Mounts}}{{if eq .Type "bind"}} {{.Source}}{{end}}{{end}}' 2>/dev/null \
    | grep -E ' (/|/data|/data/containerd[^ ]*|/var/lib/containerd[^ ]*)( |$)'
} > cause_checks.txt 2>&1
cat cause_checks.txt
```

The checks of 15 Oct behind section 2.4: the 11 and 12 Oct gap, and log coverage:

```bash
journalctl --list-boots --no-pager
journalctl -b -1 -n 12 -o short-iso --no-pager
journalctl -b 0 -o short-iso --no-pager -u data.mount -u containerd.service -u docker.service | grep -m 8 -E 'Mount|Start|Fail'
journalctl -b 0 -k -o short-iso --no-pager | grep -iE 'ext4|recovery|orphan|nvme1n1'
r='^2026-10-1(1T(15:[2-5]|1[6-9]|2)|2T(0|10:[0-3]))'
grep -E "$r" /var/log/syslog | grep -iE 'out of memory|oom-kill|killed process|watchdog|shutdown|power|reboot'
grep -E "$r" /var/log/syslog | grep -oE 'Killed process [0-9]+ \([^)]+\)' | awk '{print $4}' | sort | uniq -c
grep -E '^2026-10-14T20:(1[5-9]|2[0-2])' /var/log/kern.log | grep -E 'blocked for more than|waiting for writeback'
journalctl --no-pager -o short-iso -q -u containerd | head -n 1
for f in /var/log/syslog.4.gz /var/log/auth.log.4.gz /var/log/kern.log.4.gz; do zcat "$f" | head -n 1 | cut -c1-26; done
```

## Appendix B: where the evidence is

On the worker, in `/home/toby/snapshot-audit/`. A copy goes to `~/cloud-worker-evidence/` on Toby's PC (guide step 8.2), outside any git repository.

**From 14 Oct (before the guide):**

| File | Contents |
|---|---|
| `facts.txt` | Host facts (section 1) |
| `base_status.tsv` | The eleven common images: `ok`, `absent`, `pull_failed` or `run_failed`, with the error |
| `deepswe_images.tsv`, `deepswe_status.tsv` | The 113 DeepSWE images (question, then image) and their results |
| `damaged_chains.txt` | The layer IDs named in the errors: `sha256:51e78f04…`, `sha256:29df493b…`, `sha256:08bc4e…` |
| `cause_checks.txt` | The checks behind section 2.5. Its daemon column came from an earlier version of the command and is wrong; Appendix A has the corrected version |
| `damaged_chain_images.tsv`, `deepswe_pull_sample.tsv`, `image_ids.txt` | Empty: from checks that needed root. Guide step 5 replaces them |
| `canary_fail.log` | Not created yet. An hourly line in `toby`'s crontab starts the four working images, and writes here only if one fails |

**From the guide:**

| Folder | Files | Guide step | Answers |
|---|---|---|---|
| `sessions/` | One recording per work session | 2.3 | How every step was run |
| (top) | `long_scans.sh`, `layer_test.sh` | 2.4 | |
| `logcopy/` | Copies of `syslog`, `auth.log` and `kern.log` from 13 Sep, and the containerd and Docker journal | 3.1 | Kept after the weekly rotation |
| `logs/` | `boots.txt` | 4.1 | Q1, Q5 |
| `logs/` | `kernel.txt`, `oom_per_day.txt`, `oom_victims.txt`, `hung_tasks.txt`, `journald_watchdog.txt` | 4.2 | Q9 |
| `logs/` | `logins.txt`, `sudo_commands.txt`, `root_logins_by_day.txt`, `around_1012.txt` | 4.3 | Q2, Q8, Q11 |
| `logs/` | `day_1012.txt` | 4.4 | Q4 |
| `logs/` | `restarts_1014.txt` | 4.5 | Q11 |
| `logs/` | `root_keys.txt` | 6.1 | Q2 |
| `logs/` | `root_history.txt` | 6.2 | Q2, Q8 |
| `logs/` | `crontabs.txt`, `user_timers.txt` | 6.3 | Q8 |
| `logs/` | `hidden_store.txt` | 6.4 | Q3 |
| `logs/` | `split_brain_containers.txt` | 6.5 | Q12 |
| `logs/` | `long_scans.log`, `damage_errors.txt`, `damage_first_seen.txt`, `syslog_volume.txt` | 3.2, 7.1 | Q6, Q10 |
| `inventory/` | `images_list.txt`, `layer_test.log`, `layer_test.tsv` | 3.3, 7.2 | Impact |
| `inventory/` | `images_check.txt`, `snapshots_db.txt`, `content_records.txt`, `content_active.txt`, `snapshot_dirs.txt`, `blob_files.txt` | 5.1 | Q7, Q12, impact |
| `inventory/` | `blobs_db.txt`, `blob_files.sorted`, `blobs_missing.txt`, `blobs_unrecorded.txt`, `blob_ages.txt` | 5.2 | Q7 |
| `inventory/` | `low_numbered_dirs.txt` | 5.3 | Q5 |
| `inventory/` | `snapshot_dirs.txt` (column check; output is in the session recording) | 5.3b | Q5 |
| `inventory/` | `broken_by_repo.txt`, `containers_on_broken.tsv` | 7.2 | Impact |

**From the follow-up (step 9).** `analysis/` exists only on Toby's PC, in `~/cloud-worker-evidence/analysis/`. The other files are written on the worker and copied to the PC:

| Folder | Files | Guide step | Answers |
|---|---|---|---|
| `analysis/` (PC only) | `summary.txt`, `files_by_age.txt`, `missing_first_reported.txt`, `errors_per_day.txt`, `folders_by_date.txt` | 9.1 | Q10, Q13, Q14 |
| `analysis/` (PC only) | `hex_spread.txt` | 9.1b | Q13 |
| `analysis/` (PC only) | `split_brain_window.txt` | 9.1c | Q13, Q15, Q16 |
| `analysis/` (PC only) | `layer_errors.txt` | 9.1d | Q14, Q17 |
| `logs/` | `reboots.txt` | 9.2 | Q5, Q15 |
| `logs/` | `reboots_wtmp.txt` | 9.2b | Q5, Q15 |
| `logs/` | `reboots_console.txt` | 9.2c | Q5, Q15 |
| `logs/`, `inventory/` | `store_history.txt`, `db_record_days.txt`, `hidden_db_digests.txt` (written by root) | 9.3 | Q15, Q16 |
| `analysis/` (PC only) | `hidden_db_match.txt` | 9.4 | Q16 |
| `logs/` | `dockerd_messages.txt` | 9.5 | Q10 |
| `logcopy/`, `sessions/` | Copied to the PC | 9.7 | Evidence kept before any repair |

The mentor's summary of all of this is [`CLOUD_WORKER_MENTOR_REPORT.md`](CLOUD_WORKER_MENTOR_REPORT.md), section 7.

## Appendix C. Which missing images Toby can pull

Added 20 Oct 2026, after the repair. The sections above are the 15 and 16 Oct investigation and do not change. This appendix uses the 16 Oct removal list and the image list from 20 Oct 16:39. A name in the "pull" table is a published image: Docker Official Image, or an image under a public registry account. The old record for each of these names was deleted, so a pull downloads the file instead of skipping it. Toby can run those pulls. He does not have the build scripts for the local benchmarks, so he cannot rebuild those.

### Already on the server

These were the published images the repair put back. They do not need another pull.

| Image | Where it was published | Now |
|---|---|---|
| The 12 evaluation bases, including `alpine:3.23.4` and `ghcr.io/astral-sh/uv:0.9.30-python3.13-bookworm` | Docker Hub and ghcr.io | Present, and they started |
| `python:3.10-slim-bookworm`, `public.ecr.aws/ubuntu/ubuntu:24.04` | Docker Hub and public ECR | Kept. They already started on 16 Oct |
| `python:3.10-slim`, `python:3.10-slim@sha256:c1e4e6c0…`, `python:3.12-slim`, `python:3.6-slim-bullseye@sha256:2cfebc27…`, `python:3.9-slim-bullseye@sha256:b9e06687…` | Docker Official Images | Pulled 21 Oct 09:03. `Status: Downloaded newer image` |
| `eclipse-temurin:8-jdk`, `11-jdk`, `17-jdk` | Docker Official Images | Pulled 21 Oct 09:08. All three printed `Status: Downloaded newer image` |
| `maven:3-eclipse-temurin-17`, `maven:3-eclipse-temurin-8`, `maven:3.9-eclipse-temurin-11` | Docker Official Images | Pulled 21 Oct 09:08. All three printed `Status: Downloaded newer image` |
| `python:3.11-slim@sha256:db3ff2e1…` | Docker Official Images | Pulled 21 Oct 09:22, after 1,623 leftover snapshot names were removed. `Status: Downloaded newer image` |
| `moby/buildkit:v0.24.0` | Docker Hub account `moby` | Pulled 21 Oct 09:13 |
| `condaforge/mambaforge:24.7.1-2` | Docker Hub account `condaforge` | Pulled 21 Oct 09:13 |
| `mirror.gcr.io/curlimages/curl:8.17.0` | Public mirror | Pulled 21 Oct 09:03 |
| `smartdub26/lolbench-linux:TypeScript_2-1.0.0`, `aspnetcore_1-1.0.0`, `cargo_1-1.0.0`, `rust_1-1.0.0` | Docker Hub account `smartdub26` | Pulled 21 Oct 09:27. `Status: Downloaded newer image` |
| `smartdub26/lolbench-linux:arrow_2-1.0.0` and `smartdub26/lolbench:cpython_5-1.0.0` | Docker Hub account `smartdub26` | Pulled 21 Oct 09:38. `Status: Downloaded newer image` |
| Published DeepSWE, 113 names under `public.ecr.aws/d3j8x8q7/swe-bench-202605` | Public ECR | Loaded back. All 113 started |
| expressvpn | `docker.io/misioslav/expressvpn` | The image is present by id `sha256:60c4d5da…` and starts. The old name was not restored |

`ubuntu:22.04` and its digest `sha256:0e0a0fc6…` are present. The longer name `ubuntu:22.04@sha256:0e0a0fc6…` is not a second image.

### Published images still missing, which Toby can pull

| Image | Names still missing | Where to pull |
|---|---:|---|
| `smartdub26/lolbench-linux` digests | 0. All 15 downloaded. `sha256:cec9f7d0…` came back at 11:23 after a `503` | Docker Hub account `smartdub26`. Done |
| `harborframework/terminal-bench` | 4 digests. No tag was stored, only the digest | Docker Hub account `harborframework`, if those digests are still published. Root decides, then Toby pulls |
| `jefzda/sweap-images` | 40 tags. 2 other tags of this repository already start | Docker Hub account `jefzda` |
| `swebench/sweb.eval.x86_64.django_1776_django-10999` and `…django-15930` | 2, tag `latest` | Docker Hub account `swebench` |
| `library/sweb.eval.x86_64.*` | 40 | Same published family, stored on this host under `library/`. Pull `swebench/sweb.eval.x86_64.<id>:latest`. A pull of the `library/sweb.eval…` name looks for a Docker Official Image and will not find one |
| `ghcr.io/principled-systems/verus` | 1, tag `latest` | GitHub Container Registry |
| `xtrm0/dafny` | 1, tag `latest` | Docker Hub account `xtrm0` |
| `xinzhou1997/spec-agent-v17-clarify:0.135.0-py311` and `xinzhou1997/swe_rpg_java:mockito__mockito-3335_linux` | 2 | Docker Hub account `xinzhou1997`, if those tags were pushed |

### Names that look public and are not a sure pull

| Image | Names | Why Toby does not pull these as official images |
|---|---:|---|
| `swebench-pro-opencode/…` | 424 | The path is a Docker Hub organization, and the tags are per-run agent images. They are not Docker Official Images. A pull works only if that account still publishes the tag. The build belongs to that benchmark's owner |
| `library/sweb.openlux.*`, `library/sweb.env.*`, `library/sweb.base.py.x86_64` | 74 | Stored as local names. `openlux` is not the published `swebench/sweb.eval` family |
| `smartdub26/lolbench:TypeScript_2-1.0.0`, `aspnetcore_1-1.0.0`, `cargo_1-1.0.0`, `rust_1-1.0.0` | 4 | Hub publishes these four as `linux/arm64` only. At 21 Oct 10:53 they were retagged from the amd64 `lolbench-linux` images already on the server |
| `smartdub26/lolbench:jdk_1-1.0.0`, `jdk_2-1.0.0`, `jdk_3-1.0.0`, `jdk_5-1.0.0` | 4 | Hub publishes these four as `linux/arm64` only. At 21 Oct 10:53 they were retagged from the amd64 `lolbench-linux` jdk images already on the server |
| `xinzhou1997/requirement-clarifier:local` | 1 | The tag `local` is a local build. Docker Hub will not have it |
| `misioslav/expressvpn@sha256:60c4d5da…` | 1 | The image is already on the host by that digest. Pulling the name would only restore the repository name |

### Local builds Toby cannot pull or rebuild

These were built on this host. The name `docker.io/library/<something>` here is a local tag, not a Docker Official Image. `docker pull` of that name does not find a published image. The owner rebuilds it, which writes a new record and new files together. Toby does not have those build commands.

| Family | Names removed and not restored |
|---|---:|
| jiuwenswarm | 416. Two tags are already back |
| chrys-bench, including the local `chrys-bench-harbor` tags | 534. Five `chrys-bench-base` tags are already back |
| Local `deepswe*` | 486. 120 are already back |
| deepswe-ab-verifier and `deepswe-ab-grading-recovery1` | 113 |
| capbench, other than the four `jdk` images that were kept | 63 |
| clarify, tdd, dgm, p3-replication, and the other per-task images | The removed counts are in the repair report |
| `lolbench-rebuild`, `lolbench-recovery`, and `lolbench/jdk_*-reference-hardened` | Local lolbench names. The published `smartdub26` tags are in the pull table above |
| Harbor sidecars tagged `library/harbor-prebuilt` | 2 |

The repair report, [Images, before the repair and after it](CLOUD_WORKER_REPAIR_REPORT.md), is the full before-and-after table. This appendix is only the published subset Toby can pull himself.
