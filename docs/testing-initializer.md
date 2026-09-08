# Initializer testing (Mac + Linux)

Use this checklist to verify the **one CLI initializer** (`mac-k3d prepare`) on macOS and Linux. Keep this open while debugging — step numbers stay stable.

Related design: [prepare-wizard.md](prepare-wizard.md), [setup.md](setup.md).

---

## Important: what is *not* a bug

`prepare --init-config` writes a **default** YAML where dependencies are still `source: install` (not yet installed).

So this is **expected** and means Step 2 **passed** the OS / validate path:

```text
ERROR docker is marked for install; run `mac-k3d prepare` to install
ERROR k3d is marked for install; ...
exit=1
```


| Outcome                                            | Meaning                                                                            |
| -------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `exit=1` + “marked for install” / missing binary   | **PASS** for Step 2 (validate ran; tools not ready yet)                            |
| `exit=1` + `macOS required` / unsupported platform | **FAIL** (platform gate broken)                                                    |
| `exit=0`                                           | Only after a real interactive prepare (or hand-edited config) made deps `existing` |


**Fix to proceed:** run **Step 3** interactive prepare (it installs/discovers tools and rewrites config). Do not treat Step 2’s `exit=1` as a blocker.

---

## Step 0 — Install CLI onto PATH

```bash
cd ~/Documents/Toby/mac-k3d   # or your clone path
cargo install --path .
which mac-k3d
mac-k3d --help
```

| Purpose | Put `mac-k3d` in `~/.cargo/bin` so the command works by name |
| Expect | `which` shows `.../.cargo/bin/mac-k3d`; help says macOS and Linux |

`cargo build --release` alone is **not** enough for `mac-k3d` on PATH — that only builds `./target/release/mac-k3d`.

---

## Step 1 — Platform gate

```bash
mac-k3d prepare --init-config -c /tmp/mac-k3d-gate-test.yaml
echo "exit=$?"

mac-k3d prepare --init-config -c /tmp/mac-k3d-gate-test.yaml
echo "exit=$?"
```

| Purpose | Prove Linux/macOS is allowed; write default config; second run is idempotent |
| Expect | `exit=0` both times (no `macOS required`) |

---

## Step 2 — Non-interactive validate (no wizard)

```bash
mac-k3d prepare --non-interactive -c /tmp/mac-k3d-gate-test.yaml
echo "exit=$?"
```

| Purpose | Fleet/CI path: validate without prompts |
| Expect (fresh init-config) | `exit=1` with “docker/k3d/kubectl marked for install” — **this is OK** |
| Fail | Any unsupported-platform / macOS-only error |

After Step 3 succeeds on a real config file, re-run non-interactive on **that** file and expect `exit=0`.

---

## Step 3 — Interactive worker prepare (main eval-box test)

Needs a reachable Jenkins controller if you register an agent (URL + API token).

```bash
mac-k3d prepare -i -c ~/.config/mac-k3d/worker.yaml
echo "exit=$?"
```

| Purpose | Full initializer: storage, role=worker, Docker/Harbor/Java, LoLBench, agent, disk check |
| Expect | Wizard finishes; `exit=0`; “Setup complete” |

Then:

```bash
grep -E '^(role|platform):' ~/.config/mac-k3d/worker.yaml
mac-k3d prepare --non-interactive -c ~/.config/mac-k3d/worker.yaml
echo "validate_exit=$?"
```

| Purpose | Confirm YAML + validate now passes |
| Expect | `role: worker`; `platform: linux` or `macos`; `validate_exit=0` |

### Agent daemon check

**Linux:**

```bash
systemctl --user status mac-k3d-jenkins-agent.service
loginctl enable-linger "$USER"   # once: survive logout
```

**macOS:**

```bash
launchctl print "gui/$(id -u)/com.mac-k3d.jenkins-agent" 2>&1 | head -20
```

| Purpose | Persistent Jenkins agent process installed by prepare |
| Expect | active/running (if API token / secret was set; otherwise script may wait for secret) |

---

## Step 4 — Interactive controller prepare

```bash
mac-k3d prepare -i -c ~/.config/mac-k3d/config.yaml
echo "exit=$?"
grep -E '^(role|platform):' ~/.config/mac-k3d/config.yaml
mac-k3d prepare --non-interactive -c ~/.config/mac-k3d/config.yaml
echo "validate_exit=$?"
```

| Purpose | Same CLI initializes controller role |
| Expect | `role: controller`; `validate_exit=0` |

---

## Step 5 — Dual config on one host

```bash
mac-k3d prepare --non-interactive -c ~/.config/mac-k3d/config.yaml
echo "controller_validate=$?"
mac-k3d prepare --non-interactive -c ~/.config/mac-k3d/worker.yaml
echo "worker_validate=$?"
grep '^role:' ~/.config/mac-k3d/config.yaml ~/.config/mac-k3d/worker.yaml
```

| Purpose | One binary, two `-c` files, no clobber |
| Expect | both validates 0; roles controller + worker |

---

## Step 6 — Disk hard-fail

```bash
mac-k3d prepare --non-interactive --disk-min-gb 99999 -c ~/.config/mac-k3d/worker.yaml
echo "exit=$?"
```

| Purpose | Under-disk must fail clearly on both OS volume scanners |
| Expect | `exit=1`, free-space error (not platform error) |

---

## Step 7 — Optional: controller start (secondary)

Only after Step 4 and Docker/k3d are available:

```bash
mac-k3d start -c ~/.config/mac-k3d/config.yaml
echo "start_exit=$?"
mac-k3d status -c ~/.config/mac-k3d/config.yaml
```

| Purpose | Prepare-produced config drives Docker + k3d |
| Expect | `start_exit=0` when runtime deps are ready |

Not required to accept the **initializer** itself (Steps 0–6).

---

## Cleanup

```bash
mac-k3d clean -c ~/.config/mac-k3d/worker.yaml --yes
mac-k3d clean -c ~/.config/mac-k3d/config.yaml --yes
rm -f /tmp/mac-k3d-gate-test.yaml
```

---

## Progress checklist

Copy and tick as you go:

- [x] Step 0 — `which mac-k3d` works
- [x] Step 1 — `--init-config` exit 0 (twice)
- [x] Step 2 — `--non-interactive` on gate file: exit 1 with “marked for install” (OK)
- [x] Step 3 — interactive worker prepare + validate_exit 0
- [x] Step 3 — agent daemon running (if token set)
- [x] Step 4 — controller prepare + validate_exit 0
- [x] Step 5 — dual-config validates
- [x] Step 6 — disk hard-fail exit 1
- [ ] Step 7 — (optional) start/status

---

## Where you are now

If you just saw “docker/k3d/kubectl marked for install” with `exit=1` on `/tmp/mac-k3d-gate-test.yaml`:

1. **Steps 1–2 are done** (that failure is expected).
2. **Next command (Step 3):**

```bash
mac-k3d prepare -i -c ~/.config/mac-k3d/worker.yaml
```

Choose **CI worker**, let it install or use existing Docker/Harbor/Java, point at your Jenkins URL when prompted.

---

## Troubleshooting

### `curl: (7) Failed to connect to localhost port 9080` / `agent.jar`

**Cause:** Worker prepare tries to download `agent.jar` from the Jenkins controller. Nothing is listening on `:9080` yet (controller not started).

**What still succeeded:** Docker / k3d / harbor / LoLBench clone may already be on disk even if prepare exited non-zero (older builds aborted before writing `worker.yaml`).

**Fix (recommended order for one-machine lab):**

1. Finish docker group (once):

```bash
sudo usermod -aG docker "$USER"
# log out and back in, then:
docker info
```

1. Reinstall CLI if you pulled a fix, then **re-run worker prepare** (choose “use existing” for tools). With current code, a missing controller is a **warning**, not a hard fail — config is saved.

```bash
cd ~/Documents/Toby/mac-k3d
cargo install --path .
mac-k3d prepare -i -c ~/.config/mac-k3d/worker.yaml
```

1. Later: prepare/start **controller**, then register the agent:

```bash
mac-k3d prepare -i -c ~/.config/mac-k3d/config.yaml   # role: controller
mac-k3d start -c ~/.config/mac-k3d/config.yaml
mac-k3d config -c ~/.config/mac-k3d/config.yaml --show-jenkins
# then worker:
mac-k3d config -c ~/.config/mac-k3d/worker.yaml
```

### `E: Unable to locate package kubectl`

**Cause:** Stock Ubuntu apt often has no `kubectl` package. Prepare falls back to the official curl binary install — check `which kubectl` (`/usr/local/bin/kubectl` is fine).