# Cloud worker investigation: step by step

- **For:** Toby, on the cloud worker `iZt4ndd2dff7gqjta7mppaZ` (`47.84.16.18`).
- **Goal:** collect the evidence that answers Q1 to Q17 in `CLOUD_WORKER_DOCKER_REPORT.md`, without changing anything in the Docker store. Steps 0 to 9.5, 9.7 and 9.8 are done (15 and 16 Oct). Step 9.6 is a written question; no answer is recorded. Step 8.3, removing the investigation access, is a decision for root in the mentor report, section 10.
- **Record:** each step's result is in the investigation log (`CLOUD_WORKER_DOCKER_REPORT.md`, section 6). No further worker command is part of this investigation.
- **Summary for the mentor:** [`CLOUD_WORKER_MENTOR_REPORT.md`](CLOUD_WORKER_MENTOR_REPORT.md).

Nothing in this guide repairs, removes or restarts anything. The only changes to the server are:
- step 1: a password for `toby` and one sudo rule;
- steps 6.4 and 9.3: an empty folder and a temporary read-only view.

All of them are small and can be undone.

## Before you start

### How each step is written

- **Where:** the terminal to use: your PC, root on the worker, or `toby` on the worker.
- **Why:** what the step tells us, and how it fits the investigation.
- **Risk:**
  - **none:** it only reads, and writes only to your own folder `~/snapshot-audit/`;
  - **low:** a small change that removes no data and can be undone;
  - **high:** no limits. Only step 1.1 is high, and only because it opens a root shell.
- **Command:** copy the whole grey block and paste it into the terminal. Lines that start with `#`, and text after `#` at the end of a line, are comments; the shell ignores them.
- **Expected:** what you should see.
- **Send me:** what to paste back into the chat.

### Ground rules

1. Copy the commands exactly. Don't edit them, and don't add `sudo` where there is none.
2. Wait for the prompt to come back before pasting the next block.
3. If the output differs from **Expected**, stop and send me the output. Don't try a fix.
4. Never paste a password, or the contents of `root.pem`, into the chat or anywhere else. Fingerprints (`SHA256:…`) aren't secret and are fine to share.
5. sudo asks for `toby`'s password. Nothing appears while you type it; that is normal.

### Know where you are

The prompt shows which machine you are on, and as which user:

| Prompt ends with | Where you are | Care |
|---|---|---|
| `Toby@<your PC>:~$` | Your own PC | Normal |
| `toby@iZt4ndd2dff7gqjta7mppaZ:~$` | The worker, as `toby` | Normal. sudo allows only the 17 commands of step 1 |
| `root@iZt4ndd2dff7gqjta7mppaZ:~#` | The worker, as root | No limits. Type only the commands of step 1, then `exit` |

Use two terminals on your PC:

- **Terminal 1:** the worker as `toby`, from step 2 onwards, for most of the work.
- **Terminal 2:** your PC, for steps 0 and 1, and to check on long jobs.

### What you are looking at: the store in five points

1. **Where it lives.** Docker on this host keeps its images through containerd, in `/data/containerd`. An image is a stack of layers.
2. **Each layer exists twice:**
   - **Downloaded file (a "blob"):** the compressed layer as pulled from the registry, in `/data/containerd/io.containerd.content.v1.content/blobs/sha256/<digest>`.
   - **Unpacked folder (a "snapshot"):** the same layer unpacked, so that a container can use it, in `/data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots/<number>/fs`.
3. **containerd keeps records of both, in two databases:**
   - `io.containerd.metadata.v1.bolt/meta.db` for images and downloaded files;
   - `io.containerd.snapshotter.v1.overlayfs/metadata.db` for unpacked folders.
4. **The damage on this host always has the same shape: a record whose data is gone.** There are three kinds:
   - a downloaded file is missing (`blob not found`);
   - an unpacked folder is missing (`no such file or directory`);
   - a record has no usable data behind it (`already exists`, `missing parent`).
5. **containerd's cleanup deletes data that no record points to.** That is safe while the records are right. On 12 Oct, containerd ran for about an hour with the database of an old store on the system disk, while the real disk was mounted on top of it. Its cleanup deleted real folders that this database did not list (step 6.4).

The investigation asks when, and by what, data disappeared while its records stayed. The evidence comes from three places:

- the logs: who logged in, what restarted, when the host ran out of memory;
- containerd's records, compared with what is on disk;
- the ages of the missing files.

## Step 0: make `root.pem` safe, and test it (your PC, 5 minutes)

### 0.1 Move the key to `~/.ssh` and make it private

- **Where:** terminal 2, your PC.
- **Why:** `root.pem` is in `~/Downloads`, and every account on your PC can read it (`-rw-rw-r--`). Whoever has it can log in as root on the worker. `ssh` also refuses a key that others can read.
- **Risk:** none. It moves one file on your PC and changes its permissions.

```bash
mkdir -p ~/.ssh && chmod 700 ~/.ssh                       # your private SSH folder
mv ~/Downloads/root.pem ~/.ssh/cloud-worker-root.pem      # out of Downloads, with a clear name
chmod 600 ~/.ssh/cloud-worker-root.pem                    # readable by you only
ls -l ~/.ssh/cloud-worker-root.pem
```

- **Expected:** `-rw------- 1 Toby Toby … /home/Toby/.ssh/cloud-worker-root.pem`.
- **Send me:** the `ls` line.

### 0.2 Show the key's fingerprint

- **Where:** terminal 2, your PC.
- **Why:** the fingerprint identifies the key without revealing it. Root can confirm it's the key he gave you, and step 6.1 later shows which root logins used it.
- **Risk:** none. It reads the key and prints only its fingerprint.

```bash
ssh-keygen -lf ~/.ssh/cloud-worker-root.pem
```

- **Expected:** one line like `2048 SHA256:… (RSA)`. If it asks for a passphrase, root has to give it to you.
- **Send me:** that line.

### 0.3 Test the key without opening a shell

- **Where:** terminal 2, your PC.
- **Why:** check that the key logs in as root on this worker, before you rely on it in step 1.
- **Risk:** none. It logs in as root, runs three commands that only print, and logs out. `IdentitiesOnly=yes` makes ssh offer this key only.

```bash
ssh -i ~/.ssh/cloud-worker-root.pem -o IdentitiesOnly=yes root@47.84.16.18 'whoami; hostname; date'
```

- **Expected:** `root`, `iZt4ndd2dff7gqjta7mppaZ` and the current date.
  - If you see `Permission denied (publickey)`, the key isn't accepted for root on this host. Stop, and give root the fingerprint from 0.2.
  - If you see `UNPROTECTED PRIVATE KEY FILE`, run 0.1 again.
- **Send me:** the output.

## Step 1: install the investigation-only sudo rule (root on the worker, 10 minutes)

The rule lets `toby` run 17 exact commands as root, each with fixed arguments, and nothing else. It needs `toby`'s password, so Jenkins jobs (which also run as `toby`) can't use it. After this step you don't need `root.pem` again for the investigation.

**First, message root** and wait for an OK:

```text
I'll use root.pem once now to install the investigation-only sudo rule from
CLOUD_WORKER_DOCKER_REPORT.md section 4.2 (17 read-only commands, password required,
removed automatically after 3 days). I'll set a password for toby, run nothing else
as root, and log out. You can check it with: sudo -l -U toby
```

### 1.1 Open a root shell

- **Where:** terminal 2.
- **Why:** installing a sudo rule needs root.
- **Risk:** **high.** A root shell has no limits. Type only steps 1.2 to 1.7, then step 1.8 (`exit`).

```bash
ssh -i ~/.ssh/cloud-worker-root.pem -o IdentitiesOnly=yes root@47.84.16.18
```

- **Expected:** the prompt `root@iZt4ndd2dff7gqjta7mppaZ:~#`.

### 1.2 Give `toby` a password

- **Why:** sudo asks for it; `toby`'s password is locked now.
- **Risk:** low. It changes only `toby`'s password. Password logins over SSH are off for every account, so it opens no new way in. It can be undone with `passwd -l toby`.

```bash
passwd toby
```

- **Expected:** it asks for the new password twice, without showing it, then prints `passwd: password updated successfully`. Choose a password you can type often; sudo asks for it every 15 minutes.

### 1.3 Write the rule to a temporary file

- **Why:** prepare the rule, so that it can be checked before it is installed. Each line is one command `toby` may run. Section 4.2 of the report explains each one.
- **Risk:** none. It writes one file in `/tmp`, which nothing reads yet.

```bash
cat > /tmp/90-toby-investigate <<'EOF'
# Investigation of the Docker store: exact, read-only commands only
Cmnd_Alias MK3D_INVESTIGATE = \
    /usr/bin/ctr -n moby images check, \
    /usr/bin/ctr -n moby images ls -q, \
    /usr/bin/ctr -n moby snapshots ls, \
    /usr/bin/ctr -n moby content ls, \
    /usr/bin/ctr -n moby content active, \
    /usr/bin/ls -l --full-time /data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots, \
    /usr/bin/ls -U /data/containerd/io.containerd.content.v1.content/blobs/sha256, \
    /usr/bin/ls -ld --full-time /data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots/37 /data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots/54, \
    /usr/bin/ssh-keygen -lf /root/.ssh/authorized_keys, \
    /usr/bin/cat /root/.bash_history, \
    /usr/bin/grep -rH . /var/spool/cron/crontabs, \
    /usr/bin/ls -laR /home/naiyu/.config/systemd/user /home/yun/.config/systemd/user /home/zihan/.config/systemd/user, \
    /usr/bin/mkdir -p /mnt/rootfs, \
    /usr/bin/mount -o bind\,ro / /mnt/rootfs, \
    /usr/bin/ls -la --full-time /mnt/rootfs/data /mnt/rootfs/data/containerd /mnt/rootfs/data/containerd/io.containerd.metadata.v1.bolt, \
    /usr/bin/du -sh /mnt/rootfs/data, \
    /usr/bin/umount /mnt/rootfs
toby ALL=(root) MK3D_INVESTIGATE
EOF
```

- **Expected:** no output; the prompt comes back.

### 1.4 Check the rule, and install it only if the check passes

- **Why:** a broken sudoers file could stop sudo working for everyone. `visudo -c` checks the file, and `&&` installs it only if the check passed.
- **Risk:** low. It adds one file to `/etc/sudoers.d/`, which `rm -f /etc/sudoers.d/90-toby-investigate` removes again.

```bash
visudo -c -f /tmp/90-toby-investigate && install -m 0440 -o root -g root /tmp/90-toby-investigate /etc/sudoers.d/90-toby-investigate
```

- **Expected:** `/tmp/90-toby-investigate: parsed OK`. If it prints anything else, stop: nothing was installed. Send me the output.

### 1.5 Remove the temporary file

- **Risk:** none. It removes only the file from 1.3.

```bash
rm -f /tmp/90-toby-investigate
```

- **Expected:** no output.

### 1.6 Show what `toby` may now run

- **Why:** confirm that the rule is active, and that it's the only one.
- **Risk:** none. It only reads.

```bash
sudo -l -U toby
```

- **Expected:** the 17 commands of 1.3, run as `(root)`, and nothing else.
- **Send me:** the output.

### 1.7 Remove the rule automatically after 3 days

- **Why:** a backstop, in case removing it by hand (step 8.3) is forgotten.
- **Risk:** low. In 3 days it deletes only the file from 1.4.

```bash
systemd-run --on-active=3d --unit=toby-investigate-expire /usr/bin/rm -f /etc/sudoers.d/90-toby-investigate
```

- **Expected:** `Running timer as unit: toby-investigate-expire.timer`.

### 1.8 Leave the root shell

- **Risk:** none.

```bash
exit
```

- **Expected:** the prompt of your PC again. Don't use `root.pem` again for the investigation. Keep it in `~/.ssh`, and don't delete it until root says so.

**Then message root:** "Done at <time>: rule `/etc/sudoers.d/90-toby-investigate` installed, timer `toby-investigate-expire` set for 3 days. Nothing else changed."

## Step 2: start work as `toby` (5 minutes)

### 2.1 Log in as `toby`

- **Where:** terminal 1.
- **Risk:** none.

```bash
ssh toby@47.84.16.18
```

- **Expected:** the prompt `toby@iZt4ndd2dff7gqjta7mppaZ:~$`.

### 2.2 Check your groups and your sudo rule

- **Why:**
  - `adm` lets you read `/var/log`; `systemd-journal` lets you read the journal; `docker` lets you run containers.
  - `sudo -l` shows the rule from step 1.
  - `sudo true` shows that anything outside the rule is refused.
- **Risk:** none. `sudo true` is refused, and `true` does nothing anyway.

```bash
id
sudo -l
sudo true
```

- **Expected:**
  - `id` lists `adm`, `docker` and `systemd-journal`.
  - `sudo -l` asks for the password from 1.2 and lists the 17 commands.
  - `sudo true` is refused with a message that `toby` may not run it.
- **Send me:** the output, without the password; it isn't shown anyway.

### 2.3 Create your folders, and start recording the session

- **Why:** all evidence goes into `~/snapshot-audit/`. `script` records everything that appears on screen into `sessions/`, so root and the mentor can check what was run. Passwords aren't shown, so they aren't recorded.
- **Risk:** none.

```bash
mkdir -p ~/snapshot-audit/logs ~/snapshot-audit/inventory ~/snapshot-audit/logcopy ~/snapshot-audit/sessions
cd ~/snapshot-audit/logs
script -q -a ~/snapshot-audit/sessions/"$(date +%F_%H%M)".log
```

- **Expected:** no output; a new prompt. Everything is recorded until step 8.1.
- **If the connection drops later:** log in again, run this block again, and carry on. The background jobs of step 3 keep running.

### 2.4 Save the two scripts

- **Why:**
  - `long_scans.sh` does the two slow log scans: Q6, when each error first appeared, and Q10, what filled syslog on 4 to 11 Oct.
  - `layer_test.sh` starts every image once, to sort the broken ones by type of damage.
  
  Neither uses sudo, and neither removes anything.
- **Risk:** none. It writes two files in your folder, and `bash -n` checks them without running them.

```bash
cat > ~/snapshot-audit/long_scans.sh <<'EOF'
#!/bin/bash
# The two slow log scans: Q6 (first appearance of each error) and Q10 (what filled syslog on 4-11 Oct).
# Reads the logs and writes only into ~/snapshot-audit/logs. Run before the weekly rotation on Sunday 18 Oct.
cd ~/snapshot-audit/logs || exit 1
echo "start $(date -Is)"
zgrep -hiE '(containerd|dockerd)\[[0-9]+\]: .*(blob not found|failed to stat parent|snapshot.*already exists|missing parent|no space left)' \
  /var/log/syslog.4.gz /var/log/syslog.3.gz /var/log/syslog.2.gz /var/log/syslog.1 /var/log/syslog > damage_errors.txt
for k in 'blob not found' 'failed to stat parent' 'already exists' 'missing parent' 'no space left'; do
  l=$(grep -m 1 -i "$k" damage_errors.txt)
  printf '== %s: %s\n' "$k" "${l:-never}" | cut -c1-250
done > damage_first_seen.txt
echo "Q6 done $(date -Is)"
awk '{d = substr($1, 1, 10); b[d] += length($0) + 1; p = $3; sub(/\[.*/, "", p); sub(/:$/, "", p); n[p]++}
     END {for (d in b) printf "day %s %d MB\n", d, b[d] / 1048576; for (p in n) printf "prog %d %s\n", n[p], p}' \
  /var/log/syslog.1 > syslog_volume.txt
echo "Q10 done $(date -Is)"
EOF
cat > ~/snapshot-audit/layer_test.sh <<'EOF'
#!/bin/bash
# Start each image once, with no network and no pull, and record whether its layers mount.
# Usage: layer_test.sh <image list> <output .tsv>. Needs no sudo, and never removes an image.
in=${1:?image list}
out=${2:?output file}
grep -vE '^(sha256:|moby-dangling@)' "$in" | while read -r img; do
  msg=$(timeout 120 docker run --rm --pull never --network none --entrypoint true "$img" 2>&1 < /dev/null)
  case $msg in
    "") s=ok ;;
    *"failed to stat parent"*|*"lowerdir="*|*"overlayfs/snapshots/"*) s=layers_missing ;;
    *"blob not found"*) s=blob_missing ;;
    *"already exists"*|*"missing parent"*) s=stale_record ;;
    *"executable file not found"*) s=ok ;;   # the layers mounted; the image just has no "true"
    *) s=other ;;
  esac
  printf '%s\t%s\t%s\n' "$s" "$img" "$(printf '%s' "$msg" | tr '\n\t' '  ' | cut -c1-200)"
done > "$out"
EOF
bash -n ~/snapshot-audit/long_scans.sh && bash -n ~/snapshot-audit/layer_test.sh && echo "scripts OK"
```

- **Expected:** `scripts OK`.

## Step 3: start the long jobs first (15 minutes)

The slow jobs start now, so they run while you do steps 4 to 6.

### 3.1 Copy the logs before the weekly rotation deletes the oldest week

- **Why:** the logs are the evidence. On Sunday 18 Oct, the weekly rotation deletes 13 to 20 Sep from `/var/log`. The journal is capped at 3.9 GB, so containerd's entries from 10 Oct will go too, eventually.
- **Risk:** none. It reads the logs, and writes about 4 GB into your home on the system disk, which has 165 GB free. `df` shows the space first.

```bash
df -h /home                                      # free space; needs more than 10 GB
cp -p /var/log/syslog.*.gz /var/log/auth.log.*.gz /var/log/kern.log.*.gz /var/log/auth.log.1 /var/log/kern.log.1 ~/snapshot-audit/logcopy/
nice journalctl -u containerd -u docker -u data.mount -o short-iso --no-pager | gzip > ~/snapshot-audit/logcopy/journal_containerd_docker.txt.gz
ls -lh ~/snapshot-audit/logcopy/
```

- **Expected:**
  - `df` shows about 165 GB available.
  - After 1 to 3 minutes, `ls` lists 12 files: nine `.gz` logs (about 3 GB together), `auth.log.1` (150 MB), `kern.log.1` (27 MB), and the journal export.
- **Send me:** the `df` line and the `ls` output.

### 3.2 Start the two slow log scans in the background

- **Why:** they read about 20 GB of syslog, which takes 10 to 20 minutes. `nohup … &` runs them in the background, and they keep running if the connection drops. `nice` gives other users' work priority.
- **Risk:** none. No sudo; they read logs and write three files in `~/snapshot-audit/logs/`.

```bash
cd ~/snapshot-audit/logs
nohup nice bash ~/snapshot-audit/long_scans.sh > long_scans.log 2>&1 &
```

- **Expected:** a job number and a process ID, for example `[1] 123456`. Check progress at any time with `cat ~/snapshot-audit/logs/long_scans.log`. You get the results in step 7.1.

### 3.3 List the images, and start the start test

- **Why:**
  - **The image list.** `ctr images ls -q` asks containerd for every image name. `docker image ls` can't be used, because it fails on this host (report, section 2.3).
  - **The start test.** It starts each image once, with no network and no pull, and sorts the failures by type of damage. That gives the full list of broken images, and it takes 2 to 4 hours.
- **Risk:**
  - **`ctr images ls`: none.** It is a list command from your rule; sudo asks for your password.
  - **The start test: low.** Each test container runs `true` and is removed at once (`--rm`), so it touches no other container, and it never removes an image. Like any `docker run`, it may unpack an image that wasn't unpacked yet. That adds files and changes nothing that exists.

```bash
cd ~/snapshot-audit/inventory
sudo ctr -n moby images ls -q > images_list.txt                       # every image name
wc -l < images_list.txt
nohup nice bash ~/snapshot-audit/layer_test.sh images_list.txt layer_test.tsv > layer_test.log 2>&1 &
```

- **Expected:** about `7089`, then a job number and a process ID. `wc -l < ~/snapshot-audit/inventory/layer_test.tsv` grows as it runs; it is finished when `pgrep -f layer_test.sh` prints nothing. You get the results in step 7.2.
- **Send me:** the image count.

## Step 4: the logs (30 minutes, no sudo)

### 4.1 The boots, and the order of `/data` and containerd (Q1, Q5)

- **Why:** this is the heart of the 12 Oct event. If containerd started before `/data` was mounted, it worked on the empty folder of the system disk. `systemd[1]` lines are systemd starting and stopping services and mounts; the filter keeps only those.
- **Risk:** none. It reads the journal.

```bash
cd ~/snapshot-audit/logs
journalctl --list-boots --no-pager > boots.txt                         # every boot the journal knows
for b in -1 0; do                                                       # -1 is the 11 Sep boot, 0 is the 12 Oct boot
  echo "===== boot $b: systemd starting and stopping data.mount, containerd and docker"
  journalctl -b "$b" -o short-iso --no-pager -u data.mount -u containerd.service -u docker.service | grep -m 12 -E 'systemd\[1\]: .*(Mount|Start|Stopp|Fail)'
  echo "-- last lines of this boot"
  journalctl -b "$b" -o short-iso --no-pager -n 5 | cut -c1-160
done >> boots.txt 2>&1
cat boots.txt
```

- **Expected**, after about 2 minutes:
  - Three boots: 24 Jul 15:49, 11 Sep 09:14 and 12 Oct 10:38.
  - **Boot 0:** `Starting containerd.service` at 10:38:15, `Failed with result 'timeout'` at 10:39:45 and `Started containerd.service` at 10:40:15. Then `Mounted data.mount` at 10:54:05. That confirms Q1.
  - **Boot -1:** few or no lines, because its older entries were rotated away. Its last lines are from 11 Oct 15:24, with no shutdown messages.
- **Send me:** the output of `cat boots.txt`.
- **Corrected by step 9.2c (15 Oct, about 17:52):** 11 Sep 09:14 was not a boot. It was the oldest journal line still kept when this step was first looked at. The login prompts give five boots: 24 Jul 15:15, 4 Aug 20:44, 27 Aug 20:25, 27 Aug 21:35 and 12 Oct 10:38.

### 4.2 Out of memory, stuck processes and disk messages (Q9)

- **Why:**
  - **Out-of-memory kills:** when memory runs out, the kernel kills processes ("out-of-memory killer"). The 12 Oct restart followed a day of that.
  - **Stuck processes:** "blocked for more than" and "waiting for writeback" mean a process waited minutes for the disk.
  - **Disk recovery:** "recovery complete" and "orphan" mean a disk wasn't unmounted cleanly.
  - **The question:** did this also happen around 6 and 7 Oct, when the first damage appeared?
- **Risk:** none. It reads the kernel logs, about 50 MB.

```bash
cd ~/snapshot-audit/logs
zgrep -hE 'Out of memory: Killed process|invoked oom-killer|blocked for more than|waiting for writeback|soft lockup|I/O error|EXT4-fs (error|warning)|EXT4-fs \(nvme|recovery complete|orphan' \
  /var/log/kern.log.4.gz /var/log/kern.log.3.gz /var/log/kern.log.2.gz /var/log/kern.log.1 /var/log/kern.log > kernel.txt
grep 'Out of memory: Killed' kernel.txt | cut -c1-10 | uniq -c > oom_per_day.txt                   # kills per day
grep -oE 'Killed process [0-9]+ \([^)]+\)' kernel.txt | awk '{print $4}' | sort | uniq -c | sort -rn > oom_victims.txt
grep -E 'blocked for more than|waiting for writeback' kernel.txt | cut -c1-160 > hung_tasks.txt     # processes stuck on the disk
grep -E 'systemd-journald.service: (Failed|State)' /var/log/syslog > journald_watchdog.txt           # these lines are only in syslog
cat oom_per_day.txt oom_victims.txt journald_watchdog.txt
cut -c1-13 hung_tasks.txt | sort | uniq -c                                                          # stuck processes per hour
grep -E 'recovery complete|orphan|I/O error|EXT4-fs error' kernel.txt | cut -c1-160
```

- **Expected:**
  - **Kills per day:** on 11 Oct (from 15:21) and on 12 Oct (until 10:36), 26 in all in `kern.log`. Another day, above all 6 or 7 Oct, is a lead.
  - **Victims:** `python` and `java`, not `containerd` or `dockerd`.
  - **Watchdog:** failures at 15:23 and 15:31 on 11 Oct.
  - **Stuck processes:** at least `2026-10-14T20` and `2026-10-14T21`, when `dockerd` waited more than 2 minutes for the disk.
  - **Disk messages:**
    - `orphan cleanup` on `nvme0n1p3` at 10:38 on 12 Oct;
    - `recovery complete` and `1386 orphan inodes deleted` on `nvme1n1p1` at 10:54;
    - no `I/O error`.
- **Send me:** the whole output.

### 4.3 Logins and sudo commands since 13 Sep (Q2, Q8, Q11)

- **Why:** every SSH login is in `auth.log`, with the fingerprint of the key used. Root's work therefore shows as root logins, and as sudo commands when root used sudo. This is how we see who was on the host when fstab was edited and `/data` mounted.
- **Risk:** none. It reads `auth.log*`.

```bash
cd ~/snapshot-audit/logs
a="/var/log/auth.log.4.gz /var/log/auth.log.3.gz /var/log/auth.log.2.gz /var/log/auth.log.1 /var/log/auth.log"
zgrep -hE 'sshd.*Accepted ' $a > logins.txt                                           # every SSH login, with the key's fingerprint
zgrep -hE 'sudo.*COMMAND=' $a > sudo_commands.txt                                     # every sudo command
grep ' for root ' logins.txt | awk '{print substr($1, 1, 10), $NF}' | sort | uniq -c > root_logins_by_day.txt
grep -hE '^2026-10-12T(09|10|11|12):' logins.txt sudo_commands.txt | grep -E ' for root |COMMAND=' | sort > around_1012.txt
wc -l logins.txt sudo_commands.txt
cat root_logins_by_day.txt
echo "--- root logins and sudo commands, 12 Oct 09:00-12:59"; cat around_1012.txt
echo "--- all sudo commands"; cut -c1-200 sudo_commands.txt
echo "--- who logged in on 12 Oct 10:00-12:59"; grep -E '^2026-10-12T1[0-2]:' logins.txt | grep -oE 'for [a-z0-9_-]+' | sort | uniq -c
```

- **Expected:**
  - **Root logins per day:** the count, date and key fingerprint (`SHA256:…`) of each.
  - **12 Oct:** root logins between 10:38 and 12:10, when `/data` was mounted and fstab rewritten.
  - **Sudo commands:** a few lines, all by root, for example `systemctl start taskanchor-…` at 11:19 on 12 Oct, and the group change for `toby` at 10:17 on 15 Oct.
  - **Users:** the accounts that logged in on 12 Oct.
- **Send me:** the whole output. It contains IP addresses and user names; that's fine for this chat.

### 4.4 12 Oct in detail (Q4)

- **Why:** every start, stop and mount of containerd, Docker and `/data` on 12 Oct. The stop and start between 11:41 and 14:41 is when containerd switched back to the real database.
- **Risk:** none. It reads the journal.

```bash
cd ~/snapshot-audit/logs
journalctl --since '2026-10-12 10:30' --until '2026-10-12 15:00' -o short-iso --no-pager -u containerd -u docker -u data.mount > day_1012.txt
grep -E 'systemd\[1\]: .*(Start|Stopp|Mount|Fail)' day_1012.txt | cut -c1-160
```

- **Expected:**
  - containerd starting at 10:38 and 10:40, and `/data` mounted at 10:54;
  - Docker stopping at 11:48 and starting at 13:07;
  - a containerd stop and start between 11:41 and 14:41. Compare its time with the root logins in 4.3.
- **Send me:** the output.

### 4.5 containerd's restarts on 14 Oct (Q11)

- **Why:** containerd restarted at 11:11 and 17:36 on 14 Oct. A restart doesn't delete data, but we want to know who did it and why.
- **Risk:** none. It reads the journal and the files from 4.3.

```bash
cd ~/snapshot-audit/logs
journalctl -u containerd --since 2026-10-14 --until 2026-10-15 -o short-iso --no-pager | grep -E 'systemd\[1\]: ' > restarts_1014.txt
grep -hE '2026-10-14T(10:5|11:[01]|17:[23])' logins.txt sudo_commands.txt | grep -E ' for root |COMMAND=' >> restarts_1014.txt
cut -c1-160 restarts_1014.txt
```

- **Expected:** `Stopping`, `Stopped`, `Starting` and `Started` lines at about 11:11 and 17:36, and any root login or sudo command just before them.
- **Send me:** the output.

### 4.6 Your own scheduled jobs (Q8)

- **Why:** rule out your own account first. Four accounts keep background services running without being logged in: naiyu, toby, yun and zihan. Step 6.3 lists the other three.
- **Risk:** none. It only lists.

```bash
systemctl --user list-timers --all --no-pager
crontab -l
```

- **Expected:** no timer that deletes files, and only the hourly canary line in the crontab.
- **Send me:** the output.

## Step 5: containerd's records against the disk (20 minutes, sudo)

### 5.1 Save containerd's lists, and the files on disk

- **Why:** compare containerd's records with what is really on disk. `images check` tells, for each image, whether all its downloaded files are present (`complete`) and whether it's unpacked. The lists of layer folders and downloaded files are what's on disk.
- **Risk:** none. Every sudo command here is a list command from your rule. `images check` reads the records of all 7,089 images; it takes up to 10 minutes and changes nothing.

```bash
cd ~/snapshot-audit/inventory
d=/data/containerd
sudo ctr -n moby images check > images_check.txt 2>&1                 # each image: complete or incomplete, unpacked or not
sudo ctr -n moby snapshots ls > snapshots_db.txt                      # every layer record
sudo ctr -n moby content ls > content_records.txt                     # every downloaded-file record, with its age
sudo ctr -n moby content active > content_active.txt                  # downloads in progress, or cut off by the restart
sudo ls -l --full-time $d/io.containerd.snapshotter.v1.overlayfs/snapshots > snapshot_dirs.txt    # layer folders on disk
sudo ls -U $d/io.containerd.content.v1.content/blobs/sha256 > blob_files.txt                     # downloaded files on disk
wc -l images_check.txt snapshots_db.txt content_records.txt content_active.txt snapshot_dirs.txt blob_files.txt
awk 'NR > 1 {print $4, $NF}' images_check.txt | sort | uniq -c        # images per state
cat content_active.txt
```

- **Expected:**
  - `wc -l` gives many thousands of lines for each list, and few for `content_active.txt`.
  - The states are counts such as `5000 complete true` and `900 incomplete true`. "Incomplete" means downloaded files are missing.
  - `content_active.txt` is a header, and maybe a few downloads cut off on 11 or 12 Oct.
  - If a command says `not allowed` or `I'm sorry`, stop and send it to me: it doesn't match the rule.
- **Send me:** the whole output.

### 5.2 Records against disk, and the ages of the missing files (Q7)

- **Why:** this is the test that tells the remaining candidates apart (report, section 7.2).
  - **Deleted by age:** the missing files are all old, and the present ones younger.
  - **Lost in the restart:** the missing files are from 11 and 12 Oct.
  - **Neither:** no pattern.
  
  `comm` compares two sorted lists: `-23` keeps lines only in the first (recorded but not on disk), and `-13` lines only in the second (on disk but not recorded).
- **Risk:** none. No sudo; it reads the files from 5.1 and writes text files.

```bash
cd ~/snapshot-audit/inventory
echo "layer records $(($(wc -l < snapshots_db.txt) - 1)), layer folders $(grep -c '^d' snapshot_dirs.txt)"
awk 'NR > 1 {sub(/^sha256:/, "", $1); print $1}' content_records.txt | LC_ALL=C sort > blobs_db.txt
LC_ALL=C sort blob_files.txt > blob_files.sorted
LC_ALL=C comm -23 blobs_db.txt blob_files.sorted > blobs_missing.txt          # recorded, but not on disk
LC_ALL=C comm -13 blobs_db.txt blob_files.sorted > blobs_unrecorded.txt       # on disk, but not recorded
wc -l blobs_db.txt blobs_missing.txt blobs_unrecorded.txt
awk 'NR == FNR {miss["sha256:" $1]; next} FNR > 1 {print (($1 in miss) ? "missing" : "present") "\t" $3 " " $4}' \
  blobs_missing.txt content_records.txt | sort | uniq -c > blob_ages.txt
cat blob_ages.txt
```

- **Expected:**
  - More layer records than layer folders.
  - Some thousands of missing files, and few or no unrecorded ones.
  - `blob_ages.txt`: counts per state and age, for example `812 missing 2 months` and `40210 present 3 weeks`.
- **Send me:** the whole output.

### 5.3 Folders from a second numbering (Q5)

- **Why:** the empty database of 12 Oct numbered its folders from 1, while the real one is past 350,000. A low-numbered folder with another date would mean another split brain. Step 9.2c later showed there was no boot on 11 Sep.
- **Risk:** none. No sudo; it reads `snapshot_dirs.txt`.

```bash
cd ~/snapshot-audit/inventory
awk '$9 ~ /^[0-9]+$/ && $9 < 1000 {print $9, $6, $7, $8}' snapshot_dirs.txt | sort -n > low_numbered_dirs.txt
wc -l < low_numbered_dirs.txt
awk '{print $2}' low_numbered_dirs.txt | sort | uniq -c                       # low-numbered folders per date
head -n 5 low_numbered_dirs.txt
```

- **Expected:** only the date `2026-10-12`, with times between 10:54 and 14:41, including folders 37 and 54. Any other date answers Q5 with "yes".
- **Send me:** the output.
- **Result on 15 Oct, 13:37:** the count was `0`, and `head` printed nothing. That is not the expected list, so 5.3b checks whether field 9 is really the folder name.

### 5.3b Check the columns of `snapshot_dirs.txt` (Q5)

- **Why:** step 5.3 printed 0. That is either true (no folder numbered under 1000) or a wrong column (field 9 is not the folder name, so every line was skipped). This prints one line field by field, counts names under 1000 using the last field, and lists the ten lowest folder numbers.
- **Risk:** none. No sudo; it reads `snapshot_dirs.txt`.

```bash
cd ~/snapshot-audit/inventory
awk 'NR==2 {print NF; for (i=1;i<=NF;i++) print i, $i}' snapshot_dirs.txt
echo "---"
awk 'NR>1 && $NF ~ /^[0-9]+$/ && $NF < 1000 {c++} END {print "under 1000:", c+0}' snapshot_dirs.txt
awk 'NR>1 && $NF ~ /^[0-9]+$/ {print $NF}' snapshot_dirs.txt | sort -n | head
```

- **Expected:** 9 fields, with field 9 a folder number. `under 1000: 0`. The ten lowest numbers start at 152997.
- **Send me:** the output.
- **Result on 15 Oct, 13:39:** field 9 is the folder name (`152997`, dated `2026-09-10`). `under 1000: 0`. The lowest folders are 152997, 153000, 153120, 153125, 153126, 153127, 153129, 153130, 153132, 153133. The empty list from 5.3 is real. Folders 37 and 54 are gone, so Q5 cannot be answered from folder numbers.

## Step 6: what only root can read (20 minutes, sudo)

### 6.1 Whose keys may log in as root (Q2)

- **Why:** match the fingerprints of the root logins in 4.3 to the people the keys belong to. Each key's comment is usually its owner's name or computer.
- **Risk:** none. It prints fingerprints and comments, not the keys.

```bash
cd ~/snapshot-audit/logs
sudo ssh-keygen -lf /root/.ssh/authorized_keys > root_keys.txt
cat root_keys.txt
```

- **Expected:** one line per key, for example `2048 SHA256:… comment (RSA)`. Your fingerprint from 0.2 should be among them, and so should each fingerprint in `root_logins_by_day.txt`.
- **Send me:** the output. You may replace e-mail addresses in the comments with names.
- **Result on 15 Oct, 13:54:** two keys. `SHA256:Sw9bNRuC…` comment `zihanwu@MacBook-Pro ali-server-root` (ED25519). `SHA256:QHhVAmPV…` comment `skp-t4n2qiu4tscnr8ui08sf` (RSA), the same fingerprint as `root.pem`.

### 6.2 Root's shell history (Q2, Q8)

- **Why:** find any command of root's that edited fstab, mounted or unmounted, or deleted under `/data`. Only matching lines are kept. `-B1` also keeps the line before each match; if bash saved timestamps, that line is `#` followed by seconds since 1970.
- **Risk:** none. `sudo cat` only reads the file; the filtering runs as `toby`.

```bash
cd ~/snapshot-audit/logs
sudo cat /root/.bash_history | grep -nE -B1 'fstab|mount|/data|containerd|docker|rm -|find |prune' > root_history.txt
wc -l < root_history.txt
cat root_history.txt
```

- **Expected:** the commands that wrote and fixed fstab and mounted `/data`, and any deletion under `/data`. To turn a timestamp line such as `#1791979238` into a date, run `date -d @1791979238`. Ubuntu's default keeps only the last 2,000 commands, so a missing line isn't proof.
- **Send me:** the output. Read it first, and replace any password or token with `<removed>`.
- **Result on 15 Oct, 14:01:** 247 lines. Toby checked the paste: no password or token. No `#` timestamp lines. History lines 303 and 469 are `mkdir -p /data && cp /etc/fstab /etc/fstab.bak && mount /dev/nvme1n1p1 /data && echo \"/dev/nvme1n1p1 /data ext4 defaults 0 0\" >> /etc/fstab && df -hT /data`. The `\"` writes the quotes into the file, which is the invalid fstab line. The 12:09 UUID rewrite is not in the file. No `rm`, `prune` or `find -delete` of the store. The only `rm -rf` is `/home/zihan/lhs`. Root also ran xu's scripts from `/home/xu/check_docker/scripts/`, several with `--apply`.

### 6.3 Every crontab, and the other users' timers (Q8)

- **Why:** find any scheduled job, of any user, that deletes files or prunes Docker. Crontabs are in `/var/spool/cron/crontabs/<user>`. User timers are in `~/.config/systemd/user/` of the three users who keep services running.
- **Risk:** none. It reads crontabs, and lists only the file names of the timers.

```bash
cd ~/snapshot-audit/logs
sudo grep -rH . /var/spool/cron/crontabs | grep -vE ':[[:space:]]*#' > crontabs.txt
sudo ls -laR /home/naiyu/.config/systemd/user /home/yun/.config/systemd/user /home/zihan/.config/systemd/user > user_timers.txt 2>&1
echo "--- crontab lines that delete, prune or touch Docker or /data"; grep -iE 'rm |find |prune|delete|docker|ctr|/data' crontabs.txt
echo "--- user units"; grep -E '\.(timer|service)' user_timers.txt
```

- **Expected:**
  - Crontab lines as `/var/spool/cron/crontabs/<user>:<job>`, including your canary.
  - Unit file names of naiyu, yun and zihan; zihan's include `contextgraph-release-…` services, which use `/data`.
  - A timer named like clean, prune or gc is a lead; its owner should say what it does.
- **Send me:** the output.
- **Result on 15 Oct, 14:12:** two matching crontab lines. zihan runs `capbench_linux_sync.py` from `/data` every 6 hours. Toby's hourly canary is the other line. No timer is named clean, prune or gc. Timers found: `cg-verified-review.timer`, `jiuwen-tb4-experience-20261006.timer`, `jiuwen-tb4-experiment-experience-pilot6-r3.timer`. yun has no unit file in the filtered list.

### 6.4 The empty store hidden under `/data` (Q3)

- **Why:** when `/data` is mounted, the folder of the same name on the system disk is hidden underneath. containerd's empty database from 12 Oct 10:38 should still be there. The only way to look under a mount point is a second, read-only view of the system disk at `/mnt/rootfs`.
- **Risk:** low.
  - The view is read-only (`ro`), so nothing can be written through it, not even by root.
  - It covers the system disk only, not `/data`, and it changes no file.
  - The last commands remove it.
  - `findmnt` checks that the view really is read-only. **If it shows `rw`, stop:** run only `sudo umount /mnt/rootfs`, and send me the output.

```bash
cd ~/snapshot-audit/logs
sudo mkdir -p /mnt/rootfs                                  # an empty folder for the view
sudo mount -o bind,ro / /mnt/rootfs                        # the read-only view of the system disk
findmnt -no TARGET,OPTIONS /mnt/rootfs                     # must show "/mnt/rootfs ro,..."
sudo ls -la --full-time /mnt/rootfs/data /mnt/rootfs/data/containerd /mnt/rootfs/data/containerd/io.containerd.metadata.v1.bolt > hidden_store.txt 2>&1
sudo du -sh /mnt/rootfs/data >> hidden_store.txt 2>&1      # how much is hidden
sudo umount /mnt/rootfs                                    # remove the view
findmnt /mnt/rootfs || echo "view removed"
cat hidden_store.txt
```

- **Expected:**
  - The first `findmnt` shows `/mnt/rootfs ro,…`.
  - `hidden_store.txt` shows a `containerd` folder from 2026-10-12, around 10:38, and a `meta.db` last changed between 10:38 and 14:41. Together they prove the split brain.
  - The last `findmnt` prints `view removed`.
  
  An empty `/mnt/rootfs/data` would mean someone deleted the hidden store later; we'd ask root.
- **Send me:** the whole output.
- **Result on 15 Oct, 14:15:** `/mnt/rootfs ro,relatime`, then `view removed`. Hidden `/data` is 1.7 GB. `containerd` is dated 4 Aug 2026, not 12 Oct. `meta.db` is 906,985,472 bytes, last written 12 Oct 11:48:43. `docker` was created 12 Oct 10:39:46 and `hanxu` at 10:38:15. This is the split brain: containerd's open database was the August one on the system disk.

### 6.5 The two containers from the split brain (Q12)

- **Why:** two containers were created on 12 Oct at 11:33 and 11:41, while containerd used the empty database. If the real database doesn't know them, they will fail at their next start, and their owners should be told.
- **Risk:** none. `docker inspect` and the `sudo ls` only read, and the database check reads the file from 5.1.

```bash
cd ~/snapshot-audit/logs
for c in f870b22117e6 6afbbec81e30; do
  id=$(docker inspect --format '{{.Id}}' "$c")                                       # full container ID
  echo "== $c $(docker inspect --format '{{.Name}} {{.Config.Image}} {{.State.Status}}' "$c")"
  echo "records for it in the real database: $(grep -c "$id" ../inventory/snapshots_db.txt)"
done > split_brain_containers.txt
sudo ls -ld --full-time /data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots/37 /data/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots/54 >> split_brain_containers.txt 2>&1
cat split_brain_containers.txt
```

- **Expected:** for each container, its name, image and state (`running` or `exited`), and its number of records.
  - **2:** its layer and init records exist, so it's known.
  - **0:** it isn't known.
  
  Then both folders, dated 12 Oct.
- **Send me:** the output.
- **Result on 15 Oct, 14:19:** both containers are `exited` and have 0 records in `snapshots_db.txt`. `f870b22117e6` is `/clarify-selection4-runtime-v3-20261012` on `docker:29-dind`. `6afbbec81e30` is the Harbor egress sidecar for `bandit-incremental-cache-control`. Folders 37 and 54 do not exist, which matches step 5.3b.

## Step 7: results of the long jobs

### 7.1 The slow log scans (Q6, Q10)

- **When:** `cat ~/snapshot-audit/logs/long_scans.log` shows `Q10 done`.
- **Why:**
  - `damage_first_seen.txt` dates the first appearance of each kind of error.
  - `syslog_volume.txt` shows which days and programs wrote the 15 GB of syslog.
- **Risk:** none.

```bash
cd ~/snapshot-audit/logs
cat long_scans.log damage_first_seen.txt
grep '^day' syslog_volume.txt | sort                                  # syslog size per day, 4 to 11 Oct
grep '^prog' syslog_volume.txt | sort -k2,2nr | head -n 15             # the programs that wrote the most lines
```

- **Expected:**
  - For each error type, the first containerd or Docker line with it, or `never`. Errors that a client sees aren't always in the daemon's log, so `never` doesn't prove anything.
  - One line per day with its size in MB, then the top 15 programs.
- **Send me:** the whole output.
- **Result on 15 Oct, 14:23:** scans finished at 12:44:43 and 12:45:28. awk warned about one non-text line in `syslog.1` and continued. First lines: `blob not found` 13 Sep 01:37, `missing parent` 14 Sep 12:05, `no space left` 18 Sep 17:27, `failed to stat parent` 14 Oct 17:01, `already exists` never. Days: 4 Oct 34 MB, 5 Oct 41 MB, 6 Oct 506 MB, 7 Oct 2514 MB, 8 Oct 3580 MB, 9 Oct 4568 MB, 10 Oct 2945 MB, 11 Oct 55 MB. `dockerd` wrote 22,284,226 lines.

### 7.2 The start test (impact)

- **When:** `pgrep -f layer_test.sh` prints nothing.
- **Result on 15 Oct, 14:27:** `pgrep` printed nothing, and the shell printed `start test finished`.
- **Why:** count the broken images per type of damage, see whose they are (grouped by repository), and list the containers that use a broken image.
- **Risk:** none. `docker ps` only lists.

```bash
cd ~/snapshot-audit/inventory
cut -f1 layer_test.tsv | sort | uniq -c                                              # images per result type
awk -F'\t' '$1 != "ok" {print $1 "\t" $2}' layer_test.tsv | sed -E 's|[:@][^/]*$||' | sort | uniq -c | sort -rn > broken_by_repo.txt
awk -F'\t' '$1 != "ok" {print $2}' layer_test.tsv | while read -r i; do
  docker ps -a --filter ancestor="$i" --format "{{.ID}}\t{{.Names}}\t{{.Status}}\t$i"
done > containers_on_broken.tsv
head -n 20 broken_by_repo.txt
wc -l < containers_on_broken.tsv
```

- **Expected:** counts of `ok`, `layers_missing`, `blob_missing`, `stale_record` and `other`. The DeepSWE repository should be near the top of the list, with 113.
- **Send me:** the whole output.
- **Result on 15 Oct, 14:30:** 254 ok, 6,313 blob_missing, 253 layers_missing, 27 stale_record, no `other`. Sum 6,847. All 113 `public.ecr.aws/d3j8x8q7/swe-bench-202605` images are layers_missing. 158 containers were created from a broken image.

| Type | Meaning |
|---|---|
| `ok` | The image starts |
| `layers_missing` | A layer folder is gone, as with DeepSWE |
| `blob_missing` | A downloaded file is gone |
| `stale_record` | `already exists` or `missing parent` |
| `other` | Any other error, for example an image for another CPU type |

## Step 8: finish

### 8.1 Stop recording

- **Risk:** none. If bash says `There are running jobs`, type `exit` again; the `nohup` jobs keep running.

```bash
exit
```

- **Expected:** the normal prompt again. The recording is in `~/snapshot-audit/sessions/`.
- **Result on 15 Oct, 14:34:** `eixt` printed `command not found`. The next `exit` left the recording. The prompt came back as `toby@iZt4ndd2dff7gqjta7mppaZ:~/snapshot-audit/logs$`.

### 8.2 Copy the evidence to your PC (optional)

- **Where:** terminal 2, your PC.
- **Why:** a copy for the meeting with the mentor. It goes outside the mac-k3d repository, because it contains user names, IP addresses and root's commands.
- **Risk:** none. It reads from the worker and writes on your PC.

```bash
mkdir -p ~/cloud-worker-evidence
scp -r toby@47.84.16.18:snapshot-audit/logs ~/cloud-worker-evidence/
scp -r toby@47.84.16.18:snapshot-audit/inventory ~/cloud-worker-evidence/
du -sh ~/cloud-worker-evidence/*
```

- **Expected:** two folders, `logs` and `inventory`, a few hundred MB together.
- **Result on 15 Oct, 14:40:** both folders arrived. `logs` is 1.4 GB, almost all of it `damage_errors.txt` at 1,286 MB. `inventory` is 50 MB. `logcopy/` and `sessions/` were not part of this command. They were copied on 16 Oct in step 9.7.

### 8.3 When the investigation is over: root removes the access

Root runs this, or tells you to run it with `root.pem` in the same way as step 1:

```bash
rm -f /etc/sudoers.d/90-toby-investigate                   # removes the rule
systemctl stop toby-investigate-expire.timer 2>/dev/null   # stops the backstop timer
passwd -l toby                                             # locks toby's password again
sudo -l -U toby                                            # shows that toby may not run sudo
```

- **Risk:** low. Each line only takes back what step 1 gave.
- **Expected:** the last line says that `toby` may not run sudo.

## Step 9: follow-up checks (read-only)

Steps 0 to 8 answered Q1 to Q12. Step 9 answered Q13 to Q17. The worker commands are finished.

- **9.1 to 9.1d, 9.2 to 9.2c, 9.5, 9.7 and 9.8** are done.
- **9.3 and 9.4** were run on 16 Oct with `root.pem`, read-only. The view was removed. Tell the mentor the same day that `root.pem` was used.
- **9.6** is the message to root and xu. No answer is recorded. The repair does not wait for one.
- Nothing in step 9 changes the store.

### 9.1 Sort the PC copy by age and by source (Q13, Q14)

- **Where:** terminal 2, your PC. No worker, no sudo.
- **Why:**
  - **Files by age and source:** the copy from 8.2 holds every file record with its age and labels. A file pulled from a registry carries a `containerd.io/distribution.source…` label; a file made on this host (built or loaded) has none. Counting missing and present files by age dates the loss from the files themselves.
  - **First errors:** the error log names some missing files. The first time each one is named shows whether it was gone before 12 Oct.
  - **Folders by date:** the dates of the layer folders, and of the folders that the start test reports missing.
- **Risk:** none. It reads the copy, and writes five small files to `~/cloud-worker-evidence/analysis/`.

```bash
mkdir -p ~/cloud-worker-evidence/analysis && cd ~/cloud-worker-evidence && python3 - <<'EOF' | tee analysis/summary.txt
import bisect, collections, re

LOGS, INV, OUT = 'logs/', 'inventory/', 'analysis/'
MOUNT = '2026-10-12T10:54'                     # /data mounted under the running containerd

def hours(age):                                # "4 weeks" -> 672; ctr prints ages like docker ps
    n, unit = (age.split() + ['', ''])[:2]
    if age.startswith('About'):
        return 1
    for k, v in (('second', 0), ('minute', 0), ('hour', 1), ('day', 24), ('week', 168), ('month', 720)):
        if unit.startswith(k):
            return int(n) * v
    return 10**9

present = {l.strip() for l in open(INV + 'blob_files.txt') if l.strip()}
missing = {l.strip() for l in open(INV + 'blobs_missing.txt') if l.strip()}
pulled = re.compile(r'containerd\.io/distribution\.source\.')   # set on files pulled from a registry
rec = {}
with open(INV + 'content_records.txt') as f:
    next(f)
    for line in f:
        p = line.split()
        if not p:
            continue
        age = []
        for w in p[2:]:
            if '=' in w or w == '-':
                break
            age.append(w)
        rec[p[0].split(':')[-1]] = (' '.join(age), 'pulled' if pulled.search(line) else 'made here')

# 1. Every file record by age: missing or present, pulled from a registry or made on this host
t = collections.Counter((a, 'present' if d in present else 'missing', h) for d, (a, h) in rec.items())
with open(OUT + 'files_by_age.txt', 'w') as o:
    o.write('age\tmissing pulled\tmissing made here\tpresent pulled\tpresent made here\n')
    for a in sorted({a for a, _, _ in t}, key=hours):
        o.write('\t'.join([a] + [str(t[(a, s, h)]) for s in ('missing', 'present') for h in ('pulled', 'made here')]) + '\n')
def count(cond, state, how=None):
    return sum(v for (a, s, h), v in t.items() if cond(hours(a)) and s == state and how in (None, h))
for label, cond in (('recorded after the 12 Oct restart (under 72 hours old)', lambda x: x < 72),
                    ('recorded 2 Oct to 12 Oct, before the restart (3 to 13 days old)', lambda x: 72 <= x < 336),
                    ('recorded before 2 Oct (2 weeks or older)', lambda x: x >= 336)):
    print(f'{label}: {count(cond, "missing")} missing, {count(cond, "present")} present')
for how in ('made here', 'pulled'):
    print(f'recorded before the restart, {how}: {count(lambda x: x >= 72, "missing", how)} missing, {count(lambda x: x >= 72, "present", how)} present')

# 2. Error lines per day, and the first time the logs name each missing file
kinds = ('blob not found', 'failed to stat parent', 'missing parent', 'already exists', 'no space left')
digest = re.compile(r'blobs/sha256/([0-9a-f]{64})')
per_day = collections.defaultdict(collections.Counter)
first = {}
with open(LOGS + 'damage_errors.txt', 'rb') as f:
    for raw in f:
        line = raw.decode('utf-8', 'replace')
        per_day[line[:10]][next((k for k in kinds if k in line), 'other')] += 1
        for d in digest.findall(line):
            if d not in first or line[:16] < first[d]:
                first[d] = line[:16]
with open(OUT + 'errors_per_day.txt', 'w') as o:
    o.write('day\t' + '\t'.join(kinds) + '\tother\n')
    for day in sorted(per_day):
        o.write(day + '\t' + '\t'.join(str(per_day[day][k]) for k in kinds + ('other',)) + '\n')
named = {d: ts for d, ts in first.items() if d in missing}
print(f'missing files named in the error log: {len(named)} of {len(missing)}')
with open(OUT + 'missing_first_reported.txt', 'w') as o:
    o.write('day first reported\tmissing files\n')
    for day, n in sorted(collections.Counter(ts[:10] for ts in named.values()).items()):
        o.write(f'{day}\t{n}\n')
    for label, group in (('before', [d for d, ts in named.items() if ts < MOUNT]),
                         ('on or after', [d for d, ts in named.items() if ts >= MOUNT])):
        c = collections.Counter(rec.get(d, ('no record', '')) for d in group)
        line = f'first reported {label} 12 Oct 10:54: {len(group)} files. By age and source: ' + ', '.join(f'{a} {h} {n}' for (a, h), n in c.most_common())
        o.write(line + '\n')
        print(line)

# 3. Layer folders on disk by date, and the missing folders that the start test names
dirs = []
with open(INV + 'snapshot_dirs.txt') as f:
    next(f)
    for line in f:
        p = line.split()
        if len(p) >= 9 and p[8].isdigit():
            dirs.append((int(p[8]), p[5], p[6][:5]))
dirs.sort()
nums = [n for n, _, _ in dirs]
before = sum(1 for _, d, tm in dirs if (d, tm) < ('2026-10-12', '10:38'))
errors = ''.join(l for l in open(INV + 'layer_test.tsv') if l.startswith('layers_missing'))
named_dirs = sorted({int(n) for n in re.findall(r'snapshots/(\d+)/fs', errors)})
gone = [n for n in named_dirs if n not in set(nums)]
with open(OUT + 'folders_by_date.txt', 'w') as o:
    o.write('day\tlayer folders on disk with that date\n')
    for day, n in sorted(collections.Counter(d for _, d, _ in dirs).items()):
        o.write(f'{day}\t{n}\n')
    o.write(f'lowest folder on disk: {dirs[0][0]} ({dirs[0][1]} {dirs[0][2]})\n')
    for n in gone:
        i = bisect.bisect_left(nums, n)
        o.write(f'missing folder {n}: next folder on disk is {dirs[i][0]} ({dirs[i][1]} {dirs[i][2]})\n')
print(f'layer folders on disk: {len(dirs)}; dated before the 12 Oct boot: {before}; lowest: {dirs[0][0]} ({dirs[0][1]} {dirs[0][2]})')
print(f'folders named in layers_missing errors: {len(named_dirs)}; not on disk: {len(gone)}, numbered {gone[0]} to {gone[-1]}')
EOF
ls -la analysis/
```

- **Expected:** ten lines of summary, and five files in `analysis/`: `summary.txt`, `files_by_age.txt`, `errors_per_day.txt`, `missing_first_reported.txt`, `folders_by_date.txt`.
- **Result on 15 Oct, 15:28:**
  - **Files by age:**
    - recorded after the 12 Oct restart: 0 missing, 634 present;
    - recorded 2 to 12 Oct, before the restart: 5,791 missing, 4 present;
    - recorded before 2 Oct: 37,124 missing, 2,626 present.
  - **Files by source, recorded before the restart:** made here 41,170 missing, 80 present; pulled 1,745 missing, 2,550 present.
  - **No file record is 6, 7 or 8 weeks old** (`files_by_age.txt`): none was created, or none is kept, from about 16 Aug to 3 Sep.
  - **First errors:** the error log names 930 of the 42,915 missing files.
    - 586 were first named before 12 Oct 10:54, and all 586 are 2 months old (463 made here, 123 pulled).
    - The 344 named later are from September and October.
  - **Folders:** 10,164 on disk, 7,826 dated before the 12 Oct boot. The lowest is 152997, dated 10 Sep 09:22. The start test names 57 missing folders, numbered 153149 to 159017.

### 9.1b Did the cleanup stop partway through? (Q13)

- **Where:** your PC.
- **Why:** containerd's cleanup of downloaded files goes through them in alphabetical order of digest. If it was stopped partway, the files that survived would all sit beyond some point in that order. If it finished, the survivors are spread evenly, and they are the files that its open database listed.
- **Risk:** none. It writes `analysis/hex_spread.txt`.

```bash
cd ~/cloud-worker-evidence && python3 - <<'EOF' | tee analysis/hex_spread.txt
import collections, re

def hours(age):                                # the same reading of ctr's ages as in step 9.1
    n, unit = (age.split() + ['', ''])[:2]
    if age.startswith('About'):
        return 1
    for k, v in (('second', 0), ('minute', 0), ('hour', 1), ('day', 24), ('week', 168), ('month', 720)):
        if unit.startswith(k):
            return int(n) * v
    return 10**9

present = {l.strip() for l in open('inventory/blob_files.txt') if l.strip()}
pulled = re.compile(r'containerd\.io/distribution\.source\.')
t = collections.Counter()                      # (first hex digit, pulled or made here, present or missing), records made before the restart
with open('inventory/content_records.txt') as f:
    next(f)
    for line in f:
        p = line.split()
        if not p:
            continue
        age = []
        for w in p[2:]:
            if '=' in w or w == '-':
                break
            age.append(w)
        if hours(' '.join(age)) >= 72:
            d = p[0].split(':')[-1]
            t[(d[0], 'pulled' if pulled.search(line) else 'made here', d in present)] += 1
print('Files recorded before the 12 Oct restart, by the first hex digit of the digest (the order of containerd\'s cleanup)')
print('digit\tpulled: missing\tpresent\tmade here: missing\tpresent\tall: present %')
for k in '0123456789abcdef':
    pm, pp, hm, hp = (t[(k, h, s)] for h in ('pulled', 'made here') for s in (False, True))
    print(f'{k}\t{pm}\t{pp}\t{hm}\t{hp}\t{100 * (pp + hp) / max(1, pm + pp + hm + hp):.1f}')
EOF
```

- **Expected:** 16 rows, one per first digit. Either a clear point in the order where present files stop, or the same share present in every row.
- **Result on 15 Oct, 15:43:** no cut-off. In every row, 5.3 to 6.5 % of the files are present, and pulled and made-here files are both spread evenly. So if the cleanup deleted the files, it finished its pass, and kept the files that its open database listed.

### 9.1c The split-brain hour in the logs (Q13, Q15, Q16)

- **Where:** your PC.
- **Why:**
  - **What happened in the split brain:** `logs/day_1012.txt` holds every containerd and Docker line of 12 Oct. From 10:40:15 to 11:48:51 containerd had the hidden database open, so everything Docker did then went through it.
  - **A first look into the hidden database:** Docker logs a warning for each image it fails to check, with the image's digest. Those images were listed in the hidden database, which gives a sample of its contents to compare with the real database before root's check (9.3).
  - **Other split brains:** `logs/kernel.txt` holds every mount of the 14 TB disk since 13 Sep.
- **Risk:** none. It writes `analysis/split_brain_window.txt`.

```bash
cd ~/cloud-worker-evidence && python3 - <<'EOF' | tee analysis/split_brain_window.txt
import collections, re

present = {l.strip() for l in open('inventory/blob_files.txt') if l.strip()}
missing = {l.strip() for l in open('inventory/blobs_missing.txt') if l.strip()}
rec = {}
with open('inventory/content_records.txt') as f:
    next(f)
    for line in f:
        p = line.split()
        if p:
            age = []
            for w in p[2:]:
                if '=' in w or w == '-':
                    break
                age.append(w)
            src = ' + '.join(f'{r}/{n}' for r, n in re.findall(r'containerd\.io/distribution\.source\.([^=]+)=([^,\s]+)', line))
            rec[p[0].split(':')[-1]] = (' '.join(age), re.sub(r'deepswe-py-20/[^ ]+', 'deepswe-py-20/<task>', src) or 'made here')

# 1. What containerd and Docker reported while containerd had the hidden database open (12 Oct, 10:40:15 to 11:48:51)
keys = ('snapshot garbage collection failed', 'deleting build record', 'image pulled', 'extraction snapshot removal failed')
warn = re.compile(r'checking availability of platform specific manifest failed" digest="sha256:([0-9a-f]{64})')
seen, per10 = {}, collections.Counter()
print('== containerd and Docker, 12 Oct 10:40:15 to 11:48:51')
with open('logs/day_1012.txt', errors='replace') as f:
    for line in f:
        t = line[11:19]
        if not '10:40:15' <= t <= '11:48:51':
            continue
        m = warn.search(line)
        if m:
            seen.setdefault(m.group(1), t)
            per10[t[:4] + '0'] += 1
        elif any(k in line for k in keys):
            print(t, re.sub(r'^.*? level=', 'level=', line.strip())[:220])
print('failed image checks per 10 minutes: ' + ', '.join(f'{k} {v}' for k, v in sorted(per10.items())))

# 2. The images Docker listed from the hidden database, against the real database today
state = collections.Counter('file present' if d in present else 'recorded, file missing' if d in missing else 'not in the real database' for d in seen)
print(f'\n== images listed from the hidden database: {len(seen)}. Their digests today: ' + ', '.join(f'{k} {v}' for k, v in state.most_common()))
for ((age, src), st), n in collections.Counter((rec[d], 'present' if d in present else 'missing') for d in seen if d in rec).most_common():
    print(f'{n}\t{st}\t{age}\t{src}')

# 3. The 14 TB disk in the kernel log (13 Sep to 15 Oct): mounts, resizes, recovery
print('\n== nvme1n1p1 (/data) in the kernel log')
for line in open('logs/kernel.txt', errors='replace'):
    if 'nvme1n1p1' in line and re.search(r'mounted|resized filesystem|recovery|orphan', line):
        print(line[:19], line.split('kernel: ', 1)[-1].strip()[:150])
EOF
```

- **Expected:** the containerd and Docker events of the hour, the images listed from the hidden database compared with the real one, and the mounts of the 14 TB disk.
- **Result on 15 Oct, 15:52:**
  - **Cleanup in the split brain:** containerd's layer cleanup ran at 10:41:44, 10:43:01 and 10:43:28. Each run failed with `bucket does not exist`, which means that the hidden layer database held no layer records at all. At 10:43:54 BuildKit deleted a build record "due to missing blobs". Successful runs leave no line at this log level, so the log can't show whether the cleanup ran after the 10:54 mount.
  - **Work done in the split brain:** after the mount, `busybox:1.37.0` was pulled at 11:20:47, `docker:29-dind` at 11:23:41, and a build exported an image at 11:44:14.
  - **Image checks:** 19,848 failed checks: 47 in the 10:40s, then from 677 in the 11:00s up to 10,200 in the 11:40s. Almost all failed with `context canceled`: the client gave up waiting.
  - **The hidden database held at least 786 images.** Docker logs only the checks that fail, so it may hold more.
    - The real database has no record for 721 of their digests.
    - It records the other 65, and all 65 files are present; none is missing.
    - The 65 are 44 images from DeepSWE's registry (`public.ecr.aws/d3j8x8q7/swe-bench-202605`, recorded 4 to 5 weeks ago), 18 `jefzda/sweap-images`, and 3 others.
  - **The 14 TB disk** was mounted only once in the kernel log, at 12 Oct 10:54:05, so it stayed mounted from before 13 Sep until 12 Oct. It was grown online on 21 Sep at 12:52 (to 11.5 TB) and on 5 Oct at 10:43 (to 15.0 TB).

### 9.1d Which kind of layer damage each loss left (Q14, Q17)

- **Where:** your PC.
- **Why:**
  - containerd keeps each layer in two places: a record in the layer database, and a folder on disk. The error tells which of the two is gone:
    - `missing parent … bucket: not found`: the layer database has no record of a layer that the main database still lists. The record went together with its folder.
    - `failed to stat parent … no such file or directory`: the record is still there, and only the folder is gone.
  - Counting each kind per day shows which loss left which kind. The first date of one broken layer shows when it was already broken.
- **Risk:** none. It reads `logs/damage_errors.txt` and writes `analysis/layer_errors.txt`.

Both kinds, per day:

```bash
cd ~/cloud-worker-evidence/logs
{
  for kind in 'missing parent' 'failed to stat parent'; do
    echo "== $kind: $(LC_ALL=C grep -a -c "$kind" damage_errors.txt) lines. Lines per day:"
    LC_ALL=C grep -a "$kind" damage_errors.txt | cut -c1-10 | sort | uniq -c
    echo "the first:"
    LC_ALL=C grep -m1 -a "$kind" damage_errors.txt | cut -c1-600
  done
} > ../analysis/layer_errors.txt
cat ../analysis/layer_errors.txt
```

Then the first dates of three missing parents: those of `alpine:3.20` and `alpine:latest` (the start test's two `missing parent` images), and the one in the first line above:

```bash
cd ~/cloud-worker-evidence/logs
for k in 'moby/8431/sha256:08bc4e' 'moby/10103/sha256:34884a' 'moby/2776/sha256:8d853c'; do
  echo "== $k: $(LC_ALL=C grep -a -c "$k" damage_errors.txt) lines. Lines per day:"
  LC_ALL=C grep -a "$k" damage_errors.txt | cut -c1-10 | sort | uniq -c
done >> ../analysis/layer_errors.txt
tail -n 14 ../analysis/layer_errors.txt; ls -la --time-style=+%H:%M ../analysis/layer_errors.txt
```

- **Expected:** the lines per day of each kind, and the days on which each of the three parents failed.
- **Result on 15 Oct, about 16:15:**
  - **`missing parent`: 221 lines, from 14 Sep to 14 Oct,** before and after the 12 Oct restart: 202 up to 11 Oct, including 99 on 18 Sep and 74 on 6 to 10 Oct. The first, on 14 Sep at 12:05, names parent `moby/2776/sha256:8d853c…`.
  - **`failed to stat parent`: 59 lines, all on 14 Oct.** The first names folder 154281, one of the 10 Sep folders that are gone.
  - So before 12 Oct, layers failed because their record was gone as well as their folder. After the 12 Oct split brain, they fail because only the folder is gone.
  - **`alpine:3.20`'s missing parent (`moby/8431/…`) already failed on 30 Sep,** and again every day from 6 Oct. `alpine:latest`'s (`moby/10103/…`) failed on 29 Sep. So these stale records existed before 6 Oct. `alpine:3.20` was pulled on 24 Jul.
  - Docker logged no error at all on 2 to 4 Oct, and one on 5 Oct (`analysis/errors_per_day.txt`). 6 and 7 Oct are when the Jenkins builds met these records, not necessarily when they broke.
  - `already exists` never appears in the log, so this check can't date that kind (`alpine:3.23.4`, first seen in build #53 on 6 Oct).
  - Before this file was written, the same greps were run once with their output printed to the screen only.

### Before 9.2 and 9.5: log in, and record the session again

- **Where:** terminal 1.
- **Why:** the recording of 15 Oct ended at step 8.1. A new `script` writes a new file in `sessions/`, named by the date and time, so the follow-up is recorded as well. The folders from step 2.3 already exist.
- **Risk:** none.

```bash
ssh toby@47.84.16.18
cd ~/snapshot-audit/logs
script -q -a ~/snapshot-audit/sessions/"$(date +%F_%H%M)".log
```

- **Expected:** the prompt `toby@iZt4ndd2dff7gqjta7mppaZ:~/snapshot-audit/logs$`, then no output from `script`. Everything is recorded until you type `exit` after step 9.5.
- **Result (15 Oct, about 17:48):** the prompt came back and `script` printed nothing. The recording is running.

### 9.2 Every boot and shutdown in the login records (Q5, Q15)

- **Where:** terminal 1, the worker as `toby`. No sudo.
- **Why:**
  - The journal lists only the boots whose log it still keeps (3.9 GB cap): 24 Jul, 11 Sep and 12 Oct.
  - The login records (`wtmp`) list every boot and shutdown for as long as they are kept.
  - The hidden store's dates (4 Aug 20:44 and 27 Aug 20:26) may be boots that skipped `/data`.
- **Risk:** none. Everyone may read these files.

```bash
cd ~/snapshot-audit/logs
ls -la --full-time /var/log/wtmp* /var/lib/wtmpdb > reboots.txt 2>&1        # which login records exist, and their dates
last -x -F >> reboots.txt 2>&1                                               # every login, boot and shutdown, newest first
last -x -F -f /var/log/wtmp.1 >> reboots.txt 2>&1                            # the previous file, if there is one
grep -E '^(reboot|shutdown)|begins|last:|wtmpdb' reboots.txt
```

- **Expected:**
  - One `reboot` line per boot, with its full date: at least 12 Oct 10:38 and 11 Sep 09:14.
  - A line `wtmp begins …`, or `wtmpdb begins …`, with the oldest date the records keep.
  - A boot on 4 Aug or 27 Aug is the main lead. Records that start after August are a result too.
  - Newer Ubuntu releases may keep these records in `/var/lib/wtmpdb` instead. Then one of the commands prints an error into the file; that is fine.
- **Send me:** the output of the `grep`.
- **Result (15 Oct, about 17:49):** no boot or shutdown lines.
  - The `ls` line says `/var/lib/wtmpdb` does not exist.
  - Both `apt install wtmpdb` lines are the shell saying `last` is not installed. Ubuntu 26.04 no longer ships `last`, and its replacement `wtmpdb` is not installed either. Nothing was installed. Do not install it: that needs root and changes the shared machine.
  - Step 9.2b reads `/var/log/wtmp` without `last`.

### 9.2b The same records, read without `last` (Q5, Q15)

- **Where:** terminal 1, the worker as `toby`, inside the recording. No sudo.
- **Why:**
  - `/var/log/wtmp` is a plain list of fixed-size records: type, process, terminal, user, host and time. Python reads it directly.
  - Boot records have the user `reboot`, and shutdown records have the user `shutdown`. This Ubuntu may no longer write them; then the step prints none, and that is a result too.
  - The file may reach further back than the journal, so the step also lists the logins on the days of the hidden store (4 Aug, 27 Aug), of the 11 Sep boot, and of the restarts on 12 and 14 Oct.
- **Risk:** none. Python only reads the file, which everyone may read. It is a few hundred KB. It installs nothing.

```bash
cd ~/snapshot-audit/logs
grep '/var/log/wtmp' reboots.txt                                             # must show the file before the next command
python3 - /var/log/wtmp > reboots_wtmp.txt <<'EOF'
import collections, struct, sys, time
FMT = '<hxxi32s4s32s256shhiii16s20s'               # the glibc login record on x86-64, 384 bytes
size = struct.calcsize(FMT)
data = open(sys.argv[1], 'rb').read()
recs = [struct.unpack_from(FMT, data, o) for o in range(0, len(data) - size + 1, size)]
text = lambda b: b.split(b'\0', 1)[0].decode(errors='replace')
when = lambda r: time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(r[9]))
print(f'{len(recs)} records, {len(data) % size} bytes left over')
if recs:
    print('wtmp begins', when(recs[0]), '| ends', when(recs[-1]))
print('records per type:', dict(collections.Counter(r[0] for r in recs)))
print('== boot and shutdown records (type 1 or 2, or user reboot/shutdown)')
for r in recs:
    if r[0] in (1, 2) or text(r[4]) in ('reboot', 'shutdown'):
        print(when(r), 'type', r[0], text(r[4]), text(r[2]), text(r[5]))
days = ('2026-08-04', '2026-08-27', '2026-09-10', '2026-09-11', '2026-10-11', '2026-10-12', '2026-10-14')
print('== logins and logouts (type 7 and 8) on', ', '.join(days))
for r in recs:
    if r[0] in (7, 8) and when(r)[:10] in days:
        print(when(r), 'in ' if r[0] == 7 else 'out', text(r[4]), text(r[2]), text(r[5]))
EOF
head -n 5 reboots_wtmp.txt; grep -c . reboots_wtmp.txt
```

- **Expected:**
  - The `grep` prints one line for `/var/log/wtmp`, with a size. If it prints nothing, stop and send me `cat reboots.txt`.
  - The first line of `head` ends with `0 bytes left over`. Anything else means the record layout differs; then stop and send me the output.
  - `wtmp begins …` is the oldest date the file keeps. A date after August is a result too: then `wtmp` cannot date the hidden store.
  - Types: 7 is a login and 8 a logout. 2 is a boot and 1 a shutdown or runlevel change.
- **Send me:** the `grep` line, then the output of `head` and `grep -c`. If the line count is under about 200, send `cat reboots_wtmp.txt` as well.
- **Result (15 Oct, about 17:50):**
  - `/var/log/wtmp` is there: 395,136 bytes, `root utmp`, last change 15 Oct 17:48:09.
  - `1029 records, 0 bytes left over`, so the layout is right.
  - `wtmp begins 2026-07-24 15:15:32 | ends 2026-10-15 17:48:09`. That is the same day as the oldest boot in the journal, so the file covers 4 Aug and 27 Aug.
  - `records per type: {6: 10, 7: 1019}`: 1,019 logins and 10 login prompts. No boot, shutdown or logout record. On this machine nothing writes them, so `wtmp` cannot date the boots. Step 9.2c uses the 10 prompts.
  - `cat reboots_wtmp.txt` (15 Oct, about 17:52): 99 terminal logins on the chosen days, and none on 11 Oct. The full list stays in that file. What it shows:
    - `wtmp` keeps terminal logins only. A login on `pts/0` means every earlier holder of `pts/0` had left.
    - **4 Aug:** the boot prompts are at 20:44:37. `xu` logged in at 20:44:45 on `pts/0`, and root at 20:44:51 on `pts/1`. Root had a terminal at 20:35:53, before the boot.
    - **27 Aug:** the morning logins were on `pts/50` and `pts/51`. After the 20:25 boot, `xu` logged in at 20:26:40 on `pts/0`. After the 21:35 boot, root logged in at 21:38:23 on `pts/0`.
    - **11 Sep:** no boot. `xin` was on `pts/5` at 09:12:20, and the next new terminal, at 09:58:46, was `pts/13`. Sessions from before 09:14 were still open.
    - **12 Oct:** `xu` logged in at 10:38:30 on `pts/0`. Until 11:28 the terminal logins alternate between `xu` and root, and each new one takes `pts/0`, so the previous one had ended. At the 10:54:05 mount the open terminals were root (since 10:50:30), `xin` (since 10:50:46) and `toby` (since 10:53:29). The next root terminal opened at 11:28:57.
    - **14 Oct:** no root terminal. The containerd restarts that day were not host boots.

### 9.2c Login prompts and console logins: the boots (Q5, Q15)

- **Where:** terminal 1, the worker as `toby`, inside the recording. No sudo.
- **Why:**
  - The 10 records of type 6 are login prompts. The system starts one on each console (`tty1`, `ttyS0`) at every boot, so their dates are boot dates. A small process number (under about 5,000) also means early in a boot.
  - A login whose terminal isn't `pts/…` was typed at the console, for example in the cloud provider's web console. That is how a stuck host is restarted and fixed.
- **Risk:** none. It reads the same file again.

```bash
cd ~/snapshot-audit/logs
python3 - /var/log/wtmp > reboots_console.txt <<'EOF'
import struct, sys, time
FMT = '<hxxi32s4s32s256shhiii16s20s'               # the glibc login record on x86-64, 384 bytes
size = struct.calcsize(FMT)
data = open(sys.argv[1], 'rb').read()
text = lambda b: b.split(b'\0', 1)[0].decode(errors='replace')
print('== login prompts (type 6), and logins (type 7) not on pts/...')
for o in range(0, len(data) - size + 1, size):
    r = struct.unpack_from(FMT, data, o)
    if r[0] == 6 or (r[0] == 7 and not text(r[2]).startswith('pts/')):
        print(time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(r[9])), 'type', r[0], 'pid', r[1], text(r[2]), text(r[4]), text(r[5]))
EOF
cat reboots_console.txt
```

- **Expected:** the header, then the 10 prompt lines and any console logins.
  - A type 6 line on 4 Aug about 20:44, 27 Aug about 20:26, or 12 Oct about 10:38 confirms a boot there.
  - No type 6 line on 11 Sep means no boot that day.
- **Send me:** the output of `cat`.
- **Result (15 Oct, about 17:52):** ten login prompts, and no console login. Each boot starts one prompt on `tty1` and one on `ttyS0`, a few seconds apart, with a process number near 5,600. The five boots are:

```text
2026-07-24 15:15:32  tty1 and ttyS0
2026-08-04 20:44:37  tty1 and ttyS0
2026-08-27 20:25:23  tty1 and ttyS0
2026-08-27 21:35:55  tty1 and ttyS0
2026-10-12 10:38:21  tty1 and ttyS0
```

  - **4 Aug 20:44:37** is two seconds after the hidden store was created (20:44:35). The store was created during that boot.
  - **27 Aug 20:25:23** is 69 seconds before the hidden layer folder changed (20:26:32).
  - **27 Aug 21:35:55** is a second boot that evening. The journal did not keep it.
  - **12 Oct 10:38:21** matches the known boot at 10:38:06, so a prompt pair does mean a boot.
  - **No prompt on 11 Sep, or on any day between 27 Aug 21:35 and 12 Oct 10:38.** The host stayed up for that whole stretch. 11 Sep 09:14 was not a boot.
  - **No prompt on 14 Oct.** The containerd restarts that day were not host boots.
  - Nobody logged in on `tty1` or `ttyS0`. The prompts are the system waiting, not a person at the console.

### 9.3 The history and contents of both databases, by root (Q15, Q16)

- **Where:** a root shell on the worker. Prefer root to run it. If root cannot run it the same day, Toby may open that shell with `root.pem`, as tested in step 0.3, and run only the block below.
- **Record it either way.** Tell the mentor the same day: who ran it, that it was read-only, and that `root.pem` was used if it was. Do not add any new sudo right for `toby`. Do not repair, delete, prune or restart anything in this step.
- **First:** show the mentor this report. Then either send root this message, or run the block yourself:

```text
For the Docker store investigation (CLOUD_WORKER_INVESTIGATION_STEPS.md, step 9.3): could you run
the read-only block of step 9.3 as root? It mounts a read-only view of the system disk, as in step 6.4,
and lists the dates and sizes of the two containerd stores. It reads the 14 TB filesystem's
superblock with tune2fs -l. From the two meta.db files it extracts only sha256 digests and record
dates, not names or settings. It changes nothing in the store, and removes the view at the end.
The output goes to /home/toby/snapshot-audit/. About 5 minutes.
```

- **Why:**
  - **When each store was created:** the birth time of each database. `tune2fs -l` gives the 14 TB filesystem's creation date and its mount count.
  - **When the hidden store was used:** the dates of the hidden layer database and its folders.
  - **Which files the hidden database lists (Q16):** containerd's cleanup keeps exactly the files that its open database lists. Step 9.4 compares the list with the files present and missing.
  - **On which days each database was in use:** containerd stores a creation time with every record. Counted per day:
    - for the hidden database, they date the earlier split brains;
    - for the real database, they show whether it really recorded nothing from about 16 Aug to 3 Sep.
  - **Only digests and dates leave the databases.** `meta.db` also holds container settings, which can include other users' keys, so it is not copied.
- **Risk:** none for the store.
  - The view is read-only, as in step 6.4, and the last commands remove it. `findmnt` must show `ro`. **If it shows `rw`, stop:** run only `umount /mnt/rootfs`.
  - `stat`, `ls`, `tune2fs -l` and `grep` only read.
  - Python opens both databases read-only and maps them into memory instead of loading them, so it uses little memory. containerd keeps working.
  - The output goes to `/home/toby/snapshot-audit/`.

Run these one at a time, in this root shell. Stay in the shell until 9.3e prints `view removed`. Each block only reads. Send the output of each block before the next.

**9.3a. Open the read-only view.**

```bash
mkdir -p /mnt/rootfs && mount -o bind,ro / /mnt/rootfs
findmnt -no TARGET,OPTIONS /mnt/rootfs
```

- **Result (16 Oct):** `/mnt/rootfs ro,relatime`. The view is read-only. It is still mounted. Continue with 9.3b in this same shell.

**9.3b. Dates and sizes of both stores, and the 14 TB filesystem.**

```bash
cd /home/toby/snapshot-audit
h=/mnt/rootfs/data/containerd
r=/data/containerd
{
  echo "== birth | last change | size | name"
  stat -c '%w | %y | %s | %n' $h $h/io.containerd.metadata.v1.bolt/meta.db $h/io.containerd.snapshotter.v1.overlayfs \
    $h/io.containerd.snapshotter.v1.overlayfs/* $r $r/io.containerd.metadata.v1.bolt/meta.db \
    $r/io.containerd.snapshotter.v1.overlayfs/metadata.db $r/io.containerd.content.v1.content /data/docker
  echo "== the 14 TB filesystem"
  tune2fs -l /dev/nvme1n1p1 | grep -E '^(Filesystem created|Last mount time|Last write time|Mount count|Last mounted on)'
  echo "== hidden layer folders: $(ls $h/io.containerd.snapshotter.v1.overlayfs/snapshots | wc -l)"
  ls -la --full-time $h/io.containerd.snapshotter.v1.overlayfs/snapshots $h/io.containerd.snapshotter.v1.native /mnt/rootfs/data/docker
} > logs/store_history.txt 2>&1
cat logs/store_history.txt
```

- **Result (16 Oct):** `logs/store_history.txt` was written. The view stayed mounted.
  - **Old store, on the system disk.** `meta.db` born 4 Aug 20:44:35, last written 12 Oct 11:48:43, 906,985,472 bytes. The layer database `overlayfs/metadata.db` was born 27 Aug 20:26:32, last written 12 Oct 11:48:43, 262,144 bytes. Its `snapshots` folder has 0 layer folders and has not changed since 4 Aug 20:44:35. There is no downloaded-files folder. The hidden `/data/docker` directory was last changed at 12 Oct 10:39:46, before the 10:54 mount.
  - **Store in use, on the 14 TB disk.** The `containerd` directory, the layer database and the downloaded-files directory were born 24 Jul 17:14. The layer database is still the file from that day (last written 16 Oct 10:43). The main `meta.db` file now on disk was born 14 Oct 20:08:59, 654,802,944 bytes, and was still being written at 10:43. That birth is the file, not the date of every record inside it. Record dates are step 9.3d.
  - **The 14 TB filesystem** was created 24 Jul 15:54:51, last mounted 12 Oct 10:54:05 on `/data`, mount count 4, last write 16 Oct 09:54:29. It has not been mounted again since 12 Oct.

**9.3c. Digests named in the old database.** This can take a few minutes. It writes one digest per line and does not print them.

```bash
LC_ALL=C grep -aoE 'sha256:[0-9a-f]{64}' $h/io.containerd.metadata.v1.bolt/meta.db | sort -u > inventory/hidden_db_digests.txt
wc -l inventory/hidden_db_digests.txt
```

- **Result (16 Oct):** `180168 inventory/hidden_db_digests.txt`. These are unique `sha256:` strings found in the old database file, including copies a database keeps after a record is deleted. The count is not a count of files, and not a count of images. Step 9.4 compares the list with the files present and missing. The view is still mounted.

**9.3d. How many records each database created on each day.**

```bash
python3 - $h/io.containerd.metadata.v1.bolt/meta.db $r/io.containerd.metadata.v1.bolt/meta.db > logs/db_record_days.txt <<'EOF'
import collections, datetime, mmap, re, struct, sys
for path in sys.argv[1:]:                                    # each database is only read
    days = collections.Counter()
    with open(path, 'rb') as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as db:
        for m in re.finditer(rb'createdat', db):
            v = db[m.end():m.end() + 15]
            if len(v) == 15 and v[0] == 1:                   # a creation time as containerd stores it (Go's MarshalBinary)
                s = struct.unpack('>q', v[1:9])[0] - 62135596800
                if 1.7e9 < s < 1.9e9:
                    days[datetime.datetime.fromtimestamp(s, datetime.timezone(datetime.timedelta(hours=8))).date()] += 1
    print(f'== {path}: records created per day (CST)')
    for d, n in sorted(days.items()):
        print(d, n)
EOF
cat logs/db_record_days.txt
```

- **Result (16 Oct):** `logs/db_record_days.txt` was written. The counts are activity per day, including old copies a database keeps, not a live inventory. The view is still mounted.
  - **24 Jul to 4 Aug:** the 14 TB database has the records (8,572 on 24 Jul, 8,333 on 25 Jul, then small counts, and 250 on 4 Aug). The old database does not exist yet. The 14 TB filesystem was created 24 Jul 15:54.
  - **4 Aug to 10 Sep:** the old database has records on almost every day, from 3,259 on 4 Aug (the store was created at 20:44 that evening) through 251 on 10 Sep. The 14 TB database has none from 5 Aug through 9 Sep. So for those five weeks containerd was writing the system-disk database, not the 14 TB one.
  - **10 Sep:** the 14 TB database resumes, with 5,691 records, and the old database drops to 251 and then stops. This is the same day as the oldest layer folder still on the 14 TB disk (10 Sep 09:22).
  - **11 Sep to 11 Oct:** only the 14 TB database. Quiet days with no new records: 13 Sep, and 2, 3 and 4 Oct.
  - **12 Oct:** both. The old database has 229 records, and the 14 TB database has 489. That is the known split-brain day.
  - **13 to 16 Oct:** only the 14 TB database (2,333, 3,336, 2,093, and 89 on the morning of 16 Oct). The old database has no record dated after 12 Oct.

**9.3e. Remove the view, and make the three files readable by `toby`.**

This is still investigation. It does not fix the store. `umount` removes only `/mnt/rootfs`, the read-only view opened in 9.3a. It does not unmount `/data`, and it does not stop Docker or containerd. The old store stays where it is, hidden under the mount. `chown` changes the owner of the three result files in `/home/toby/snapshot-audit/` so `toby` can copy them in step 9.4. It does not change `meta.db`, a blob, or a layer folder.

```bash
umount /mnt/rootfs
findmnt /mnt/rootfs || echo "view removed"
chown toby: logs/store_history.txt logs/db_record_days.txt inventory/hidden_db_digests.txt
```

- **Result (16 Oct):** `view removed`. The root shell was then closed. `/data` stayed mounted. Nothing in the store was changed. The three result files are owned by `toby`.
- **Expected:**
  - 9.3a shows `/mnt/rootfs ro,…`, and 9.3e prints `view removed`.
  - **`store_history.txt`:**
    - the hidden `meta.db` born on 4 Aug at about 20:44:35;
    - the hidden layer database (`…overlayfs/metadata.db`) born on 27 Aug at 20:26:32, if that is when it was first opened;
    - the birth time of the real database;
    - the 14 TB filesystem's creation date, a `Last mount time` of 12 Oct 10:54:05, and its mount count.
  - **`db_record_days.txt`:** a line for each day with records, for each database.
    - The days listed for the hidden database are the days containerd used it. Days in August, or on 11 and 12 Sep, are earlier split brains.
    - A gap from about 16 Aug to 3 Sep in the real database means containerd used the other database then.
    - The counts include old copies of records that a database keeps in its free space. Read them as activity per day, not as exact numbers.
  - **`hidden_db_digests.txt`:** thousands of lines.
- **Send me:** the whole output, including the line count.

### 9.4 Compare the hidden database with the files (Q16)

- **Where:** your PC, after step 9.3.
- **Why:** if the split brains deleted the files, the files that survived are the ones that the hidden database names, and the missing files are not.
- **Risk:** none. It copies three small files from the worker and writes `analysis/hidden_db_match.txt`.

```bash
cd ~/cloud-worker-evidence
scp toby@47.84.16.18:snapshot-audit/inventory/hidden_db_digests.txt inventory/
scp toby@47.84.16.18:snapshot-audit/logs/store_history.txt toby@47.84.16.18:snapshot-audit/logs/db_record_days.txt logs/
python3 - <<'EOF' | tee analysis/hidden_db_match.txt
import collections

def hours(age):                                # the same reading of ctr's ages as in step 9.1
    n, unit = (age.split() + ['', ''])[:2]
    if age.startswith('About'):
        return 1
    for k, v in (('second', 0), ('minute', 0), ('hour', 1), ('day', 24), ('week', 168), ('month', 720)):
        if unit.startswith(k):
            return int(n) * v
    return 10**9

hidden = {l.strip().split(':')[-1] for l in open('inventory/hidden_db_digests.txt') if l.strip()}
present = {l.strip() for l in open('inventory/blob_files.txt') if l.strip()}
unrecorded = {l.strip() for l in open('inventory/blobs_unrecorded.txt') if l.strip()}
t = collections.Counter()
with open('inventory/content_records.txt') as f:
    next(f)
    for line in f:
        p = line.split()
        if not p:
            continue
        age = []
        for w in p[2:]:
            if '=' in w or w == '-':
                break
            age.append(w)
        h = hours(' '.join(age))
        period = 'after the restart' if h < 72 else '2 to 12 Oct' if h < 336 else 'before 2 Oct'
        how = 'pulled' if 'containerd.io/distribution.source.' in line else 'made here'
        d = p[0].split(':')[-1]
        t[(period, how, 'present' if d in present else 'missing', d in hidden)] += 1
print(f'digests named in the hidden database: {len(hidden)}')
print('recorded\tsource\tfile\tnamed in the hidden database\tnot named')
for period in ('after the restart', '2 to 12 Oct', 'before 2 Oct'):
    for how in ('pulled', 'made here'):
        for state in ('present', 'missing'):
            print(f'{period}\t{how}\t{state}\t{t[(period, how, state, True)]}\t{t[(period, how, state, False)]}')
print(f'files on disk with no record in the real database: {len(unrecorded)}; named in the hidden database: {len(unrecorded & hidden)}')
EOF
```

- **Expected:**
  - **The split brains deleted the files** if, among the files recorded before the restart, nearly all the present ones are named and nearly none of the missing ones.
  - **A few missing files may be named**, because a database keeps old copies of deleted records in its free space. Many would mean the files were deleted some other way; the database would then need an exact reading.
  - Files recorded after the restart are not part of the test.
- **Send me:** the output.
- **Result (16 Oct):** the test passes. `analysis/hidden_db_match.txt`. The old database file names 180,168 digests.
  - **Before 2 Oct, still on disk:** 2,626 files. 2,624 are named in the old database (2,547 pulled, 77 made here). 2 pulled files are present and not named.
  - **Before 2 Oct, file gone:** 37,124 records. 36,969 are not named in the old database. 155 are named (151 pulled, 4 made here). That is 0.4%, the few free-space copies the check allowed.
  - **2 to 12 Oct:** 5,791 missing, and none of them are named in the old database. 4 files are still present, and 1 of those is named.
  - **After the 12 Oct restart:** 634 present, 0 missing. This period is not part of the test.
  - **97 files on disk with no record in the current database.** None of them are named in the old database.
  - The cleanup kept the files the old database named and deleted the ones it did not.

### 9.5 What filled the logs from 6 Oct? (Q10)

- **Where:** terminal 1, the worker as `toby`. No sudo: `toby` is in the `adm` group, which may read the logs.
- **Why:** Docker wrote 22 million lines in `syslog.1` (4 to 11 Oct), but the damage errors are only 1.3 million of them. On 12 Oct the flood was image checks that the client gave up on (step 9.1c). Counting Docker's messages by kind shows what filled 6 to 11 Oct.
- **Risk:** none. It reads about 15 GB at low priority (`nice`), in 5 to 10 minutes, and writes one small file.
- **Run it before Sunday 18 Oct.** The weekly rotation then compresses `syslog.1`, and this command no longer finds it. If it is later than that, send me a note instead.

```bash
cd ~/snapshot-audit/logs
nice grep -a ' dockerd\[' /var/log/syslog.1 | grep -aoE 'level=[a-z]+ msg="[^"]{0,60}' | sed -E 's/[0-9a-f]{12,}/<id>/g; s/[0-9]+/<n>/g' \
  | awk '{c[$0]++} END {for (k in c) print c[k] "\t" k}' | sort -rn | head -n 30 > dockerd_messages.txt
cat dockerd_messages.txt
```

- **Expected:** up to 30 lines, each a count and a message, largest first. Together they make up most of the 22 million.
- **Send me:** the output.
- **Then stop recording:** type `exit` once, as in step 8.1. Run step 9.7 after this, so that the copy of `sessions/` includes the new recording.
- **Result (15 Oct, the command finished; the connection then dropped):** `dockerd_messages.txt` was written and printed. Of Docker's lines in `syslog.1`, 20,275,470 are `checking availability of platform specific manifest failed` and 1,346,163 are `failed to check image labels`. Together that is almost all of the 22 million. `image pulled` is 9 lines and `image created` is 1,041. The flood is Docker re-checking images whose files are already missing. It is not a third kind of deletion.
- The SSH session then died (`Connection reset by peer`) before `exit`. The next section resumes from there. Do not run the `grep` again.

### Resume after 9.5: the connection dropped

- **Where:** terminal 1, then terminal 2.
- **Why:** the 9.5 file is already on the worker. This checks it is there, starts a short recording of that check, and leaves. Step 9.7 then copies it to the PC.
- **Risk:** none. `ls` only reads. `exit` only ends the login.

```bash
ssh toby@47.84.16.18
cd ~/snapshot-audit/logs
script -q -a ~/snapshot-audit/sessions/"$(date +%F_%H%M)".log
ls -la --full-time dockerd_messages.txt
wc -l dockerd_messages.txt
exit
exit
```

- **Expected:** `dockerd_messages.txt` exists, dated 15 Oct, and `wc` prints 30. `script` prints nothing. The second `exit` returns you to the PC. Then run 9.7.
- **Result (16 Oct 09:14):** the file is 1,831 bytes, dated 15 Oct 18:07:32, and `wc` printed 30. After `exit` the prompt was still `toby@iZt4ndd2dff7gqjta7mppaZ`. Step 9.8 was run from that prompt. One more `exit` is still needed before 9.7.

### 9.6 Ask root, xu and the users, in writing (Q2, Q4, Q8, Q11, Q12, Q14, Q15)

- **Where:** the team chat, or a message to each person. This does not use the worker, so it can go out while step 9.5 is still running.
- **Why:** some answers are only in people's memory, or in the Alibaba Cloud console. Written answers go into the record as they are.
- **Do not wait for answers.** Assume none come back before the rest of the investigation. Steps 9.5, 9.7, 9.3 and 9.4 go ahead. An answer, if one arrives later, is added to the record then.
- **Risk:** none.

**What to tell root now.** He asked whether many images have been removed, and whether the data is gone or only stored somewhere other than where the records point. This is already answered by the disk check of 15 Oct (steps 5.1, 5.2 and 9.1). Send this, and send the questions below as well:

```text
The image names are still in Docker's list: 7,750 on 15 Oct. What is gone is the data behind most of them, and it is not sitting in another folder.

On the 14 TB disk, containerd's own lists were compared with the files:
- 46,179 downloaded-file records, and 3,361 files on disk. 42,915 records point at a file that is not there. Those missing records add up to 481 GB. The recorded files still on disk add up to 405 GB. 97 files are on disk with no record. So the mismatch is "the record remains, the file was removed", not "the file moved".
- The 14 TB disk was not wiped. It was 88% used, with 1.7 TB free. The 481 GB is how much recorded file data is gone. The disk is still mostly full because the surviving data is still there. A full disk is not what deleted the files. containerd's cleanup did, while it held the old database.
- 19,581 layer records, and 10,164 layer folders. The missing folders are not elsewhere on that disk. The oldest folder still there is from 10 Sep.
- Of 6,847 images tested, 254 start. 6,313 fail because the downloaded file is gone, 253 because the unpacked layer folder is gone (all 113 DeepSWE images: their downloaded files are still on disk, the unpacked folders are not), and 27 because of a stale record.
- The only other store is the old one hidden on the system disk, under the /data mount. It holds a database and no downloaded files. Its folder has not changed since 4 Aug 20:44. The missing files are not in it.
- Everything recorded after the 12 Oct 13:07 restart is still on disk (634 files, none missing). The disk itself was not wiped.

Nothing has been repaired or deleted by this investigation. A full comparison of the hidden database's list still needs you to run step 9.3, which is read-only.
```

To root:

```text
Questions for the Docker store investigation (CLOUD_WORKER_DOCKER_REPORT.md, section 5.2).
Nothing will be changed on the worker until the mentor and you have reviewed the result.
1. 12 Oct: who restarted the host at about 10:38, and how (Alibaba Cloud console, or a command)?
2. 12 Oct: who mounted /data at 10:54 (the mkdir/cp/mount/echo one-liner)? Were Docker and containerd stopped first?
3. 12 Oct: who rewrote /etc/fstab to the UUID line at 12:09, and added the systemd drop-ins?
4. 14 Oct: who restarted containerd at 11:11 and 17:36, and why?
5. When was the 14 TB disk attached, and first mounted on /data?
6. The host did not boot on 11 Sep. After the 27 Aug 21:35 boot it stayed up until 12 Oct 10:38. Who mounted /data during that stretch, and when? Were Docker and containerd stopped first?
7. Who restarted the host on 4 Aug at 20:44, and twice on 27 Aug (20:25 and 21:35)? After each, was /data mounted before containerd started?
8. Was the store ever moved or copied, or were old files or folders deleted to free space?
9. Could you export the console's history of restarts and disk changes (the resizes on 21 Sep and 5 Oct)?
```

To xu:

```text
For the Docker store investigation: root ran your scripts in /home/xu/check_docker/scripts/, several with --apply.
1. What do they change with --apply? Do they remove or re-create images, layers or downloaded files?
2. On which dates were they run with --apply, and what did evidence/20261007/missing-targets.txt list?
```

To the team:

```text
For the Docker store investigation on the cloud worker (nothing is removed now):
1. Did any job use a lot of memory on 11 Oct from about 15:00?
2. Does anyone run a clean-up script or service that deletes files under /data?
3. Whose are these two containers, created on 12 Oct during the incident? Both will fail at their next start.
   - clarify-selection4-runtime-v3-20261012 (on docker:29-dind)
   - the Harbor egress sidecar of bandit-incremental-cache-control
4. Did anyone restart Docker or containerd, or mount /data, on 4 Aug, 27 Aug, or 10 to 13 Sep? The host itself did not boot between 27 Aug 21:35 and 12 Oct 10:38.
```

- **Record:** each answer in the record, section 5.2, with its date.

### 9.7 Copy the rest of the evidence to your PC

- **Where:** terminal 2, your PC, after steps 9.2 to 9.2c and 9.5.
- **Why:** before any repair, every piece of evidence needs a second copy. `logcopy/` (3.3 GB) and `sessions/` are still only on the worker.
- **Risk:** none. It reads from the worker and writes on your PC.

```bash
df -h ~                                                                      # needs more than 5 GB free
scp -r toby@47.84.16.18:snapshot-audit/logcopy toby@47.84.16.18:snapshot-audit/sessions ~/cloud-worker-evidence/
scp toby@47.84.16.18:snapshot-audit/logs/{reboots.txt,reboots_wtmp.txt,reboots_console.txt,dockerd_messages.txt} ~/cloud-worker-evidence/logs/
du -sh ~/cloud-worker-evidence/*
```

- **Expected:** `logcopy` of about 3.3 GB and `sessions` in `~/cloud-worker-evidence/`, and the four files in `logs/`.
- **Result (16 Oct):** the PC has 530 GB free. The copy finished. `logcopy` is 3.3 GB, `sessions` is 164 KB (the 15 Oct 12:05 and 17:48 recordings and the 16 Oct 09:15 check), `logs` is 1.4 GB, and the four named files arrived (`reboots.txt`, `reboots_wtmp.txt`, `reboots_console.txt`, `dockerd_messages.txt`).

### 9.8 Does the fstab line parse, without a reboot (Q1)

- **Where:** terminal 1, the worker as `toby`. No sudo. Run it whenever you are logged in. It does not need the recording from 9.5.
- **Why:** root rewrote `/etc/fstab` on 12 Oct and said on 16 Oct that the new line has not been tested, because the host has not been rebooted. The previous line was skipped at boot even though it looked like a mount instruction. This check only asks whether the line is valid today, and whether `/data` is the 14 TB disk right now.
- **Risk:** none. Both commands only read. Do not reboot.

```bash
findmnt --verify
findmnt -no SOURCE,TARGET,FSTYPE,OPTIONS /data
```

- **Expected:**
  - `findmnt --verify` prints no `parse error` for the `/data` line.
  - The second line shows `/dev/nvme1n1p1` on `/data`.
  - A clean result means the line is valid while the machine is up. It does not mean the next boot will mount the disk. That stays a planned reboot, after the mentor and root agree, as in the mentor report, section 9.2.
- **Send me:** both outputs.
- **Tell root and the mentor:** the UUID line and the "do not start without `/data`" settings are written, and they are not protection until a watched reboot shows `/data` mounted before containerd starts.
- **Result (16 Oct, run as `toby`):** `0 parse errors, 0 errors, 3 warnings`. The warnings are `cannot detect on-disk filesystem type (Permission denied)` on `/`, `/boot/efi` and `/data`. That is `toby` being unable to read the disk itself, not a bad line. `/data` is `/dev/nvme1n1p1`, type `ext4`, mounted read-write. The line is valid while the machine is up. No reboot has tested it.

## If something goes wrong

| What you see | What it means | What to do |
|---|---|---|
| `Permission denied (publickey)` with `root.pem` | The key isn't accepted for root on this host | Stop. Give root the fingerprint from 0.2 |
| `UNPROTECTED PRIVATE KEY FILE` | Others can read the key | Run step 0.1 again |
| `I'm sorry toby` or `not allowed` after a sudo command | The command differs from the rule, even by one character | Copy the command again exactly. If it still fails, send it to me |
| `sudo: … incorrect password` | Typing mistake | Try again. After three failures, wait a minute |
| The connection drops | Nothing is lost; `nohup` jobs keep running | Log in, run step 2.3 again, and carry on |
| `findmnt` shows `rw` in step 6.4 or 9.3 | The view isn't read-only | Run only `umount /mnt/rootfs` (in 6.4 with `sudo`), then send me the output |
| `No such file` for `/var/log/syslog.1` in step 9.5 | The weekly rotation has compressed it | Stop, and send me `ls -la /var/log/syslog*` |
| `No space left on device` | The system disk is full | Stop, and send me `df -h / /home` |
| Anything else unexpected | | Stop, and send me the command and its output |
