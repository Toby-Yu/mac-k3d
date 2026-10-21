# Cloud worker repair: what was fixed

- **Host:** `iZt4ndd2dff7gqjta7mppaZ` (`47.84.16.18`).
- **Window:** 20 Oct 2026. The damage was found on 12 Oct and investigated on 15 and 16 Oct.
- **For:** Toby, root, and the mentor. The team message in repair step 14.3 is written and not sent.
- **This page:** the before-and-after record of the repair. The commands are in [`CLOUD_WORKER_REPAIR_STEPS.md`](CLOUD_WORKER_REPAIR_STEPS.md). The investigation that came first is listed at the bottom.
- **Status, 21 Oct 11:23 CST:** Docker is open. The prompt is back. Lolbench is 264 of 335 names back. All 15 `lolbench-linux` digests are on the server. `sha256:cec9f7d0…` printed `Status: Downloaded newer image` on the retry at 11:23. About 71 lolbench names are local builds and stay with the owner. capbench is 4 of 67. Those 63 names are local builds. Toby and root cannot pull or rebuild them.

## What the repair had to make true

A Docker image is consistent when three things hold in the `moby` namespace:

1. Every download record has its blob file.
2. Every blob file has a download record.
3. Every snapshot folder has a snapshot record, and no snapshot record that a current image uses is missing its folder.

`moby` is the only namespace Docker reads. A pull or a start does not look at `moby_history` or the recovery namespaces.

## Before and after

Before is the store check at the start of the repair window (16 Oct, step 2.3). After is the store check at 16:42 on 20 Oct (step 12.2), then the removal of 53 unused snapshot records at 17:16 (step 14.2c). A live recount at 17:22 is in the next section, because Docker was already open and the host had gained a few images.

| Item | 16 Oct, before | 20 Oct 16:42, after the loads | What changed |
|---|---:|---:|---|
| Download records, all namespaces | 82,010 | 56,736 | Broken image names and their records were removed |
| Blob files on disk | 4,278 | 4,151 | Files with no Docker image were not kept |
| Download records with no file | 77,732 | 52,585 | The drop is the removed Docker damage. The rest is `moby_history` and old recovery namespaces |
| Blob files with no record | 0 | 0 | Unchanged. No file is sitting on disk without a record |
| Snapshot records, all namespaces | 21,183 | 3,923 | Leftover layer records were removed |
| Snapshot folders | 10,409 | 2,710 | Folders for removed layers went with those records |
| Docker images (`moby`) | 8,057 | 932 | 932 images start. The names that could not start were removed |
| Leases in `moby` | 20,944 | 10,570 | Build-cache leases that held broken records were cleared |
| `/` used | 66% | 65% | |
| `/data` used | 98% | 96% | Still above the daily check's 85% warning |

At 17:16, step 14.2c removed 53 more snapshot records: the last 45 Docker records that had no folder, and 8 unused records sitting on them. No current image and no live container used them. The image count stayed 932.

## Docker, counted on its own

Recount at 20 Oct 17:22 CST, after Docker had been open for about half an hour. The image count is 941, nine more than at 17:16, so the host is taking new images again. The match still holds.

| Check | `moby` (Docker) | `moby_history` |
|---|---:|---:|
| Images | 941 | 0 |
| Download records | 4,119 | 52,626 |
| Download records with no file | 0 | 52,340 |
| Snapshot records listed by `ctr` | 2,581 | 0 |

Blob files on disk: 4,291. Every one has a record in some namespace. Snapshot folders: 2,770. Every one has a record.

## `moby_history`

`moby_history` is a second containerd namespace. It is not a Docker image list.

- It has **0 images** and **0 snapshot records**.
- It has about **52,340 download records whose files are gone**. Those files were deleted in the 12 Oct split: containerd's cleanup removed files that the database it had open did not name. The records stayed behind in this namespace.
- Docker pull and `docker run` read `moby` only. They do not download, skip, or mount anything because of `moby_history`.
- There is nothing to re-download. A record here is a name of a file that belonged to an old store, not an image someone can pull back.

On 21 Oct Toby asked for these records to be removed. They are not images, and nothing in Docker reads them.

That decision was made on 21 Oct. At 09:21 CST the `moby_history` namespace was deleted: 52,677 download records and 7,256 leases, 0 images, 0 snapshots. Docker was stopped for the edit and started again. The running container `jiuwen-deepseek-paired15-dind-20261015` was stopped by that and started again. It is up. `moby_history` is not the hidden store from 12 Oct. That hidden store was already empty. This namespace was only the leftover download records.

The old `lolbench-recovery-*` and `capbench-recovery-*` namespaces are still there. They are not Docker. Together they hold images of their own (205 names) and 228 download records whose files are gone. One `capbench-recovery-*` namespace still has 1,357 snapshot records, and those have folders. They were not deleted, because those images and folders are real data outside Docker.

The daily check still prints `RESULT WARN` because of those 228 recovery records and because `/data` is past 85% full. It is not a Docker image whose file is missing.

## What was fixed on the server

- One store, on the 14 TB disk. The hidden store on the system disk is empty, and the folder is immutable.
- `/etc/fstab` mounts that disk by UUID. containerd and Docker refuse to start unless `/data` is already mounted.
- `/usr/local/sbin/mount-data` refuses to mount `/data` while Docker or containerd is running.
- A read-only store check runs every day at 07:30 and writes `/var/lib/store-check/last.txt`.
- The Docker socket is `root:docker` mode `660` again. Toby's canary crontab is restored.
- The 12 base images start, including `alpine:3.23.4`.
- The 113 published DeepSWE images were saved, the stale layer records that blocked unpack were removed, and they were loaded again. All 113 start.
- 178 other saved images were loaded. 177 start by name. The expressvpn image had no repository name after the load; it starts by its image id.
- 932 images started in the full test at 16:39. The 53 leftover snapshot records were removed after that, at 17:16.

## Images, before the repair and after it

The counts below are from the repair log, not from a fresh listing. Before is the start test at 16 Oct 17:22, which listed every Docker name then on the host: **8,057**. After is the start test at 20 Oct 16:39: **932 names, and all 932 started**. At 17:22, after the socket was given back, a recount showed **941** images and still **0** Docker download records without a file. Those extra nine arrived because the host was in use again. Their names are not in the sealed list.

The [mentor report](CLOUD_WORKER_MENTOR_REPORT.md), section 9.0, is the 15 Oct inventory (7,750 names). The store grew before 16 Oct, so a few family totals differ. The repair removed from the 16 Oct list.

A row that says **removed** means the name is gone. The broken record went with the name. There is nothing left for that name that a later pull would skip.

| Group | 16 Oct, before | 20 Oct 16:39, after | What the repair did | Records now | If root wants them back |
|---|---:|---:|---|---|---|
| Published DeepSWE | 113 | 113 | Saved, the stale snapshot records removed, then loaded again | Present. All 113 started | Already back |
| Local images whose name contains `deepswe` | 606 | 120 | 120 still had their blob and were saved and loaded. 486 had no blob and were removed | The 120 started. The 486 names are gone | Pull the published ones. Rebuild the ones that were only built here |
| deepswe-ab-verifier, including 5 `deepswe-ab-grading-recovery1` | 113 | 0 | Removed. The blob was already gone | No name and no broken record left | The owner rebuilds or pulls, if these tags exist on a registry |
| jiuwenswarm | 418 | 2 | 416 had no blob and were removed. 2 still had a blob (`jiuwenswarm-bench-base` and `jiuwenswarm-probe-base`) and were loaded back | Those 2 started. The other 416 names are gone | The owner rebuilds. These are local builds, so `docker pull` cannot recreate them |
| chrys-bench | 539 | 5 | 534 had no blob and were removed. 5 `chrys-bench-base` tags still had a blob and were loaded back | Those 5 started. The other names are gone | The owner rebuilds the local tags, or pulls any tag that is on a registry |
| lolbench | 335 | 235 | 235 already started, so they were kept. 92 had no blob and were removed. 8 had a stale layer record and were removed | The 235 started. The 100 removed names are gone | Pull or rebuild the 100. Leave the 235 |
| capbench | 67 | 4 | The four `capbench-linux-sync/jdk-1`, `jdk-2`, `jdk-3` and `jdk-5` already started and were kept. 63 had no blob and were removed | Those 4 started. The 63 names are gone | The owner pulls or rebuilds the 63 |
| SWE-bench Pro | 426 | 0 | Removed. The blob was already gone | No name and no broken record left | Pull, if the registry still has the tags |
| SWE-bench, `jefzda/sweap-images` | 42 | 2 | 2 already started and were kept. 24 had no blob. 16 had a stale layer record. Those 40 were removed | The 2 started. The 40 names are gone | Pull the 40 |
| SWE-bench, `sweb.*` | 114 | 0 | Removed. The blob was already gone | No name and no broken record left | Pull |
| Official base images | 26 | 18 | The 12 tags evaluations ask for are back and started. See the next table. 14 other official tags had no blob and were removed | The 18 started. The removed tags have no broken record left | `docker pull` for any official tag that is still missing |
| Harbor and buildkit | 7 | 0 | 6 had no blob. 1 had a stale layer record. All 7 were removed | No name and no broken record left | Pull `harborframework/terminal-bench`, the egress sidecars, and `moby/buildkit` if a job needs them |
| Leftover `moby-dangling@` build tags | 903 | 0 | Removed. The blob was already gone. These were untagged build leftovers, not images a job pulls by name | No name and no broken record left | Nothing to pull. A new build creates new tags |
| Untagged ids created by the loads on 20 Oct | 0 | 107 | `docker load` added these. One is expressvpn, `sha256:60c4d5da…`, which had no repository name after the load | All 107 are inside the 932 that started | expressvpn is back by that id. The other 106 are untagged copies from the loads, not a missing benchmark |
| clarify task images | 422 | 12 | 410 had no blob and were removed. 12 still had a blob and were loaded back | Those 12 started | Rebuild the rest. They were built on this server |
| tdd task images | 452 | 306 | 268 already started and were kept. 38 were saved and loaded. 146 had no blob and were removed | The 306 started | Rebuild the 146 |
| dgm task images | 5 | 3 | `dgm-knowledge-k3-v2`, `v3` and `v4` already started and were kept. 2 had no blob and were removed | The 3 started | Rebuild the 2 if that owner still wants them |
| swe-marathon | 3 | 2 | Both `swe-marathon-cpu/daemon-bootstrap` tags already started and were kept. 1 had no blob and was removed | The 2 started | Rebuild or pull the one that was removed, if it is still needed |
| p3-replication | 3 | 0 | Removed. The blob was already gone | No name and no broken record left | The owner rebuilds |
| Other task images built on this server | 3,462 | 3 | 3,459 had no blob and were removed. The 3 that stayed are `proofke-evolving:lean428-20261015`, `proofke-evolving:recovery-20261016-a41527ebe650` and `remain178-dind-probe:29.6.2-flat` | Those 3 started | Rebuild. A typical removed name was one task image plus its verifier and egress copy |
| **Docker image names** | **8,057** | **932** | | Every one of the 932 started | |

expressvpn is the one saved name that did not come back under its old name `docker.io/misioslav/expressvpn@sha256:60c4d5da…`. It is the untagged id in the row above, and `docker run` by that id returned success.

### The 12 base images the evaluations ask for

All 12 are on the server and started on 20 Oct.

| Image | 16 Oct | What the repair did |
|---|---|---|
| `alpine:3.21`, `alpine:3.22`, `busybox:1.36`, `debian:bookworm` | Already started | Kept |
| `ubuntu:22.04`, `ubuntu:24.04`, `alpine:3.19` | The blob was gone | Removed, then pulled |
| `debian:bookworm-slim` | The blob was gone. It was not in the 8,057 names; jobs had already failed it | Pulled |
| `alpine:3.20`, `alpine:latest` | The layer record was stale | Removed, then pulled |
| `alpine:3.23.4`, `ghcr.io/astral-sh/uv:0.9.30-python3.13-bookworm` | The layer record was stale, and the names were not in the 8,057 | The hidden snapshot tree was removed, then both were pulled |

`ubuntu:22.04` and `ubuntu:24.04` are also present by digest. `alpine:3.23.4` is present by digest `sha256:5b10f432…`. `python:3.10-slim-bookworm` and `public.ecr.aws/ubuntu/ubuntu:24.04` already started on 16 Oct and were kept. They are outside this list of 12.

### Official images that are not those 12

These were removed because the blob was already gone. A `docker pull` of the same tag writes a new record and a new file together. Nothing of the old record remains to make that pull skip the download.

| Image | Names removed |
|---|---|
| `python` | `3.10-slim` by name and by digest, `3.11-slim`, `3.12-slim`, `3.6-slim-bullseye`, `3.9-slim-bullseye` |
| `eclipse-temurin` | 8, 11 and 17, each by digest |
| `maven` | three digest tags: `3-eclipse-temurin-17`, `3-eclipse-temurin-8`, `3.9-eclipse-temurin-11` |
| `condaforge/mambaforge` | `24.7.1-2` by digest |

Temurin and Maven were pulled on 21 Oct 09:08 and are on the server. All seven `python` names are on the server, including `python:3.11-slim@sha256:db3ff2e1…` pulled at 09:22 after the leftover snapshot was removed. `moby/buildkit:v0.24.0` and `condaforge/mambaforge:24.7.1-2` were pulled at 09:13. The current table is in "What is back".

### Build cache

The build cache is not an image list. On 20 Oct, `docker builder prune -af` reclaimed **1.738 TB**, and a second prune printed `Total: 0B`. Step 8.6 then removed **6,929** build-cache leases. Those leases were holding snapshot records that no current image used. The cache is empty. The next build creates a new cache. There is nothing to pull.

### Benchmarks that are not back

For every family whose after count is lower than its before count, the missing names were deleted. Checked at 17:22:

- In Docker's `moby` namespace, download records with no file: **0**.
- Blob files with no record: **0**.
- Every snapshot folder has a record.
- Every image name then on the host started at 16:39.

So a benchmark that has not been pulled back is absent, and its old record is absent too. The record is correct because it is gone. It is not a broken record waiting in the database. Root and the owner decide whether that image comes back. A pull is enough for a published tag. A rebuild is required for an image that was only ever built on this host, which is the case for jiuwenswarm and for most of the task images.

`moby_history` was separate. It had 0 images. Its 52,340 records without files were not these benchmarks, and Docker did not read them. At 09:21 on 21 Oct that namespace was deleted. The section above records that deletion.

### What "the jiuwenswarm owner rebuilds" means

jiuwenswarm images are benchmark images built on this host. The repair deleted the 416 broken names so they could not keep stale layer records alive. "Rebuild" means the person who owns that benchmark runs their own build command again. That command creates new image names, new download records, and new snapshot folders in one pass. Two jiuwenswarm tags are already back, the two whose files were still on disk.

That rebuild is how those images come back, if root and that owner want them. The store is already consistent without them.

### Can people use Docker on this server?

Yes. Docker and containerd are running. They were started again on 16 Oct, after the store was made one disk, and they were left running. At 20 Oct 16:53 the socket was given back to the `docker` group: `root:docker`, mode `660`. Toby's account could see Docker 29.6.2. Anyone in the `docker` group can use Docker on this host.

What is still held back:

- The Jenkins node `linux-iZt4ndd2dff7gqjta7mppaZ` is online. Toby confirmed that on 21 Oct. beacon jobs stay on `AGENT_LABEL=linux-Michael-Ubuntu` until the mentor moves them.
- Twelve containers that were running before the window are stopped. Four are `jiuwen*` dind containers on `swe-marathon-cpu/daemon-bootstrap:20261012-r3-v2`, plus `remain178-one-image-probe`, plus seven `dgm-container-20261013_*` containers. Their owners start them if they still need them. The image behind the swe-marathon containers is one of the two tags that were kept.
- A reboot has not been done. Docker does not need another restart before people use it. The watched reboot, step 18, is the later test that `/data` mounts before containerd at boot.

Images that were removed are simply not here. Using Docker does not bring them back. Root decides, with each owner, which families to pull or rebuild.

## What is back

The reference count is how many names of that family were on this server on 16 Oct, before the removal. It is the set this host had, not every tag the registry publishes. A group is fully fixed when every name in that reference set is on the server again. **Yes** in the table below means that. For a row that is not fully fixed, the last column says whether Toby or root can finish it. Toby can pull a published image, and he can retag an amd64 image that is already on this host. Root decides the later published images, and Toby pulls those if root asks. A local build stays with that benchmark's owner. Toby does not have the Dockerfile or the build command, and root cannot recreate the image from the name alone.

| Family | Reference, on this server 16 Oct | Back now | Fully fixed | Who finishes the rest |
|---|---:|---:|---|---|
| Evaluation bases (`alpine`, `ubuntu`, `debian`, `busybox`, `uv`) | 12 | 12 | Yes | Done |
| Published DeepSWE, `public.ecr.aws/d3j8x8q7/swe-bench-202605` | 113 | 113 | Yes | Done. Saved and loaded on 20 Oct |
| Official `python` tags that were on this host | 7 | 7 | Yes | Done. Pulled 21 Oct |
| Official `eclipse-temurin` | 3 | 3 | Yes | Done. Pulled 21 Oct 09:08 |
| Official `maven` | 3 | 3 | Yes | Done. Pulled 21 Oct 09:08 |
| `moby/buildkit` | 1 | 1 | Yes | Done. Pulled 21 Oct 09:13 |
| `condaforge/mambaforge` | 1 | 1 | Yes | Done. Pulled 21 Oct 09:13 |
| lolbench, whole family | 335 | 264 | No | Toby's published pulls are done. At 10:53 he retagged 8 `smartdub26/lolbench` names from the amd64 `lolbench-linux` images already here. All 15 digests are on the server. `sha256:cec9f7d0…` came back at 11:23 after a `503`. About 71 local names stay with the owner: `lolbench-rebuild`, `lolbench-recovery`, local deepswe tags, and `jdk_*-reference-hardened` |
| `jefzda/sweap-images` | 42 | 2 | No | Toby, by pull, after lolbench. 40 tags. beacon's swebenchpro pipeline uses this repository |
| Published SWE-bench `sweb.eval` | 42 | 0 | No | Toby, by pull of `swebench/sweb.eval.x86_64.<id>:latest`. 40 were stored here under `library/` |
| `swebench-pro-opencode` | 424 | 0 | No | Owner. These are per-run images. Toby can pull one only if that account still publishes the tag |
| jiuwenswarm | 418 | 2 | No | Owner rebuild. 416 names. Toby does not have the build command |
| chrys-bench | 539 | 5 | No | Owner rebuild. 534 names. Five `chrys-bench-base` tags are back |
| Local `deepswe*` | 606 | 120 | No | Owner rebuild for the 486 local names. Toby pulls a tag only when it is actually published |
| deepswe-ab-verifier | 113 | 0 | No | Owner rebuild |
| capbench | 67 | 4 | No | Owner rebuild for the 63. They are local names (`capbench-r08-base`, `capbench-r08-coverage`, `capbench-r08-dind`, and the other `capbench-linux-sync/jdk` tags). Toby and root cannot pull them. The four kept images are `jdk-1`, `jdk-2`, `jdk-3`, and `jdk-5` |
| `mirror.gcr.io/curlimages/curl` | 1 | 1 | Yes | Done. Pulled 21 Oct 09:03 |
| `harborframework/terminal-bench` | 4 | 0 | No | Root decides. Toby pulls the 4 digests if root asks |
| `ghcr.io/principled-systems/verus`, `xtrm0/dafny`, `xinzhou1997` | 1 + 1 + 3 | 0 | No | Root decides. Toby pulls them if root asks |

### Lolbench tags pulled at 09:27

Eight names. Four are on the server. Four are published only for `linux/arm64`. This host is `x86_64`. The registry index for `smartdub26/lolbench:TypeScript_2-1.0.0` lists `linux/arm64` and an attestation, and no `linux/amd64` manifest. The same platform error came back for `aspnetcore_1`, `cargo_1`, and `rust_1`. Pulling them with another platform flag would download an arm64 image that cannot run here. The `lolbench-linux` tag of each name is the image for this host, and those four downloaded.

| Image | Result, 21 Oct 09:27 |
|---|---|
| `smartdub26/lolbench-linux:TypeScript_2-1.0.0` | Done. Digest `sha256:d206ccf2…` |
| `smartdub26/lolbench-linux:aspnetcore_1-1.0.0` | Done. Digest `sha256:bb95c42a…` |
| `smartdub26/lolbench-linux:cargo_1-1.0.0` | Done. Digest `sha256:8637e8c0…` |
| `smartdub26/lolbench-linux:rust_1-1.0.0` | Done. Digest `sha256:92260ffb…` |
| `smartdub26/lolbench:TypeScript_2-1.0.0` | Not pulled. Registry has `linux/arm64` only |
| `smartdub26/lolbench:aspnetcore_1-1.0.0` | Not pulled. Registry has `linux/arm64` only |
| `smartdub26/lolbench:cargo_1-1.0.0` | Not pulled. Registry has `linux/arm64` only |
| `smartdub26/lolbench:rust_1-1.0.0` | Not pulled. Registry has `linux/arm64` only |

### Lolbench tags from 09:38

Six names. Two downloaded. Four `smartdub26/lolbench` jdk tags are `linux/arm64` only, the same platform error as 09:27. `jdk_1` was inspected: the index lists `linux/arm64` and an attestation. `lolbench-linux:jdk_1-1.0.0` is `linux/amd64`, and that image, with `jdk_2`, `jdk_3`, and `jdk_5`, was already on the server. Do not pull the four `lolbench` jdk tags again.

| Image | Result, 21 Oct 09:38 |
|---|---|
| `smartdub26/lolbench-linux:arrow_2-1.0.0` | Done. `Status: Downloaded newer image` |
| `smartdub26/lolbench:cpython_5-1.0.0` | Done. Digest `sha256:81c5d064…` |
| `smartdub26/lolbench:jdk_1-1.0.0` | Not pulled. Registry has `linux/arm64` only. `lolbench-linux:jdk_1-1.0.0` is already on the server |
| `smartdub26/lolbench:jdk_2-1.0.0` | Not pulled. Registry has `linux/arm64` only. `lolbench-linux:jdk_2-1.0.0` is already on the server |
| `smartdub26/lolbench:jdk_3-1.0.0` | Not pulled. Registry has `linux/arm64` only. `lolbench-linux:jdk_3-1.0.0` is already on the server |
| `smartdub26/lolbench:jdk_5-1.0.0` | Not pulled. Registry has `linux/arm64` only. `lolbench-linux:jdk_5-1.0.0` is already on the server |

### Official `python` and `curl`, image by image

Seven `python` names were on this host. All seven are back. `curl` was one image, and it is back.

| Image | Result, 21 Oct 09:03 |
|---|---|
| `python:3.10-slim-bookworm` | Done. It already started on 16 Oct and was kept |
| `python:3.10-slim` | Done. `Status: Downloaded newer image`. Current digest `sha256:6ff50646…` |
| `python:3.10-slim@sha256:c1e4e6c0…` | Done. `Status: Downloaded newer image` |
| `python:3.12-slim` | Done. `Status: Downloaded newer image`. Digest `sha256:a6e34c59…` |
| `python:3.6-slim-bullseye@sha256:2cfebc27…` | Done. `Status: Downloaded newer image` |
| `python:3.9-slim-bullseye@sha256:b9e06687…` | Done. `Status: Downloaded newer image` |
| `python:3.11-slim@sha256:db3ff2e1…` | Done. Pulled 21 Oct 09:22 after the leftover snapshot was removed. `Status: Downloaded newer image`. Stored as `python@sha256:db3ff2e1…` |
| `mirror.gcr.io/curlimages/curl:8.17.0` | Done. `Status: Downloaded newer image`. Digest `sha256:935d9100…` |

### Official `eclipse-temurin` and `maven`, image by image

Three temurin tags and three maven tags were on this host. All six are back. Pulled at 21 Oct 09:08 from `/var/log/cloud-worker-repair/2026-10-20-pulls/out/19.1b_pull.log`.

| Image | Result, 21 Oct 09:08 |
|---|---|
| `eclipse-temurin:8-jdk` | Done. Digest `sha256:e80636b3…` |
| `eclipse-temurin:11-jdk` | Done. Digest `sha256:74983af8…` |
| `eclipse-temurin:17-jdk` | Done. Digest `sha256:fe947d99…` |
| `maven:3-eclipse-temurin-17` | Done. Digest `sha256:a6d08f0b…` |
| `maven:3-eclipse-temurin-8` | Done. Digest `sha256:b9fac829…` |
| `maven:3.9-eclipse-temurin-11` | Done. Digest `sha256:7bb84a45…` |

### Why these six could not be pulled before this morning

On 16 Oct the blob files for these six images were already gone. The download records were still in the database. `docker pull` treats an existing download record as "already downloaded" and skips that file. A pull while those records were still there would have skipped the missing files, and the image would still not start. The repair deleted those records because the files were already gone. After that, a pull writes a new record and a new file together.

They were also not in the 20 Oct base pull. That pull was the 12 evaluation tags (`alpine`, `ubuntu`, `debian`, `busybox`, `uv`). Temurin and Maven waited until the store check was finished.

This morning's pull had no old record to skip. All six printed `Status: Downloaded newer image`. None printed `AlreadyExists`. These six were not blocked by an incorrect record.

### Why the leftover snapshot records were still there, and what was removed

The `python:3.11-slim` pull at 09:03 stopped with `AlreadyExists` on snapshot `sha256:b9b037ff…`. `ctr snapshots info` said that snapshot does not exist. Both were true, for two different databases.

`ctr` reads the overlay snapshot database. That database had no such snapshot, and the folder was gone. Docker's unpack reads containerd's own database, `meta.db`. The name was still there. A pull refuses to unpack when that name exists, even though the folder is gone. Step 14.2c on 20 Oct removed 53 overlay records that `ctr` could see. It could not remove these, because `ctr snapshots rm` returns "does not exist" for a name that is only in `meta.db`.

They stayed because 1,344 leases in `moby` still pointed at them. Containerd's cleanup will not drop a snapshot name while a lease points at it. There were 1,623 snapshot names in `meta.db` with no overlay snapshot behind them. 504 of those names were `sha256:` layer ids, including the one that blocked `python:3.11`. None of them was the parent of a snapshot that still has a folder. None was a running container's snapshot. No current image's manifest pointed at one.

At 09:21 those 1,623 names were deleted, with 1,604 child links, 1,344 lease links, and 16 stale labels. `moby_history` was deleted in the same edit. A copy of `meta.db` from just before the edit is at `/var/log/cloud-worker-repair/2026-10-21-clean/meta.db.before`. After the edit, snapshot names with no overlay snapshot: 0. Docker download records with no file: 0. Image count in `moby`: 964, which is the 963 from before this pull plus `python:3.11`.

`python:3.11-slim@sha256:db3ff2e1…` then printed `Status: Downloaded newer image`.

What is still not a fully quiet daily check:

- 20 recovery namespaces, not used by Docker, with 205 of their own images and 228 download records whose files are gone. One capbench namespace has 1,357 snapshot folders. Those were left.
- `/data` is 97% full, about 424 GB free after the lolbench digest pulls. The daily check warns above 85%.

## What Toby can pull

Toby's published lolbench pulls are done. The shared bases are back. At 10:53 the 8 arm64-only `smartdub26/lolbench` names were retagged from the amd64 images already here. At 11:23 all 15 `lolbench-linux` digests were on the server. `sha256:cec9f7d0…` printed `Status: Downloaded newer image` on the retry. The remaining lolbench names and all 63 missing capbench names are local builds. Toby and root cannot rebuild those. `jefzda/sweap-images` waits until that benchmark is the one to run.

| Order | Need | What | Images | Who | Time |
|---|---|---|---:|---|---|
| 1 | Must. Shared bases | Official `python` 7 of 7, `eclipse-temurin` 3 of 3, `maven` 3 of 3 | 12 names | Toby. Done | Done |
| 2 | Likely. `docker build` uses buildkit. mambaforge is the published conda base | `moby/buildkit:v0.24.0`, `condaforge/mambaforge:24.7.1-2` | 2 | Toby. Done. Pulled 21 Oct 09:13 | Done |
| 3 | Likely. The lolbench tags the investigation named as stale | `lolbench-linux` `TypeScript_2`, `aspnetcore_1`, `cargo_1`, `rust_1` are back. The four `smartdub26/lolbench` tags were retagged at 10:53 | 8 | Toby. Done | Done |
| 4 | Likely. The rest of the published lolbench tags | `arrow_2` and `cpython_5` are back. Four `lolbench` jdk tags were retagged at 10:53 from the `lolbench-linux` images already here | 6 | Toby. Done | Done |
| 4b | lolbench, before any other benchmark | 15 `lolbench-linux` digests, still published as `linux/amd64` | 15 | Toby. Done at 11:23. `sha256:cec9f7d0…` needed one retry after a `503` | Done |
| 4c | lolbench names Hub now publishes only as arm64 | Retag the amd64 `lolbench-linux` image onto `smartdub26/lolbench` for the 8 tags | 8 names | Toby. Done at 10:53. The image was already on the server | Done |
| 4d | lolbench local builds, and capbench | About 71 lolbench names (`lolbench-rebuild`, `lolbench-recovery`, local deepswe tags, `jdk_*-reference-hardened`) and 63 capbench names (`capbench-r08-base`, `capbench-r08-coverage`, `capbench-r08-dind`, and `capbench-linux-sync/jdk` other than 1, 2, 3, 5) | 71 + 63 | Owner. Toby and root cannot pull or rebuild these | Local builds. The files were deleted |
| 5 | Likely when swebenchpro runs. beacon's pipeline pulls `sweap-images` | 40 tags of `jefzda/sweap-images`. Two other tags already start | 40 | Toby, by pull, after lolbench | 2–4 h |
| 6 | Likely when classic SWE-bench runs | 42 images, pulled as `swebench/sweb.eval.x86_64.<id>:latest` | 42 | Toby, by pull | 4–8 h |
| 7 | Later. `curl:8.17.0` is already back. The others run only if root asks | `verus:latest`, `dafny:latest`, two `xinzhou1997` tags, four `harborframework/terminal-bench` digests | 8 still open | Root decides. Toby pulls if root asks | 30–90 min |

Orders 1 to 4 are done. Orders 5 and 6 are the long ones. Start 5 or 6 when `/data` has at least 200 GB free. After the lolbench digest pulls, about 424 GB was free. A tag the registry no longer has fails in seconds.

These stay with their owners. Toby does not have the build, and a pull of the local name will not find a published image. Root cannot rebuild them either.

| Family | Names still missing | Who brings them back |
|---|---:|---|
| lolbench local names | About 71 (`lolbench-rebuild`, `lolbench-recovery`, local deepswe tags, `jdk_*-reference-hardened`) | The owner rebuilds. The published `smartdub26` names are Toby's pulls, above |
| jiuwenswarm | 416. Two tags are already back | The owner rebuilds |
| chrys-bench | 534. Five `chrys-bench-base` tags are already back | The owner rebuilds |
| Local `deepswe*` | 486. 120 are already back | The owner rebuilds, or Toby pulls a tag that is actually published |
| deepswe-ab-verifier | 113 | The owner |
| capbench, other than the four `jdk` images that were kept | 63 | The owner. Toby and root cannot pull these |
| Task images: clarify, tdd, dgm, p3-replication, and the other local builds | The removed counts are in the table above | The owner rebuilds |
| `swebench-pro-opencode` | 424 per-run agent images | The owner. Toby can pull one only if that account still publishes the tag |
| `library/sweb.openlux.*`, `sweb.env.*`, `sweb.base.py.x86_64` | 74 | Local names. They are not the published `swebench/sweb.eval` images Toby can pull |

## What still waits, and what does not

Done for the consistency of Docker:

- At 20 Oct 17:22, records and files in `moby` matched, and every image then on the host started.
- The guards against the 12 Oct split are installed.
- At 21 Oct 09:21 the 1,623 snapshot names that had no overlay snapshot were removed, and `moby_history` was removed. `moby` again has 0 download records without a file and 0 snapshot names without an overlay snapshot.

Not part of that consistency fix:

- Step 14.3, the team message. The local steps are done. Toby sends it when he is ready.
- Step 15, the jiuwenswarm rebuild. The owner's build, when they want those images.
- Step 16.4, the report to root and the mentor. Same wait as the team message.
- Step 17, the Jenkins node. Toby confirmed on 21 Oct that `linux-iZt4ndd2dff7gqjta7mppaZ` is already online. Moving beacon jobs onto it is still the mentor's decision.
- Step 18, a watched reboot. It proves the fstab line at boot. It is a later window, not a record fix.
- Step 19, the remaining pulls in "What Toby can pull". The 8 arm64-only lolbench names were retagged at 10:53. All 15 `lolbench-linux` digests are on the server as of 11:23. The local lolbench names and the 63 capbench names stay with their owners.

Step 16.3 passed at 17:47. The copy `~/cloud-worker-evidence/repair/2026-10-16.tar.gz` matches the server archive: `b629f4e14403f1f3067b3e847efc38a3039c377427c0388c39dc7d125e846373`. There is no further command on the server. Leave 14.3 unmarked.

## The documents

| File | Purpose |
|---|---|
| [`CLOUD_WORKER_INVESTIGATION_STEPS.md`](CLOUD_WORKER_INVESTIGATION_STEPS.md) | How the read-only investigation was run. It does not change the store |
| [`CLOUD_WORKER_DOCKER_REPORT.md`](CLOUD_WORKER_DOCKER_REPORT.md) | What was found: the 12 Oct split, the missing files, the evidence |
| [`CLOUD_WORKER_MENTOR_REPORT.md`](CLOUD_WORKER_MENTOR_REPORT.md) | Cause, the proposed repair, and the decisions for the mentor and root |
| [`CLOUD_WORKER_REPAIR_STEPS.md`](CLOUD_WORKER_REPAIR_STEPS.md) | The commands Toby follows on the server, one step at a time |
| [`CLOUD_WORKER_REPAIR_REPORT.md`](CLOUD_WORKER_REPAIR_REPORT.md) | This page. Before and after, what is fixed, and what is left for owners |

Each `.md` has a `.html` next to it, made by `python3 cloud_worker_docs_html.py`.
