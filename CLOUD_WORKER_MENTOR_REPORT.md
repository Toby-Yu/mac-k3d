# Cloud worker Docker images: what broke, why, and what we propose

- **Host:** cloud worker `iZt4ndd2dff7gqjta7mppaZ` (Jenkins node `linux-iZt4ndd2dff7gqjta7mppaZ`), a shared evaluation worker.
- **Prepared by:** Toby, 15 and 16 Oct 2026, for the mentor and the root account holder.
- **Status:** the read-only investigation is finished. Nothing in the store has been repaired, removed or restarted. The proposals in section 9 need the mentor's and root's approval first. The decisions are in section 10.
- **Based on:**
  - the investigation record, [`CLOUD_WORKER_DOCKER_REPORT.md`](CLOUD_WORKER_DOCKER_REPORT.md);
  - the step-by-step guide, [`CLOUD_WORKER_INVESTIGATION_STEPS.md`](CLOUD_WORKER_INVESTIGATION_STEPS.md): steps 0 to 8.2 and 9.1 to 9.2c and 9.5 on 15 Oct, and steps 9.3, 9.4, 9.7 and 9.8 on 16 Oct. Step 9.3 was run with `root.pem`, read-only, and the view was removed. Step 9.6 is a written question with no answer recorded;
  - the evidence copy on Toby's PC, in `~/cloud-worker-evidence/` (section 7).
- **Labels:** <span class="tag confirmed">confirmed</span> shown by the evidence; <span class="tag likely">likely</span> the best explanation, not yet proven; <span class="tag unknown">unknown</span> open.
- **Aim:** find which step of the process failed, not who to blame. People are named only where we need to ask them something.

## 1. Summary

1. **Most images on the worker are broken.** On 15 Oct, 6,593 of the 6,847 images tested failed to start, and only 254 started. All 113 DeepSWE images fail, 7 of 11 common base images fail, and `docker image ls` itself fails. 158 containers were created from broken images. Images pulled since the 12 Oct restart work.
2. **The damage always has the same shape: containerd's records point at data that is gone.** 42,915 of 46,179 downloaded-file records have no file on disk. There are 19,581 layer records but only 10,164 layer folders.
3. **The cause is a "split brain" (section 3).**
   - containerd started on an old, second store on the system disk. Then the 14 TB `/data` disk was mounted on top of it.
   - containerd kept that old store's database, but worked on the real disk. Its cleanup deleted real data that the old database did not list.
   - It started with a broken `/data` line in `/etc/fstab`. No boot could mount the disk, so every boot needed a manual mount.
4. **It happened on 12 Oct, after a forced restart.** <span class="tag confirmed">confirmed</span> for the layer folders and for the downloaded files (guide step 9.4).
   - The boot skipped `/data`, and containerd started on the old store. At 10:54 the disk was mounted on top of the running containerd. For the next hour containerd worked on the real disk with the wrong database.
   - Every check possible on the PC copy fits:
     - nothing recorded after the restart is missing;
     - the cleanup finished its pass;
     - the 65 files that both databases are known to list all survived.
5. **An earlier loss, before 13 Sep, was most likely the same failure.** <span class="tag likely">likely</span>
   - **The old database had been used before.** It already held at least 786 images on 12 Oct, and the real database doesn't record 721 of them. No file was ever written into the old store itself. So the old database was in use while the real disk was mounted on top.
   - **It also lists DeepSWE images pulled in September.** The host did not boot then: the boots are 4 Aug 20:44, 27 Aug 20:25, 27 Aug 21:35 and 12 Oct 10:38, and it stayed up from 27 Aug 21:35 until 12 Oct. The disk was mounted by hand on that running host.
   - **It fits the loss:** files recorded before mid-August are gone, and no layer folder older than 10 Sep survives.
   - **It left a different trace from 12 Oct.** Since 14 Sep, layers fail because their record is gone together with their folder. Only after 12 Oct do layers also fail with just the folder gone.
   - **The record days are dated** (guide steps 9.3 and 9.4). The old database was written from 4 Aug through 10 Sep, and again on 12 Oct. The 14 TB database was written 24 Jul to 4 Aug, then not again until 10 Sep. Who mounted the disk on 10 Sep is still unknown. That question does not block the repair.
6. **27 images have stale records. Jenkins builds first met them on 6 and 7 Oct, but they are most likely older, from the earlier loss.** <span class="tag likely">likely</span>
   - Two of them were already failing on 29 and 30 Sep, with the same error.
   - Docker logged no error at all on 2 to 4 Oct, so 6 and 7 Oct are not necessarily when they broke.
   - Still open: one kind of error that the log never records, and what xu's repair scripts changed.
7. **Some of it is fixed, and some is still exposed.**
   - Written since 12 Oct 12:09, and not yet proven: the fstab line now uses the disk's UUID, and containerd and Docker are set to refuse to start without `/data`. Root confirmed on 16 Oct that this has not been tested, because the host has not been rebooted since. A line that parses today can still be skipped at the next boot. The check is in section 9.2. Do not reboot as part of this investigation.
   - Still exposed:
     - nothing stops someone mounting `/data` under running services;
     - the host can still run out of memory and hang;
     - nothing checks the store, so the damage went unnoticed for a month;
     - images built here are in no registry;
     - root actions are hard to trace: the root key is shared, and the history has no timestamps.
8. **For root, who asked whether the images were removed or only disagree with the records.** <span class="tag confirmed">confirmed</span> from the disk check of 15 Oct and the database comparison of 16 Oct (record, steps 5.1, 5.2, 9.1 and 9.4). The questions in guide step 9.6 have no answer recorded, and the repair does not wait for them.
   - Docker still lists the images: 7,750 names. They were not dropped from the list.
   - The data behind most of them was removed. 42,915 of 46,179 downloaded-file records have no file on the 14 TB disk. Those records add up to 481 GB. The recorded files still on disk add up to 405 GB. 9,417 of 19,581 layer records have no folder. 97 files are on disk with no record, so a file sitting unlisted is rare. The disk was not wiped: it is 14 TB, 88% used, with 1.7 TB free. The 481 GB is the size of the missing recorded files. The cleanup deleted them. The disk being mostly full is the data that survived.
   - The missing files are not in the old store on the system disk. That store holds a database and no downloaded files, and its folder is unchanged since 4 Aug 20:44.
   - What still works: 254 of 6,847 tested images start, and every file recorded after the 12 Oct 13:07 restart is present. All 113 DeepSWE images fail because their unpacked folders are gone; their downloaded files are still on disk.
9. **Proposal, now that the read-only checks are finished:**
   - a planned rebuild of the store in an announced window (section 9.0);
   - guards that make each step of this failure impossible or visible: a registry, a daily store check, memory limits, team rules, and later a Jenkins health gate.

## 2. The situation: one shared evaluation worker

- **The host:** 96 CPUs, 182 GB of memory, a 504 GB system disk, and a separate 14 TB `/data` disk (88% used). The `/data` disk was grown online twice: on 21 Sep (from 8.0 to 11.5 TB) and on 5 Oct (to 15.0 TB, shown by `df` as 14T).
- **Shared by everyone:** one Docker daemon, one containerd and one image store, in `/data/containerd`.
- **Many people run different benchmarks on it directly:**
  - DeepSWE, SWE-bench (`sweb.*` images) and lolbench;
  - capbench, chrys-bench and jiuwenswarm;
  - Harbor tasks and TaskAnchor.
  
  12 accounts are in the `docker` group, which is root-equivalent.
- **Root is reached with two SSH keys.** One of them is the same key as `root.pem`, Toby's copy for this investigation. It logs in from several addresses, so it is shared.
- **mac-k3d's Jenkins jobs also run here.** Since 14 Oct they run on the local worker instead (`AGENT_LABEL=linux-Michael-Ubuntu`).
- **Why this matters:** one failure in the host, the mount or one person's clean-up damages every user's images at once. Nobody owns the store, and host changes leave little trace, so it took a month to notice.

## 3. How the image store fits together

An image is a stack of layers. containerd keeps each layer twice, and keeps a record of each copy in a database. Docker trusts these records; it does not check the disk before using them.

**The two stores, and how the mount hides one of them.** <span class="tag confirmed">confirmed</span>

There is one path, `/data`, and two different directories have used that path.

- **The store in use now** is on the 14 TB disk, `/dev/nvme1n1p1`, which is mounted on `/data`. Everything you see at `/data/containerd` is this store: the database containerd opened at 13:07 on 12 Oct, the 3,361 downloaded files still on disk, and the 10,164 layer folders. On 15 Oct this disk was 88% used, with 1.7 TB free. The login banner on 16 Oct at 09:14 said 92.7% of 13.49 TB, about 1.0 TB free. Usage went up, so the disk was not emptied overnight. It was not wiped.
- **The old store** is on the system disk, in the `/data` directory that the 14 TB disk covers. While the disk is mounted, that directory is not visible at `/data`. Root sees it only by a read-only view of the system disk, at `/mnt/rootfs/data/containerd` (guide steps 6.4 and 9.3). It was created at the 4 Aug 20:44 boot. Its database, `io.containerd.metadata.v1.bolt/meta.db`, is 865 MB and was last written at 12 Oct 11:48:43. It has no folder of downloaded files.
- **What happened.** On 12 Oct the boot did not mount the 14 TB disk, because the `/data` line in `/etc/fstab` was wrapped in quotes. containerd started and opened the old database on the system disk. At 10:54 the 14 TB disk was mounted on `/data` while containerd was still running. From then on, new files were written to the 14 TB disk, but the open database stayed the old one. containerd's cleanup deletes disk data that its open database does not list, so it deleted layer folders on the 14 TB disk. <span class="tag confirmed">confirmed</span> for those folders. <span class="tag likely">likely</span> for most of the 481 GB of missing downloaded files. An earlier loss, before 13 Sep, was most likely the same kind of mismatch, during the 4 Aug boot or one of the two 27 Aug boots.

**12 Oct, one path and two disks.** A mount does not merge two disks. It covers a folder. After 10:54, the name `/data` means the 14 TB disk. The system disk's `/data` folder is still there, underneath, and containerd already had its database file open from that folder.

<p class="legend"><span class="disk">system disk, 504 GB</span><span class="store">14 TB disk</span><span class="fault">the step that deleted data</span></p>
<div class="flow">
<div class="node disk"><b class="t">Before 10:54. The 14 TB disk is not mounted</b>The path <code>/data</code> is a folder on the 504 GB system disk. containerd opens the old database there, <code>/data/containerd/.../meta.db</code>, created 4 Aug. <span class="tag confirmed">confirmed</span><span class="ev">fstab line was quoted, so the boot skipped it; containerd started at 10:40</span></div>
<div class="arrow">10:54: someone mounts /dev/nvme1n1p1 on /data, while containerd is still running</div>
<div class="node store"><b class="t">After 10:54. The same path now means the other disk</b>New files written to <code>/data/containerd</code> land on the 14 TB disk. The database containerd already opened stays the old one, on the system disk, now hidden under the mount.<span class="ev">meta.db last written 12 Oct 11:48:43, eight seconds before containerd stopped</span></div>
<div class="arrow">cleanup: delete whatever the open database does not list</div>
<div class="node fault"><b class="t">The 14 TB disk loses layer folders the old database does not name</b>The downloaded files that remain are on the 14 TB disk, not in the old store. The old store has a database and no downloaded files.<span class="ev">3,361 files and 10,164 folders at /data/containerd; the hidden store has no blobs folder</span></div>
<div class="arrow">13:07: containerd is started again, with the 14 TB disk already mounted</div>
<div class="node store"><b class="t">The database in use now is the 14 TB one</b>It still lists files and folders the cleanup removed. Docker trusts that list, so the images fail and a pull does not replace a file the record still names.</div>
</div>

**Base images and benchmarks, from the 15 Oct start test** (`inventory/layer_test.tsv`). Each image is in one row. A missing downloaded file and a missing layer folder are both "the record remains, the data was removed". The same tables are in section 9.0.

| Base image | Images | Usable | Not usable |
|---|---:|---:|---:|
| `alpine:3.21`, `alpine:3.22`, `busybox:1.36`, `debian:bookworm` | 4 | 4 | 0 |
| `ubuntu:22.04`, `ubuntu:24.04`, `debian:bookworm-slim`, `alpine:3.19` | 4 | 0 | 4, downloaded file gone |
| `alpine:3.20`, `alpine:latest`, `alpine:3.23.4`, the `uv` image | 4 | 0 | 4, stale record |
| **12 base tags** | **12** | **4** | **8** |

| Benchmark | Images | Usable | Not usable | Downloaded file gone | Layer folder gone | Stale record |
|---|---:|---:|---:|---:|---:|---:|
| DeepSWE published | 113 | 0 | 113 | 0 | 113 | 0 |
| Local `deepswe*` | 611 | 0 | 611 | 491 | 120 | 0 |
| deepswe-ab-verifier | 108 | 0 | 108 | 108 | 0 | 0 |
| jiuwenswarm | 418 | 0 | 418 | 416 | 2 | 0 |
| chrys-bench | 426 | 0 | 426 | 421 | 5 | 0 |
| lolbench | 335 | 235 | 100 | 92 | 0 | 8 |
| capbench | 67 | 4 | 63 | 63 | 0 | 0 |
| SWE-bench Pro | 424 | 0 | 424 | 424 | 0 | 0 |
| SWE-bench sweap | 42 | 2 | 40 | 24 | 0 | 16 |
| SWE-bench `sweb.*` | 116 | 0 | 116 | 116 | 0 | 0 |
| Not one of the benchmarks above | 4,187 | 13 | 4,174 | 4,158 | 13 | 3 |
| **Tested, total** | **6,847** | **254** | **6,593** | | | |

The row "not one of the benchmarks above" is 4,187 names: 26 official base images, 4,154 task images built on this server, and 7 Harbor or buildkit images. Section 9.0 splits that row. The published DeepSWE images are the special case: their downloaded files are still on the 14 TB disk, and every unpacked layer folder is gone, so all 113 fail. The four capbench images that still start are `capbench-linux-sync/jdk-1`, `jdk-2`, `jdk-3` and `jdk-5`. The eight stale lolbench images are the `TypeScript_2`, `aspnetcore_1`, `cargo_1` and `rust_1` tags of `smartdub26/lolbench` and `smartdub26/lolbench-linux`.

<p class="legend"><span class="people">users and jobs</span><span class="store">containerd's records</span><span class="disk">data on disk</span><span class="fault">failure</span></p>
<div class="flow">
<div class="node people"><b class="t">12 docker-group accounts and Jenkins jobs</b>Every benchmark runs <code>docker pull</code>, <code>docker build</code> and <code>docker run</code> against the same daemon</div>
<div class="arrow">one Docker daemon, one containerd, one store for everybody</div>
<div class="node store"><b class="t">Image record</b>in containerd's main database <code>/data/containerd/io.containerd.metadata.v1.bolt/meta.db</code>: name, then manifest, then the list of layers</div>
<div class="arrow">each layer is kept twice</div>
<div class="row">
<div class="node store"><b class="t">Downloaded-file record</b>also in <code>meta.db</code>: one per compressed layer, config or manifest, by digest</div>
<div class="node store"><b class="t">Layer record</b>in the layer database <code>…/io.containerd.snapshotter.v1.overlayfs/metadata.db</code>: one per unpacked layer, with a folder number</div>
</div>
<div class="row">
<div class="arrow">must match a file</div>
<div class="arrow">must match a folder</div>
</div>
<div class="row">
<div class="node disk"><b class="t">Downloaded file ("blob")</b><code>…/io.containerd.content.v1.content/blobs/sha256/&lt;digest&gt;</code><span class="ev">46,179 records, 3,361 files on disk</span></div>
<div class="node disk"><b class="t">Layer folder ("snapshot")</b><code>…/io.containerd.snapshotter.v1.overlayfs/snapshots/&lt;N&gt;/fs</code><span class="ev">19,581 records, 10,164 folders on disk</span></div>
</div>
<div class="row">
<div class="arrow">file gone, record kept</div>
<div class="arrow">folder gone, record kept</div>
</div>
<div class="row">
<div class="node fault"><b class="t"><code>blob not found</code>: 6,313 images</b>For example <code>debian:bookworm-slim</code>, <code>ubuntu:24.04</code>, chrys-bench and jiuwenswarm. It also breaks <code>docker image ls</code></div>
<div class="node fault"><b class="t"><code>failed to stat parent … no such file or directory</code>: 253 images</b>All 113 DeepSWE images are in this group</div>
</div>
<div class="arrow">a record whose data is only partly there</div>
<div class="node fault"><b class="t"><code>already exists</code> or <code>missing parent</code>: 27 images</b>For example <code>alpine:3.23.4</code> (first failed in a build on 6 Oct), <code>alpine:3.20</code> (failing since at least 30 Sep) and the <code>uv</code> image</div>
</div>

Two rules of containerd explain the whole incident:

1. **Its cleanup deletes data that its open database does not list.** That is safe only while the database belongs to the disk it is cleaning.
2. **It trusts its records.** A downloaded file with a record counts as present, so pulling the image again does not bring the file back. The record has to be removed first. This follows from containerd's design; it will be tested on one image before any repair.

**What a split brain is.** containerd's store lives at a fixed path, `/data/containerd`.

- When the 14 TB disk is not mounted, that path is an ordinary folder on the system disk. containerd creates or opens a store there.
- When the disk is mounted later, the path shows the real store. But containerd keeps the database file it has already opened, which is now hidden under the mount.
- From then on, containerd reads and writes files and folders on the real disk, while its records go to the hidden database. Rule 1 then deletes real data.
- Restarting containerd after the mount ends it: containerd then opens the real database. Whatever the cleanup deleted stays gone, and the real records still point at it.
- The real database's own cleanup then deletes what only the old database recorded. For example, the layer folders of the two containers created during the 12 Oct split brain are gone.

## 4. Cause 1: the 12 Oct split brain

<span class="tag confirmed">confirmed</span> for the layer folders, <span class="tag likely">likely</span> for most downloaded files. All times are CST.

<p class="legend"><span class="trigger">trigger</span><span class="fault">step that failed</span><span class="store">what containerd used</span><span class="disk">what was on disk</span><span class="fixed">fix</span></p>
<div class="flow">
<div class="node trigger"><b class="t">Earlier, date unknown: a root one-liner writes the <code>/data</code> line into <code>/etc/fstab</code> with literal quotes</b><code>mkdir -p /data &amp;&amp; cp /etc/fstab /etc/fstab.bak &amp;&amp; mount /dev/nvme1n1p1 /data &amp;&amp; echo \"/dev/nvme1n1p1 /data ext4 defaults 0 0\" &gt;&gt; /etc/fstab</code>. The line is invalid, but fstab is read only at the next boot<span class="ev">root's history, line 303 (logs/root_history.txt); /etc/fstab.bak on the worker: findmnt reports a parse error at line 12</span></div>
<div class="arrow">weeks later</div>
<div class="node trigger"><b class="t">11 Oct 15:21 to 12 Oct 10:36: the host runs out of memory again and again</b>26 out-of-memory kills (java, python, opencode, rg, harbor; never containerd or Docker). The system journal stops answering and is killed at 15:23 and 15:31<span class="ev">logs/oom_per_day.txt, logs/oom_victims.txt, logs/journald_watchdog.txt, logs/kernel.txt</span></div>
<div class="arrow"></div>
<div class="node trigger"><b class="t">12 Oct 10:38: the host is restarted without a clean shutdown</b>Both disks replay their filesystem journals. Who restarted it is not in the host's logs<span class="ev">logs/kernel.txt (orphan cleanup at 10:38:15), logs/boots.txt</span></div>
<div class="arrow">the boot reads fstab</div>
<div class="node fault"><b class="t">The quoted line is skipped, so the 14 TB disk is not mounted on <code>/data</code></b>Local filesystems are reported mounted at 10:38:11 without <code>/data</code><span class="ev">logs/boots.txt; systemctl show (record, section 2.4)</span></div>
<div class="arrow">no rule made containerd wait for /data</div>
<div class="node fault"><b class="t">10:38 to 10:40: containerd starts on <code>/data/containerd</code> of the <i>system disk</i></b>The old store from 4 Aug is there. containerd opens its database (<code>meta.db</code>, 865 MB, holding at least 786 images) and keeps it open. Its cleanup runs at 10:41 to 10:43 and fails: the old layer database has no records at all. Docker starts at 10:42:54<span class="ev">logs/boots.txt, logs/hidden_store.txt, analysis/split_brain_window.txt</span></div>
<div class="arrow">16 minutes later</div>
<div class="node fault"><b class="t">10:54:05: the 14 TB disk is mounted on <code>/data</code> under the running containerd</b>Very likely the same one-liner (history line 469). <code>/etc/fstab.bak</code> is saved at 10:54:02 and the disk is mounted at 10:54:05: that is its <code>cp</code>, then its <code>mount</code><span class="ev">logs/root_history.txt, logs/kernel.txt (10:54:05), /etc/fstab.bak file time, logs/around_1012.txt (the 10:50 root login)</span></div>
<div class="arrow">split brain from 10:54 to 11:48</div>
<div class="row">
<div class="node store"><b class="t">What containerd used: the hidden August database</b>It recorded the hour's work there: <code>busybox</code> and <code>docker:29-dind</code> pulled at 11:20 and 11:23, two containers at 11:33 and 11:41 (layer folders 37 and 54, from its own count), a build at 11:44. Last write at 11:48:43<span class="ev">analysis/split_brain_window.txt, logs/hidden_store.txt</span></div>
<div class="node disk"><b class="t">What was on disk: the real store on the 14 TB disk</b>Folders 37 and 54 were created here. The real store's own folders, numbered from 152,997, were not in the open database's list</div>
</div>
<div class="arrow">containerd's cleanup: delete what the open database does not list</div>
<div class="node fault"><b class="t">Real layer folders are deleted</b> <span class="tag confirmed">confirmed</span>: the 57 missing folders that the start test names (153149 to 159017) sit among folders created on 10 Sep, when DeepSWE was pulled. 7,826 folders from before the restart survive, so the removal did not reach every folder. One possible reason: it was cut off when containerd was stopped at 11:48:51<br><b class="t">Most real downloaded files are deleted too</b> <span class="tag likely">likely</span>: every file recorded after the restart is present (634 of 634), while 5,791 of the 5,795 recorded on 2 to 11 Oct are missing<span class="ev">inventory/layer_test.tsv, analysis/folders_by_date.txt, analysis/files_by_age.txt, analysis/hex_spread.txt</span></div>
<div class="arrow"></div>
<div class="node fixed"><b class="t">11:48:51 services stopped. 12:09:49 fstab rewritten with <code>UUID=…</code>, and containerd and Docker made to require <code>/data</code>. 13:07:52 containerd reopens the real database</b><span class="ev">logs/day_1012.txt; fstab and drop-in file times (record, section 2.4)</span></div>
<div class="arrow"></div>
<div class="node fault"><b class="t">Result: the real database points at folders and files that are gone</b>Images pulled before 12 Oct fail; images pulled after 13:07 work. The two containers created in the window (folders 37 and 54) are unknown to the real database and will not start again</div>
</div>

**Why the downloaded files are probably part of the same event.** The analysis of the PC copy (guide step 9.1, `analysis/`) sorts every file record by its age and by where it came from:

| File records | Missing | Present |
|---|---|---|
| Recorded after the 12 Oct restart (13:07) | 0 | 634 |
| Recorded 2 Oct to 12 Oct, before the restart | 5,791 | 4 |
| Recorded before 2 Oct | 37,124 | 2,626 |
| Before the restart, made on this host (built or loaded; no registry label) | 41,170 | 80 |
| Before the restart, pulled from a registry | 1,745 | 2,550 |

- **Files written on 10 and 11 Oct are all gone, and nothing written after 13:07 on 12 Oct is.** So most of the loss happened between 11 Oct midday and the 12 Oct restart.
- **In that window, the only process that deletes files in bulk is containerd's cleanup,** running against the wrong database. A forced restart can lose only the last seconds of writes, not weeks-old files.
- **Files made on this host are almost all gone.** Many files pulled from registries survived, including all of DeepSWE's.
- **The cleanup finished its pass** (guide step 9.1b). It goes through the files in alphabetical order of their digest. The survivors are spread evenly in that order, 5 to 7 % in each of the 16 groups, so it was not cut off partway.
- **The files known to be in the old database survived** (guide step 9.1c). Docker listed 786 images from the old database during the split brain. The real database records 65 of them, and all 65 files are present; none is missing.
- **What is not proven yet:** whether the old database lists every surviving file, and none of the missing ones. Check 1 in section 8 compares the full list.

## 5. Cause 2: an earlier split brain, and the stale records

<span class="tag likely">likely</span> that an earlier split brain caused the earlier loss, and that the stale records come from it. When each database was in use is now known (guide step 9.3d, 16 Oct).

<p class="legend"><span class="trigger">trigger</span><span class="fault">step that failed</span><span class="store">what containerd used</span><span class="disk">what was on disk</span><span class="unknown">not explained yet</span></p>
<div class="flow">
<div class="node disk"><b class="t">24 Jul: the host is set up</b>Docker and containerd are installed, with the store at <code>/data/containerd</code>. Their configuration has not changed since<span class="ev">record, section 2.5 (apt history and config file dates)</span></div>
<div class="arrow">fstab has no valid /data line, so every boot needs a manual mount</div>
<div class="node fault"><b class="t">4 Aug 20:44:37: a boot. Two seconds earlier, containerd creates the old store</b>Login prompts start on <code>tty1</code> and <code>ttyS0</code> at 20:44:37. The old store's folders are all created at 20:44:35, so containerd started during that boot, while <code>/data</code> was not mounted<span class="ev">logs/reboots_console.txt, logs/hidden_store.txt</span></div>
<div class="arrow"></div>
<div class="row">
<div class="node fault"><b class="t">27 Aug: two boots, 20:25:23 and 21:35:55</b>The layer-database folder changes at 20:26:32, 69 seconds after the first. The second boot is not in the journal. Both are possible only while <code>/data</code> is not mounted at start<span class="ev">logs/reboots_console.txt, logs/hidden_store.txt</span></div>
<div class="node store"><b class="t">5 Aug to 9 Sep: only the old database is written</b> <span class="tag confirmed">confirmed</span>. The 14 TB database has no record created on those days. The old database has records on almost every day from 4 Aug 20:44 through 10 Sep<span class="ev">logs/db_record_days.txt</span></div>
</div>
<div class="arrow"></div>
<div class="node disk"><b class="t">10 Sep: DeepSWE's images are pulled into the real store</b>Folder 152,997 (09:22) is now the oldest on disk<span class="ev">analysis/folders_by_date.txt; DeepSWE's file records are 4 to 5 weeks old</span></div>
<div class="arrow"></div>
<div class="node disk"><b class="t">27 Aug 21:35 to 12 Oct 10:38: the host stays up</b>No login prompt in between, so no boot. 11 Sep 09:14 was the oldest journal line still kept, not a boot. <code>/data</code> was mounted by hand on this running host<span class="ev">logs/reboots_console.txt; fstab had no valid line, from the 12 Oct boot</span></div>
<div class="arrow">if containerd was already running when the disk was mounted: a split brain</div>
<div class="row">
<div class="node store"><b class="t">The old database records new work, while the files go to the real disk</b> <span class="tag likely">likely</span>: on 12 Oct it held at least 786 images, 721 of them unknown to the real database, and 44 from DeepSWE's registry. No file was ever written into the old store itself<span class="ev">analysis/split_brain_window.txt, logs/hidden_store.txt</span></div>
<div class="node fault"><b class="t">The cleanup deletes real data that the old database doesn't list</b> <span class="tag likely">likely</span>: files recorded before mid-August, and every layer folder older than 10 Sep. Images used in the window, such as DeepSWE and <code>sweap-images</code>, survive<span class="ev">analysis/missing_first_reported.txt, analysis/folders_by_date.txt</span></div>
</div>
<div class="arrow">later, containerd is restarted with /data mounted and opens the real database (date unknown)</div>
<div class="node fault"><b class="t">13 Sep 01:37: first <code>blob not found</code>, on the oldest day the logs keep. 14 Sep 12:05: first <code>missing parent</code></b>A layer's record is gone together with its folder, while the main database still lists the layer<span class="ev">logs/damage_first_seen.txt, analysis/errors_per_day.txt, analysis/layer_errors.txt</span></div>
<div class="arrow"></div>
<div class="node disk"><b class="t">13 Sep to 12 Oct: the 14 TB disk stays mounted, so no split brain</b>The kernel log shows no other mount of it. It is grown online on 21 Sep and 5 Oct<span class="ev">logs/kernel.txt, analysis/split_brain_window.txt</span></div>
<div class="arrow"></div>
<div class="row">
<div class="node fault"><b class="t">29 Sep to 10 Oct: the old stale records are met again</b> <span class="tag likely">likely</span>: <code>alpine:3.20</code>'s missing parent already fails on 30 Sep, and <code>alpine:latest</code>'s on 29 Sep. No error is logged on 2 to 4 Oct. Jenkins builds meet them on 6 and 7 Oct<span class="ev">analysis/layer_errors.txt, analysis/errors_per_day.txt</span></div>
<div class="node unknown"><b class="t">6 to 10 Oct: what filled the syslog?</b>It grows from 41 MB a day to 506 MB on 6 Oct and 4,568 MB on 9 Oct, nearly all of it Docker's lines<span class="ev">logs/syslog_volume.txt</span></div>
</div>
<div class="arrow"></div>
<div class="node fault"><b class="t">12 Oct: the same failure again (section 4)</b></div>
</div>

**Why an earlier split brain is likely:**

- **The old database had been used before 12 Oct.** It held at least 786 images when containerd opened it, and the real database doesn't record 721 of them.
- **Their files were written to the real disk.** The old store's `containerd` folder has not changed since 4 Aug 20:44:36, and it has no folder for downloaded files. So nothing was ever downloaded into the old store itself. Those images must have been pulled while the old database was open and the real disk was mounted on top.
- **It was in use after DeepSWE was pulled in September.** The 65 images that both databases list include 44 from DeepSWE's registry, recorded in the real database 4 to 5 weeks ago. Only two pulls are logged on 12 Oct, too few to have added them then.
- **The boots are now dated, and 11 Sep is not one of them.** Login prompts mark five boots: 24 Jul 15:15, 4 Aug 20:44, 27 Aug 20:25, 27 Aug 21:35 and 12 Oct 10:38. The host then stayed up until 12 Oct. fstab had no valid `/data` line, so the disk was mounted by hand on that running host, not after an 11 Sep boot.
- **It explains the loss:** files recorded before mid-August are gone, no layer folder older than 10 Sep survives, and the first errors appear on 13 and 14 Sep. The images used in that window, such as DeepSWE and `jefzda/sweap-images`, are in the old database, and their files survived.
- **It left a different trace from 12 Oct** (guide step 9.1d).
  - Before 12 Oct, the only layer error is `missing parent`: 202 lines from 14 Sep to 11 Oct. The layer database has no record of a layer that the main database lists, so the record went together with its folder.
  - `failed to stat parent` appears only after 12 Oct: 59 lines, all on 14 Oct. There the record is kept and only the folder is gone.
  - containerd has two layer cleanups. The main database's cleanup removes a layer through the layer store, record and folder together. The layer store's own cleanup removes only the folders that its database doesn't list.
  - So in the earlier loss, the old main database's cleanup most likely worked through the real layer database. On 12 Oct, the layer store's cleanup worked against the old layer database, which held only that hour's layers.

**Still to confirm:**

- **Why the old layer database was empty when containerd started on 12 Oct:** its cleanup failed with `bucket does not exist`.
  - A likely reason: containerd opens the layer database only when it first needs it, not at start. If, in the earlier split brain, `/data` was mounted before that first use, containerd used the real layer database together with the old main database. That would also give the different trace above.
  - Check 8 reads containerd's source. Check 2 gives the layer database's birth time; its folder last changed on 27 Aug.
- **The real database's empty period starts in mid-August,** before any September date. The 4 Aug boot created the old store, and the two 27 Aug boots are the last host starts before 12 Oct. Check 2 dates every record in both databases.

**The stale records, first met by Jenkins builds on 6 and 7 Oct:**

- They are most likely older (guide step 9.1d). `alpine:3.20`, pulled on 24 Jul, fails with a `missing parent` that was already in the log on 30 Sep. `alpine:latest`'s was in the log on 29 Sep.
- Docker logged no error at all on 2 to 4 Oct, and one on 5 Oct, so the dates of 6 and 7 Oct need not be when they broke.
- There was no out-of-memory kill and no disk stall on those days, and no boot or mount between 13 Sep and 12 Oct.
- Still open:
  - `alpine:3.23.4` fails with `already exists`, a kind the log never records, so it can't be dated.
  - xu's Docker audit and repair scripts were run as root, several with `--apply`. Their evidence folder is dated 7 Oct (`missing-targets.txt`). What they change is not known.
  - What filled the syslog from 6 Oct: 41 MB a day before, then 506 MB on 6 Oct and 4,568 MB on 9 Oct, nearly all of it Docker's lines.

**Candidates, and what would tell them apart:**

| Candidate | Fits | Doesn't fit, or not known | Check (section 8) |
|---|---|---|---|
| **Earlier loss:** an earlier split brain, during the 4 Aug boot or one of the two 27 Aug boots, with `/data` mounted by hand afterwards on the host that stayed up until 12 Oct | The old database held images whose files were written to the real disk, including September's DeepSWE images. fstab could not mount `/data` at boot. Files before mid-August and folders before 10 Sep are gone; the first errors are on 13 and 14 Sep. Its layer errors differ from 12 Oct's in the way that the old main database plus the real layer database would explain | The old layer database was empty on 12 Oct; explained only if containerd opens it at first use (check 8). Which of the three boots, and when the disk was mounted, are not known | 1, 2, 3, 8 |
| **Earlier loss:** a clean-up that removed old files and folders, by hand or by a script | The loss hit only old files and folders | No such command in root's retained history, and no cron job or timer does it. It doesn't explain the old database's images | 4 |
| **Earlier loss:** a move or copy of the store around 10 Sep that left out old data | No folder older than 10 Sep | No copy command in the retained history. It doesn't explain the old database's images | 2, 4 |
| **Stale records:** a late effect of the earlier loss, seen when an image was next used | `missing parent` errors from 14 Sep. `alpine:3.20`'s missing parent already failed on 30 Sep, and `alpine:latest`'s on 29 Sep. No error at all was logged on 2 to 4 Oct | `already exists` (`alpine:3.23.4`) is never logged, so it can't be dated | 5 |
| **Stale records:** xu's repair scripts (`--apply`) | Dates match (evidence folder of 7 Oct) | At least two stale records were failing before 7 Oct. What the scripts change is not known | 4 |

## 6. Confirmed, likely, unknown

| Finding | Status | Main evidence |
|---|---|---|
| The quoted fstab line made the 12 Oct boot skip `/data` | <span class="tag confirmed">confirmed</span> | `/etc/fstab.bak` and `findmnt --verify` (record, section 2.4); `logs/boots.txt` |
| containerd opened the old store on the system disk, kept its database open through the 10:54 mount, and wrote to it until 11:48:43 | <span class="tag confirmed">confirmed</span> | `logs/hidden_store.txt`, `logs/boots.txt`, `analysis/split_brain_window.txt` |
| The 12 Oct split brain deleted real layer folders, including all DeepSWE images' | <span class="tag confirmed">confirmed</span> | `inventory/layer_test.tsv`, `analysis/folders_by_date.txt` |
| The 10:54 mount was the root one-liner at history line 469 | <span class="tag likely">likely</span>; the times match to the second | `logs/root_history.txt`, `logs/kernel.txt`, file time of `/etc/fstab.bak` |
| Most missing downloaded files were deleted in the same window | <span class="tag confirmed">confirmed</span> | `analysis/hidden_db_match.txt` (guide step 9.4), `analysis/files_by_age.txt` |
| An earlier loss, before 13 Sep, took files recorded before mid-August and every layer folder older than 10 Sep | <span class="tag confirmed">confirmed</span> that it happened | `analysis/missing_first_reported.txt`, `analysis/folders_by_date.txt`, `logs/damage_first_seen.txt` |
| containerd had used the old database before 12 Oct with the real disk mounted on top, and this caused the earlier loss | <span class="tag likely">likely</span>; the boots that could start it are now known (4 Aug 20:44, 27 Aug 20:25, 27 Aug 21:35), and there was no boot on 11 Sep | `analysis/split_brain_window.txt`, `logs/hidden_store.txt`, `logs/reboots_console.txt` |
| The earlier loss removed layer records together with their folders; 12 Oct removed only folders | <span class="tag likely">likely</span>; the error kinds show it, and the mechanism is to be confirmed in containerd's source (check 8) | `analysis/layer_errors.txt` |
| No split brain between 13 Sep and 12 Oct: the 14 TB disk was not mounted again | <span class="tag confirmed">confirmed</span> | `logs/kernel.txt` |
| The stale records first met on 6 and 7 Oct were already there before: at least two failed on 29 and 30 Sep | <span class="tag confirmed">confirmed</span> | `analysis/layer_errors.txt` |
| The stale records come from the earlier loss | <span class="tag likely">likely</span>; the `already exists` kind and xu's scripts are <span class="tag unknown">unknown</span> | `analysis/layer_errors.txt`, `analysis/errors_per_day.txt`, `logs/oom_per_day.txt` |
| Who restarted the host, who typed the mount, who rewrote fstab, who restarted containerd on 14 Oct | <span class="tag unknown">unknown</span>; root key `QHhV…` was the only root key used on 12 Oct | `logs/around_1012.txt`, `logs/restarts_1014.txt` |
| Filesystem damage, a failing disk, an upgrade, a config change, containerd killed for memory | ruled out | record, section 2.5 |

## 7. Where the evidence is

**On Toby's PC, in `~/cloud-worker-evidence/`.** This is outside any git repository, because it holds user names, IP addresses and root's commands. About 4.8 GB, including `logcopy/` (3.3 GB), copied on 16 Oct (guide step 9.7).

| Folder | File | What it shows | Used for |
|---|---|---|---|
| `logs/` | `boots.txt`, `day_1012.txt` | Boots; every containerd and Docker line of 12 Oct, including the cleanup runs, pulls and image checks of the split brain | Section 4 timeline |
| `logs/` | `kernel.txt`, `oom_per_day.txt`, `oom_victims.txt`, `journald_watchdog.txt`, `hung_tasks.txt` | Out-of-memory kills, the journal being killed, disk stalls; the 10:38 orphan cleanup, the 10:54:05 mount, the two disk resizes | The trigger, the mount time, no other mount since 13 Sep |
| `logs/` | `hidden_store.txt` | The old store under `/data`: dates, `meta.db` size and last write | Both split brains |
| `logs/` | `root_history.txt` | Root's shell history (filtered): the fstab one-liner at lines 303 and 469, xu's scripts | Who did what (no timestamps) |
| `logs/` | `around_1012.txt`, `logins.txt`, `root_logins_by_day.txt`, `root_keys.txt`, `sudo_commands.txt` | Root logins and keys, sudo commands | Who was logged in |
| `logs/` | `restarts_1014.txt` | containerd's three restarts on 14 Oct | Unexplained restarts |
| `logs/` | `split_brain_containers.txt` | The two containers made during the split brain | Owners to inform |
| `logs/` | `damage_errors.txt` (1.3 GB), `damage_first_seen.txt`, `syslog_volume.txt` | Every containerd and Docker error line since 13 Sep; first date of each kind; syslog size per day | Earlier loss, stale records |
| `logs/` | `crontabs.txt`, `user_timers.txt` | Scheduled jobs | No clean-up job found |
| `inventory/` | `images_list.txt`, `images_check.txt` | All 7,750 images; containerd's own check of each | Impact |
| `inventory/` | `content_records.txt`, `blob_files.txt`, `blobs_missing.txt`, `blobs_unrecorded.txt`, `blob_ages.txt` | Downloaded-file records against files on disk | 42,915 missing files |
| `inventory/` | `snapshots_db.txt`, `snapshot_dirs.txt` | Layer records against folders on disk | Missing folders |
| `inventory/` | `layer_test.tsv`, `broken_by_repo.txt`, `containers_on_broken.tsv` | Start test of every image; broken images per repository; containers on broken images | Impact, owners |
| `analysis/` | `summary.txt`, `files_by_age.txt`, `missing_first_reported.txt`, `errors_per_day.txt`, `folders_by_date.txt` | Guide step 9.1: files by age and source, first errors, folders by date | Sections 4 and 5 |
| `analysis/` | `hex_spread.txt` | Guide step 9.1b: surviving files in the cleanup's order | The cleanup finished its pass |
| `analysis/` | `split_brain_window.txt` | Guide step 9.1c: the split-brain hour, the images in the old database, the 14 TB disk's mounts | Both split brains |
| `analysis/` | `layer_errors.txt` | Guide step 9.1d: the two kinds of layer error per day, and when three missing parents first failed | The two losses' traces; the stale records' age |

**Only on the worker:**

| Where | What | Note |
|---|---|---|
| `/home/toby/snapshot-audit/logcopy/` | Copies of syslog, auth.log and kern.log from 13 Sep, and the containerd and Docker journal (3.3 GB) | Copy to the PC before any repair (check 6) |
| `/home/toby/snapshot-audit/sessions/2026-10-15_1205.log` | Recording of the whole session of 15 Oct | Shows exactly what was run |
| `/var/log/syslog.1` | The 15 GB syslog of 4 to 11 Oct | Compressed at the next weekly rotation (Sunday 18 Oct), then kept 4 weeks |
| `/etc/fstab`, `/etc/fstab.bak`, `/etc/systemd/system/{containerd,docker}.service.d/` | The fixed line, the quoted line, and the guards | File times are evidence: do not edit before the review |
| Under the `/data` mount point, on the system disk | The old store, 1.7 GB, used during the split brains | The read-only view was removed on 16 Oct. Keep the store until the approved repair archives it |
| Alibaba Cloud console | Instance restart and disk history | Root only |

**In the mac-k3d repository:** the record ([`CLOUD_WORKER_DOCKER_REPORT.md`](CLOUD_WORKER_DOCKER_REPORT.md)), the guide with every command and its result ([`CLOUD_WORKER_INVESTIGATION_STEPS.md`](CLOUD_WORKER_INVESTIGATION_STEPS.md)), and this report. None of them contains a key or a password.

## 8. What the checks found, and what is still unknown

The worker commands are finished. Rows 1, 2, 3, 5 and 6 were run on 15 and 16 Oct. Rows 4, 7 and 8 are still unknown. None of them is a command left to run before the repair in section 9.

| # | Question | Check | Who |
|---|---|---|---|
| 1 | Did the split brains delete the downloaded files? | Done on 16 Oct (guide 9.4). Of 2,626 files from before 2 Oct still on disk, 2,624 are named in the old database. Of 37,124 missing from that period, 36,969 are not named. Of 5,791 missing from 2 to 12 Oct, none are named. The cleanup kept what the old database listed and deleted the rest | Toby (guide 9.4) |
| 2 | When was the old database in use, and when was each store created? | Done on 16 Oct (guide 9.3b, 9.3d). The 14 TB database was written 24 Jul to 4 Aug, then not again until 10 Sep, then every day after. The old database was written 4 Aug through 10 Sep, and again on 12 Oct (229 records). It has no record dated after 12 Oct. The 14 TB filesystem was created 24 Jul 15:54, mounted 4 times, last on 12 Oct 10:54. The old store has 0 layer folders | Toby, as root, read-only |
| 3 | Which boots were there? | Done. Login prompts (guide 9.2c): 24 Jul 15:15, 4 Aug 20:44, 27 Aug 20:25, 27 Aug 21:35, 12 Oct 10:38. No boot on 11 Sep, and none between 27 Aug 21:35 and 12 Oct | Toby |
| 4 | What happened on 4 Aug, 27 Aug and 10 to 13 Sep? Who mounted `/data` after the 27 Aug 21:35 boot, on the host that stayed up until 12 Oct? Who restarted the host on 12 Oct, mounted `/data` at 10:54, rewrote fstab at 12:09, and restarted containerd on 14 Oct? What do xu's `--apply` scripts change? | Questions in writing to root and xu. The Alibaba console has the restart and disk history | Toby asks (guide 9.6) |
| 5 | What were the 22 million Docker log lines of 4 to 11 Oct? | Done. 20,275,470 lines are a failed platform-manifest check, and 1,346,163 are a failed image-label check. Docker was re-checking images whose files were already missing | Toby (guide 9.5) |
| 6 | Is all the evidence safe before a repair? | Done on 16 Oct. `logcopy/` (3.3 GB), `sessions/` and the reboot and message files are on the PC | Toby (guide 9.7) |
| 7 | Who owns the two split-brain containers and the broken images? | `broken_by_repo.txt` and `split_brain_containers.txt` to the team | Toby |
| 8 | Why was the old layer database empty when containerd started on 12 Oct? The layer errors fit a first-use opening (guide step 9.1d) | Read containerd 2.2's source: does the overlayfs snapshotter open its database at start, or at first use? And which cleanup removes a layer's record with its folder? | Toby |

## 9. Proposed solution (needs the mentor's and root's approval)

The goal is that every base image and every benchmark image on this worker starts, and that the records for those images match the files and the folders. Nothing in this section is done by the investigation. This is the account to give root before any fix. The questions in guide step 9.6 are unanswered, and the repair does not wait for them.

### What broke, and why

containerd is the program that stores the images and starts the containers. Docker passes `docker pull` and `docker run` to it. Its data on this host is the directory `/data/containerd`. That path is one name for two disks. `/data` is a folder on the system disk until the 14 TB disk is mounted on it. After the mount, the name `/data` means the 14 TB disk, and the system-disk folder is hidden underneath.

Each store has two databases. The database holds the record. The downloaded bytes and the unpacked files sit beside it.

| Database | The record | The file that must match it |
|---|---|---|
| Main database, `meta.db` | The image name, and one downloaded-file record per blob | The blob, at `blobs/sha256/<digest>` |
| Layer database, `metadata.db` | One layer record per unpacked layer, with a folder number | The snapshot folder, at `snapshots/<number>/fs` |

**Counts to give root, before any fix.** The 14 TB numbers are the inventory of 15 Oct. The old-store numbers are from the read-only check of 16 Oct.

| Piece | 14 TB store, in use now | Of which the record remains and the file is gone | Old store, system disk, hidden under the mount |
|---|---:|---:|---|
| Image names | 7,750 | 6,593 of 6,847 tested images fail to start | Records only. Docker is not using this store |
| Downloaded-file records | 46,179 | 42,915 have no blob (481 GB of recorded sizes) | The main database names them. 180,168 digest strings are in the file, including old copies |
| Blob files on disk | 3,361 | 97 of these have no record in the current database | No blob folder |
| Layer records | 19,581 | 9,417 have no snapshot folder | The layer database is 256 KB |
| Snapshot folders | 10,164 | | 0 |

**Why the 14 TB disk is still full.** The cleanup deleted files. It did not empty the disk. On 16 Oct at 09:14 the login banner said `/data` was 92.7% of 13.49 TB, about 1.0 TB free, so about 12.5 TB was in use. On 15 Oct the same disk was 88% used, with 1.7 TB free. Usage went up, so new data was still being written.

The 42,915 missing blobs add up to 481 GB of recorded sizes. That is the size of what is gone, about 4% of 13.49 TB. Deleting it leaves a disk that is still mostly full. The 7,750 image names and the 19,581 layer records are rows inside the two database files. They are not a separate pile of bytes.

| Part | Still on the 14 TB disk? | Size |
|---|---|---|
| The whole `/data` filesystem | Yes | 13.49 TB. About 12.5 TB used and about 1.0 TB free on 16 Oct 09:14 |
| Blob files that a record still names | Yes. 3,264 files | **405 GB**, summed from the size recorded for each file |
| Blob files on disk with no record | Yes. 97 files | Not in the size list |
| Main database and layer database | Yes | 655 MB and 64 MB |
| Missing blob files | No. The record remains, the file is gone | **481 GB** recorded. This is not part of the 12.5 TB in use |
| The 10,164 snapshot folders | Yes. The folders are there | Not measured. The listing shows each folder's own entry, 4 KB, not the unpacked files inside |
| Anything else on `/data`, including `/data/docker` | On this disk if it lives under `/data` | Not measured |

The measured image bytes still on the disk are the 405 GB of blobs plus about 0.7 GB of databases. Subtracting that from the 12.5 TB in use leaves about 12.1 TB. That remainder is the unpacked snapshot folders plus whatever else is stored on `/data`. It was not added up file by file, so it is not a size for the snapshots alone.

Files recorded after containerd opened the 14 TB database on 12 Oct at 13:07 are all still on disk: 634 files, none missing.

**The 12 Oct split brain.** A running containerd keeps the databases it opened at start. It does not switch databases when a disk is mounted later.

On 12 Oct the boot at 10:38 did not mount the 14 TB disk. containerd started at 10:40 and opened the old store's main database and the old store's layer database, on the system disk. At 10:54 the 14 TB disk was mounted on `/data` while containerd was still running. New files written to that path landed on the 14 TB disk. The records stayed in the old databases, which containerd already had open. Those two files were last written at 11:48:43, eight seconds before containerd stopped.

containerd's own garbage collection then ran against those open databases. Its job is to delete blobs and snapshot folders that the open records do not name, so leftovers do not fill the disk. The blobs and snapshot folders on the 14 TB disk were not named by the old records, so the cleanup treated them as leftovers and deleted them. The records in the 14 TB databases were left in place. They still name the missing blobs and the missing snapshot folders.

Step 9.4 measured the downloaded files. Of 2,626 files from before 2 Oct still on the 14 TB disk, 2,624 are named in the old main database. Of 37,124 missing from that period, 36,969 are not named. Of 5,791 missing from 2 to 12 Oct, none are named. The old layer database has 0 snapshot folders, and 9,417 layer records on the 14 TB disk name a folder that is gone. The published DeepSWE images are this case: their blobs are still on the 14 TB disk, and every snapshot folder is gone.

The same mismatch ran from the 4 Aug 20:44 boot until 10 Sep: only the old database received new records. On 10 Sep the 14 TB database was in use again. That is the date of the oldest snapshot folder still on the 14 TB disk.

### 9.0 What has to work, and in which order

**All images on 15 Oct.** Docker lists 7,750 names. The start test tried the 6,847 named images and skipped 903 leftover `moby-dangling@sha256:` build tags. Usable means the start test returned `ok`.

| Group | Images | Usable | Not usable |
|---|---:|---:|---:|
| Every name Docker lists | 7,750 | | |
| Named images the start test tried | 6,847 | 254 | 6,593 |
| Not in the start test (leftover build tags) | 903 | | |

**What the 7,750 names are.** Official base images are 26 of them: `alpine`, `ubuntu`, `debian`, `busybox`, `python`, `maven`, `eclipse-temurin` and `mambaforge`. The names beside the named benchmarks are task images built on this server, those 26 base images, 7 tool images, and 903 leftover build tags.

| Group | What the names are | Images | All four present | Record yes, blob file no | Blob yes, snapshot folder no | Layer record stale | Not in the start test |
|---|---|---:|---:|---:|---:|---:|---:|
| Named benchmarks | The 10 benchmark rows below | 2,660 | 241 | 2,155 | 240 | 24 | 0 |
| Official base images | `alpine`, `ubuntu`, `debian`, `busybox`, `python`, `maven`, `eclipse-temurin`, `mambaforge` | 26 | 6 | 18 | 0 | 2 | 0 |
| Task images built here | One image per task, plus its verifier and egress copies | 4,154 | 7 | 4,134 | 13 | 0 | 0 |
| Harbor and buildkit | `harborframework/terminal-bench` (4), `harbor-prebuilt` egress sidecars (2), `moby/buildkit` (1) | 7 | 0 | 6 | 0 | 1 | 0 |
| Leftover build tags | `moby-dangling@sha256:…`. No tag a job would pull | 903 | | | | | 903 |
| **Every name Docker lists** | | **7,750** | **254** | **6,313** | **253** | **27** | **903** |

The columns are the same four pieces as the benchmark table. The Harbor row's stale record is `harbor-prebuilt:harbor-docker-egress-control-sidecar--b67f71411fe7d8f5`.

**The 4,154 task images.** These are evaluation tasks built on this server. A typical name is `library/<task>__<id>-main`, plus a verifier and a `pier-egress-proxy` copy of the same task.

| Group | What it is | Images | All four present | Record yes, blob file no | Blob yes, snapshot folder no | Layer record stale |
|---|---|---:|---:|---:|---:|---:|
| Per-task copies | `main`, verifier, and egress, one set per task | 3,552 | 0 | 3,552 | 0 | 0 |
| clarify | `clarify-*` rebuilds | 422 | 0 | 410 | 12 | 0 |
| tdd | `tdd-*` | 146 | 0 | 146 | 0 | 0 |
| dgm | `dgm-knowledge-*` | 7 | 3 | 4 | 0 | 0 |
| swe-marathon | `swe-marathon-cpu` and one dind probe | 3 | 2 | 1 | 0 | 0 |
| p3-replication | `p3-replication-xin` | 3 | 0 | 3 | 0 | 0 |
| chrys recovery | `chrys-recovery` | 2 | 0 | 2 | 0 | 0 |
| Other local builds | `contextgraph-kit`, `e2e-formal-agent-runtime`, `proofke-evolving`, `remain178-dind-probe`, and similar | 19 | 2 | 16 | 1 | 0 |
| **Task images** | | **4,154** | **7** | **4,134** | **13** | **0** |

The 7 task images that start are `dgm-knowledge-k3-v2`, `dgm-knowledge-k3-v3`, `dgm-knowledge-k3-v4`, both `swe-marathon-cpu/daemon-bootstrap` tags, `proofke-evolving:lean428-20261015` and `remain178-dind-probe:29.6.2-flat`.

**The 26 official base images inside the 7,750.** Six start. The 12 tags in the next table are the ones mac-k3d evaluations ask for. Three of those 12, `debian:bookworm-slim`, `alpine:3.23.4` and the `uv` image, are not names in this list of 7,750. A job tried them and they failed. They are not an extra 3 on top of the 26.

| Image | Names in the 7,750 | All four present | Record yes, blob file no | Blob yes, snapshot folder no | Layer record stale |
|---|---|---:|---:|---:|---:|
| `alpine` | 5: `3.19`, `3.20`, `3.21`, `3.22`, `latest` | 2 (`3.21`, `3.22`) | 1 (`3.19`) | 0 | 2 (`3.20`, `latest`) |
| `ubuntu` | 5: `22.04`, the same tag by digest, `24.04`, `ubuntu` with no tag, `public.ecr.aws/ubuntu/ubuntu:24.04` | 1 (the `public.ecr.aws` tag) | 4 | 0 | 0 |
| `python` | 7 lines: `3.10-slim` by name and by digest, `3.11-slim`, `3.12-slim`, `3.6-slim-bullseye`, `3.9-slim-bullseye`, `3.10-slim-bookworm` | 1 (`3.10-slim-bookworm`) | 6 | 0 | 0 |
| `debian:bookworm` | 1 | 1 | 0 | 0 | 0 |
| `busybox:1.36` | 1 | 1 | 0 | 0 | 0 |
| `eclipse-temurin` | 3, each by digest: 8, 11, 17 | 0 | 3 | 0 | 0 |
| `maven` | 3, each by digest | 0 | 3 | 0 | 0 |
| `condaforge/mambaforge` | 1 | 0 | 1 | 0 | 0 |
| **Official bases** | **26** | **6** | **18** | **0** | **2** |

**Base images the mac-k3d evaluations ask for.** These 12 are shared tags. Four start. Eight do not. The four that start were pulled on 14 Oct. Today's repair does these 12. The other official tags in the table above, including the five failing `python` tags, are not in that 8 hours.

| Image | Usable |
|---|---|
| `alpine:3.21`, `alpine:3.22`, `busybox:1.36`, `debian:bookworm` | Yes. 4 images |
| `ubuntu:22.04`, `ubuntu:24.04`, `debian:bookworm-slim`, `alpine:3.19` | No. The downloaded file is gone. 4 images |
| `alpine:3.20`, `alpine:latest`, `alpine:3.23.4`, `ghcr.io/astral-sh/uv:0.9.30-python3.13-bookworm` | No. The record is stale. 4 images |
| **These 12** | **4 usable, 8 not usable** |

**The four pieces, for every base tag we have checked.** "Not reached" means the start stopped at an earlier missing piece, so the later piece was not tested. `debian:bookworm-slim`, `alpine:3.23.4` and the `uv` image were seen in job failures, not in the 6,847-image start test.

| Image | Usable | Download-file record | Blob file | Layer record | Snapshot folder |
|---|---|---|---|---|---|
| `alpine:3.21`, `alpine:3.22`, `busybox:1.36`, `debian:bookworm` | Yes. 4 | Yes | Yes | Yes | Yes |
| `python:3.10-slim-bookworm` | Yes. 1 | Yes | Yes | Yes | Yes |
| `public.ecr.aws/ubuntu/ubuntu:24.04` | Yes. 1 | Yes | Yes | Yes | Yes |
| `alpine:3.19` | No | Yes | No | Not reached | Not reached |
| `ubuntu:22.04` (the name, and the same image by digest) | No | Yes | No | Not reached | Not reached |
| `ubuntu:24.04`, and `ubuntu` with no tag | No | Yes | No | Not reached | Not reached |
| `debian:bookworm-slim` | No | Yes | No | Not reached | Not reached |
| `python:3.10-slim`, `3.11-slim`, `3.12-slim`, `3.6-slim-bullseye`, `3.9-slim-bullseye` | No. 5 tags. `3.10-slim` is listed twice | Yes | No | Not reached | Not reached |
| `alpine:3.20`, `alpine:latest` | No | Yes | The start got past the blob check | Parent record is missing | Not created |
| `alpine:3.23.4`, `ghcr.io/astral-sh/uv:0.9.30-python3.13-bookworm` | No | Yes | Not reported missing | The record already exists | Not created |

**Benchmarks, from the same start test.** Each image is in one row, so the rows add up to the 6,847. An earlier table counted a few names in two rows; this is the table to use for the totals.

| Benchmark | Images | All four present, usable | Record yes, blob file no | Blob yes, snapshot folder no | Layer record stale |
|---|---:|---:|---:|---:|---:|
| DeepSWE published | 113 | 0 | 0 | 113 | 0 |
| Local images whose name contains `deepswe` | 611 | 0 | 491 | 120 | 0 |
| deepswe-ab-verifier | 108 | 0 | 108 | 0 | 0 |
| jiuwenswarm | 418 | 0 | 416 | 2 | 0 |
| chrys-bench | 426 | 0 | 421 | 5 | 0 |
| lolbench | 335 | 235 | 92 | 0 | 8 |
| capbench | 67 | 4 | 63 | 0 | 0 |
| SWE-bench Pro, `swebench-pro-opencode` | 424 | 0 | 424 | 0 | 0 |
| SWE-bench, `jefzda/sweap-images` | 42 | 2 | 24 | 0 | 16 |
| SWE-bench, `sweb.*` | 116 | 0 | 116 | 0 | 0 |
| Not one of the benchmarks above | 4,187 | 13 | 4,158 | 13 | 3 |
| **Tested, total** | **6,847** | **254** | **6,313** | **253** | **27** |

The last benchmark row, 4,187, is the 26 official bases, the 4,154 task images and the 7 Harbor and buildkit images in the tables above. Reading a row: the image name is still in the database in every column. **All four present** means the download-file record, the blob, the layer record and the snapshot folder match, and the image starts. **Record yes, blob file no** means the start stopped because the blob is gone, so the layer record and the snapshot were not reached. **Blob yes, snapshot folder no** means the blob is on disk and the layer record still names a folder that is gone. **Layer record stale** means the start got past the blob and failed because the layer record is wrong (`already exists` or `missing parent`). The four capbench images that start are `jdk-1`, `jdk-2`, `jdk-3` and `jdk-5`.

158 containers were created from broken images (85 Created, 67 Exited, 6 Dead). They are not running.

### Today's plan, and how long the server is empty

Planning estimates, not a timed run. `live_restore` is false, so stopping Docker stops every container. Today's list is the mount, the base images (including the ones that broke the earlier mac-k3d evaluation runs), the 113 published DeepSWE images, and the 418 jiuwenswarm images. Local `deepswe*` (611), deepswe-ab-verifier (108), chrys-bench, lolbench, capbench and SWE-bench are not in this day.

| Order | Work | Hours to reserve | What is true at the end |
|---|---|---:|---|
| 1 | One store. Stop Docker and containerd, archive the old store, make empty `/data` immutable, mount the 14 TB disk, start containerd | 1 | containerd is running on the 14 TB database only. Image records are still the broken ones |
| 2 | The 12 base images, including the mac-k3d evaluation failures: `alpine:3.23.4` (build #53), `alpine:3.20` (build #61), `alpine:3.19` (egress fallback), `alpine:latest`, `ubuntu:22.04`, `ubuntu:24.04`, `debian:bookworm-slim`, and `ghcr.io/astral-sh/uv:0.9.30-python3.13-bookworm`. Confirm the four that already start | 1 | All 12 start. Eight are a remove-and-pull. The four already usable are only checked |
| 3 | Published DeepSWE, 113 images. The downloaded files are already on disk, about 120 GB. The work is unpacking layer folders | 2 | Those 113 start |
| 4 | jiuwenswarm, 418 images. The downloaded files are gone, so this is a pull or a rebuild, then a start check | 4 | Those 418 start. This is the part that can run longer if the registry is slow. The server stays empty until the check passes |
| | **Today, total** | **8** | Mount is the one 14 TB disk, containerd is up, the 12 bases start, published DeepSWE starts, jiuwenswarm starts |

1. Reserve **8 hours** with nobody using the server.
2. The first **4 hours** are the mount, the 12 base images, and the 113 published DeepSWE images.
3. The next **4 hours** are the 418 jiuwenswarm images.
4. If a jiuwenswarm pull is still running when those 4 hours end, the server stays empty until that image's start check has passed.

**Dates of the two stores** (16 Oct). The file counts are in the table under "What broke, and why".

- Old main database: born 4 Aug 20:44:35, last written 12 Oct 11:48:43, 907 MB. Old layer database: born 27 Aug 20:26:32, last written at that same 11:48:43, 0 snapshot folders.
- 14 TB main database file: born 14 Oct 20:08:59, 655 MB, open. Its layer database file is the one from 24 Jul 17:14.
- 14 TB filesystem: created 24 Jul 15:54:51, mounted 4 times, last mount 12 Oct 10:54:05. It has not been mounted since.

**Order of the repair.** Each step has one purpose. The host steps are done once, by the host owner. Image steps start only after the store structure is one database on the 14 TB disk.

1. **Make one store, and make the old store unreachable.** Purpose: containerd can open only the 14 TB database, so a later mount cannot hide a second database under `/data` again. With `docker.socket`, `docker` and `containerd` stopped, unmount `/data`, move the hidden store (database only, no downloaded files) from the system-disk mount point into an archive, and make the empty mount point immutable (`chattr +i /data`). Mount the 14 TB disk on `/data` again and check it with `findmnt`. Keep the UUID line and the settings that refuse to start containerd unless `/data` is a mount. A written rule refuses to mount `/data` while containerd or Docker is running. The archive stays until the review is closed. After this step the records and the files are still the broken pairs in the table above. What changed is that there is no second store left at the mount point.

**Which records this step uses.** It does not rebuild a correct store from either database. It chooses the 14 TB store as the only store containerd will open.

| Store | Used as the correct records? | Why |
|---|---|---|
| Old store, system disk | No. It is moved to an archive and not opened again | It has records and no blobs and no snapshot folders. Treating its records as the keep-list is what the 12 Oct cleanup did, and that deleted the 14 TB files |
| 14 TB store | Yes, this is the one store that remains | Its databases stay in place. They are the records containerd opens. They are not correct yet: 42,915 downloaded-file records and 9,417 layer records still name a file or folder that is gone. The 3,361 blobs and 10,164 snapshot folders that are still on this disk stay too |

The correct records are written in steps 2 to 5, into the 14 TB databases. How one image is changed is section 9.1. The old database is not copied in.

2. **Repair the 12 base images, and check them.** Purpose: every later benchmark is built on these. Confirm the 4 that already start. Remove and pull the 8 that do not. One person does this, because the images are shared. The names are in the base-image table above, including `alpine:3.23.4` (build #53) and `alpine:3.20` (build #61).
3. **Repair published DeepSWE (113), then the local `deepswe*` images (611) and deepswe-ab-verifier (108).** Purpose: DeepSWE runs on the bases from step 2. The 113 already have their downloaded files on the 14 TB disk (about 120 GB), so the pull unpacks new layer folders. The 611 and the 108 are missing the file or the folder, so the pull downloads what is gone. Today's plan does the 113 only. The 611 and the 108 are a later day.
4. **Repair the 418 jiuwenswarm images.** Purpose: that benchmark runs on the corrected bases, including the `uv` image from step 2. All 418 fail. Remove the record and pull or rebuild.
5. **Split the remaining benchmarks, one owner each.** Purpose: the store is one database and the bases are correct, so each owner repairs only that benchmark's images. SWE-bench `sweb.*` and sweap, SWE-bench Pro, the lolbench images that do not start (leave the 235 that do), capbench (leave the four `jdk` images), and chrys-bench. They use `docker` only. They do not mount a disk, and they do not get the root key.
6. **Check after each group.** Purpose: the next group starts from records that match the disk. Each repaired image starts, its downloaded-file record has its blob, and its layer record has its snapshot folder.

**Why the empty `/data` is made immutable.** Four pieces, together:

1. `chattr +i` on the empty system-disk folder, after the old store is archived. The kernel then refuses to create a file there, so containerd cannot make a second database on the system disk.
2. Mounting the 14 TB disk covers that folder. Writes go to the 14 TB disk. The flag stays on the hidden folder underneath. The flag does not choose which disk is mounted.
3. The `UUID=` line mounts this 14 TB disk. `RequiresMountsFor=/data` and `AssertPathIsMountPoint=/data` stop containerd from starting while `/data` is only the system-disk folder. Those lines are written and have not been tested by a reboot. Section 9.2 is the check.
4. A rule that `/data` is never mounted while containerd is already running. That is the act that caused 12 Oct.

### 9.1 How one image goes from the current records to a correct image

**How a record is changed.** Do not edit `meta.db` or the layer `metadata.db` by hand. They are database files, not lists you can rewrite, and they can hold other users' settings.

For one image, the change is remove, then pull. A pull while the old record remains does nothing: containerd treats the image as already present.

| Piece | Before, on an image that fails | After `docker rmi`, then `docker pull` |
|---|---|---|
| Image record in `meta.db` | The name is listed, and it points at missing data | A new record for that name |
| Downloaded-file record | The row remains | A new row for each blob of this image |
| Blob on the 14 TB disk | Present for the 113 published DeepSWE images. Gone for the bases and benchmarks in step 3 and step 4 of section 9.0 | Kept when it was present, so the pull does not download it again. Downloaded when it was gone |
| Layer record | Present, and it names a missing folder, or it is a stale record | A new record, written by the unpack |
| Snapshot folder | Missing, or not the folder the record names | A new folder, written by the same unpack |

Before any removal, tell the owners of the two containers created during the 12 Oct split brain, and of the 158 containers created from broken images. Shared bases are repaired in step 2, by one person, before anyone repairs a benchmark.

### 9.2 Prevent a repeat on this server

**The fstab fix has to be checked before it counts as protection.** Root rewrote the line on 12 Oct and said on 16 Oct that it has not been tested, because there has been no reboot. On this server the last bad line also looked like a mount instruction, and the boot skipped it. So a written line is not evidence that the next boot will mount the 14 TB disk.

Ask root to do two things, both recorded:

1. **Now, read-only, no reboot.** Done on 16 Oct as `toby` (guide step 9.8): `0 parse errors, 0 errors`. The three warnings are permission denied, because `toby` cannot read the disk's own type. `/data` is `/dev/nvme1n1p1`. This proves the line is valid today. It does not prove the next boot.
2. **Later, in an announced window, one reboot.** After the boot, `/data` must already be mounted before containerd starts, and containerd must be using the database on the 14 TB disk. Until that reboot has been watched, the fstab change stays an untested fix. Do not reboot to find out during this investigation.

Each row below is one step of the 12 Oct failure, and the guard that stops that step. None of these is done by the investigation. They wait for approval. The UUID line and the "refuse to start without `/data`" settings were written on 12 Oct and are in row 1 and row 2. They are not marked done, because no reboot has tested them.

| What went wrong on this server | Guard | How it stops that step | When |
|---|---|---|---|
| A broken fstab line meant that every boot needed a manual mount, for weeks | Keep the `UUID=` line (written 12 Oct, not tested: no reboot since). Run `findmnt --verify` now, and again after any fstab edit. Prove it with one announced reboot before calling it protection. Don't paste system commands copied from escaped text, such as JSON or a chat message: the one-liner's `\"` became literal quotes | A bad line is caught the day it is written. The reboot is what shows the boot actually mounts `/data` | Now, then one planned reboot |
| containerd started without `/data` | Keep `RequiresMountsFor=/data` and `AssertPathIsMountPoint=/data` (written 12 Oct, also untested until a reboot). Empty the mount point and make it immutable (`chattr +i`), so nothing can be written to the system disk's `/data` | containerd refuses to start, and there is no second store it could open | The settings exist; the immutable mount point is before the repair |
| `/data` was mounted under a running containerd, on 12 Oct and, if the earlier loss is a split brain, by hand during the uptime that began on 27 Aug 21:35 | A written procedure, and a small root script: refuse to mount `/data` while `containerd` or `docker` is active; stop `docker.socket`, `docker` and `containerd` first | The split brain needs both a running containerd and a second store. Both are now blocked | Before the repair |
| The host ran out of memory for 19 hours and was force-restarted | A memory ceiling for all containers (Docker's `cgroup-parent` set to a slice with `MemoryMax`) and for `user.slice`. Turn on `systemd-oomd`, so the job that uses too much memory is killed early, not the host | The host stays responsive, with no forced restart and no unclean boot | Before the repair |
| The damage went unnoticed for a month (`blob not found` since 13 Sep; stale records failing since at least 29 Sep) | A daily read-only store check, run as a root timer: records against files and folders, `ctr images check`, and a few canary images started. It writes a status file that everyone can read, and alerts the team | Damage is seen within a day, and its date is known | With the repair |
| `/data` was grown by hand twice (21 Sep, 5 Oct), and nothing reports free space | The daily check also reports free space on `/data` and `/`, and warns the team at 85% | Space is added before writes fail, and nobody needs an emergency clean-up under running jobs | With the repair |
| Images built here can't be restored | A private registry as the source of truth: push every image an evaluation needs, name images by digest, and use a pull-through cache for public images | A damaged store becomes a refill from the registry, not weeks of rebuilds | With the repair |
| Root actions can't be traced | Personal admin accounts with sudo; retire the shared root key; `HISTTIMEFORMAT` and `history -a` for root; `auditd` rules on `/etc/fstab`, `/etc/containerd`, `/etc/docker`, `/etc/systemd`, and on deletions in `/data/containerd` by anything other than containerd; one change log for host changes | Next time, who did what and when is known within minutes | Now |
| The logs were too short (the journal is capped at 3.9 GB, and containerd's entries start on 10 Oct) | Keep containerd's and Docker's logs for at least 30 days. Rate-limit or fix the noisy Docker messages | The evidence of the next incident survives | Now |

### 9.3 While several people still use the worker directly

mac-k3d does not yet run every harness and benchmark, so direct use continues for a while. These rules keep the shared store consistent while many people pull and run the same images:

1. **One host owner.** Only the host owner mounts, edits fstab, reboots, or restarts Docker or containerd, and only after announcing it in the team channel. Everyone else asks.
2. **No global clean-up.** No `docker system prune`, `docker image prune -a` or `docker builder prune` on the shared daemon. Nobody deletes anything under `/data/containerd` or `/data/docker`. Disk space is freed by one agreed clean-up job, which removes images by owner label and last use, never images that a benchmark pins.
3. **Label and pin.** Build with `--label owner=<name>`. Name shared base images (`alpine`, `ubuntu`, `python`, DeepSWE bases) by digest, and treat them as the platform's, not removable by one user.
4. **Concurrent use is safe; deleting under others is not.** containerd writes a file once per digest, so many people pulling the same image at the same time is fine. The danger is deleting, or a host change, while others use the store. Rules 1 and 2 cover both.
5. **Check before a run.** Start the image once with no network (`docker run --rm --network none <image> true`). If it fails with `blob not found` or `failed to stat parent`, report a store problem instead of retrying. Retrying fills the logs: the 22 million Docker lines of 4 to 11 Oct.
6. **Memory.** Run long jobs with a memory limit (`docker run --memory`, or `systemd-run --scope -p MemoryMax=`), and say in the team channel when a run needs more than about 64 GB.

### 9.4 When Jenkins and mac-k3d run the evaluations

<p class="legend"><span class="fixed">source of truth or guard</span><span class="store">pipeline step</span><span class="disk">worker store</span><span class="people">evaluation jobs</span></p>
<div class="flow">
<div class="node fixed"><b class="t">Registry: the source of truth for every image an evaluation uses</b>Public images come through a pull-through cache. Images built here are pushed after the build. Configs name images by digest</div>
<div class="arrow">pull by digest</div>
<div class="node store"><b class="t">Prepare and verify on the worker</b>mac-k3d's prepare stage (<code>pipeline/tools/prewarm_task_images.sh</code>), extended to x86-64 workers: pull, then start once with no network. A failure is reported as "store damaged", not as a task failure</div>
<div class="arrow"></div>
<div class="node disk"><b class="t">The worker's image store is a cache</b>It can be emptied and refilled from the registry at any time. Only the clean-up job removes images</div>
<div class="arrow"></div>
<div class="node people"><b class="t">Evaluation jobs through Jenkins</b>CPU locks (already in mac-k3d), plus memory as a limit when a job is admitted. During the transition, also by hand under the rules of 9.3</div>
<div class="arrow">checked every day</div>
<div class="row">
<div class="node fixed"><b class="t">Daily store check</b>A read-only root timer writes a status file: records against files and folders, canary images, and free space</div>
<div class="node fixed"><b class="t">Health gate</b>Jenkins takes the node offline and alerts the team when the check fails. After a repair it clears <code>$MAC_K3D_SHARE/env-check/</code> for that worker, so images are proven again</div>
</div>
<div class="row gap">
<div class="node fixed"><b class="t">Host guards</b><code>UUID=</code> fstab line; containerd and Docker require <code>/data</code>; empty, immutable mount point; mount only with the services stopped</div>
<div class="node fixed"><b class="t">Memory guards</b>A memory ceiling for containers and users; <code>systemd-oomd</code> acts early, so the host never hangs</div>
</div>
</div>

What this adds to mac-k3d, and why:

- **An image check on Linux workers too.** Today `prepare` does nothing for DeepSWE on x86-64: Harbor uses whatever image is in the shared store, broken or not. Builds #53 (6 Oct) and #61 (7 Oct) met the damage that way, and failed inside the run instead of before it.
- **Memory as an admission limit.** It is not one today ("RAM is not an admission limit" in the integration log). The 11 Oct crisis started with memory, not CPU.
- **Fewer writers.** Once most benchmarks run through Jenkins, the `docker` group can shrink to the Jenkins agent user and the host owner, and the store gets one writer instead of twelve.
- **Results stay trustworthy.** The health gate keeps evaluations off a damaged store. Clearing env-check results stops a cached "pass" from hiding a store that broke later.

## 10. Decisions needed

The read-only investigation is finished. Guide steps 0 to 9.5, 9.7 and 9.8 are done. Steps 9.3 and 9.4 were run on 16 Oct with `root.pem`, read-only, and the view of the system disk was removed. Nothing in the store has been repaired. These decisions are for the mentor and root. The repair in section 9 does not wait for the unanswered questions.

1. Approve the repair order in section 9.0, or change it. The plan to reserve is 8 hours with nobody using the server: one store, the 12 base images, the 113 published DeepSWE images, then the 418 jiuwenswarm images. Name the host owner for step 1, and the date.
2. mac-k3d jobs stay on `AGENT_LABEL=linux-Michael-Ubuntu` until that repair is approved and the start checks in section 9 pass.
3. Where the registry lives (on this host's `/data`, another host, or Alibaba Cloud's container registry), and who looks after it.
4. Who is the host owner, and whether the team agrees to the rules in 9.3 while people still use the worker directly.
5. A watched reboot that proves the UUID fstab line is a later announced step (section 9.2). Step 9.8 showed the line parses today (`0 parse errors, 0 errors`). No reboot has tested it. Do not reboot in order to approve this report.
6. The questions in guide step 9.6 stay open and do not block the repair: who mounted `/data` on 10 Sep, who restarted the host and mounted `/data` on 12 Oct, who rewrote fstab, who restarted containerd on 14 Oct, what xu's `--apply` scripts changed, and who owns the two split-brain containers.
7. Root removes the investigation access (guide step 8.3). The timer removes the sudo rule on 18 Oct at about 12:03 in any case. Whether the shared root key is retired is a separate decision.
8. Reading containerd 2.2's source (section 8, check 8) does not block the repair. It was never a command on the worker.
