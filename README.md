# mac-k3d

Turn a **new Linux or Mac** into a **Jenkins controller or worker**. Download a GitHub Release binary; it installs Docker and the rest.

## Project goal

This project is a CI path for AI harness evaluation: many short jobs, each in its own sandbox, so one agent cannot read answers online or touch another run.

- **Jenkins** queues one task per build, caps CPU with a lock, and discards the job when it finishes. That is the parallelism and the short lifetime.
- **Harbor** runs DeepSWE, LoLBench, and SWE-bench Pro. The agent allowlist is the isolation in this tree: the sandbox may reach the DeepSeek API and nothing else.
- **k3d** already hosts Jenkins on the controller. The next sandbox is a short-lived k3d cluster per build: a default-deny NetworkPolicy (DeepSeek API only), image pulls through a Harbor registry proxy cache, then the cluster is deleted. That cluster, the NetworkPolicy, and the registry cache are **not** in this tree yet. Do not report them as shipped.

Pier is not the eval runner. It keeps a Docker Compose sandbox on the worker host, so it does not give a fresh Kubernetes network, a NetworkPolicy, or a registry cache. One Harbor runner is the path that can move onto that k3d sandbox next. End-to-end story: [docs/workflow.md](docs/workflow.md).

- **Controller** — Docker → k3d → Jenkins UI (`http://localhost:17070`)
- **Worker** — Docker + Java + Jenkins inbound agent (not a k3d node)

Users do **not** need Rust. Developers who build from source do.

## Requirements

The binary installs Docker, k3d, kubectl, helm, and Java when you choose **Install** in the wizard (`apt` on Ubuntu/Debian, Homebrew on macOS).

Leftovers it cannot hide:

- **Linux:** one logout after the `docker` group is added, then re-run `mac-k3d setup`
- **macOS:** first Docker Desktop window (and Homebrew if missing)
- **sudo** / brew for package install
- **Worker:** paste a Jenkins API token from the UI

If auto-install fails, install Docker yourself and re-run setup ([docs/new-machine.md](docs/new-machine.md)).

## Install (users)

1. Download the asset for your OS/arch from this repo’s GitHub **Releases** page (`mac-k3d-linux-x86_64`, `mac-k3d-linux-aarch64`, `mac-k3d-darwin-aarch64`, or Intel `mac-k3d-darwin-x86_64`).
2. `chmod +x` (macOS: also `xattr -d com.apple.quarantine`).
3. Run `./mac-k3d` or `./mac-k3d setup -c ~/.config/mac-k3d/config.yaml` (controller) / `worker.yaml` (worker).
4. Choose the role; let it **Install** Docker if asked.
5. Controller: open `http://localhost:17070`. Worker: paste API token, then `mac-k3d config -c worker.yaml` if needed.

```bash
chmod +x mac-k3d-linux-x86_64
# macOS (unsigned GitHub binary):
#   xattr -d com.apple.quarantine ./mac-k3d-darwin-aarch64
./mac-k3d-linux-x86_64          # TTY → setup wizard (controller or worker)
# optional:
mkdir -p ~/.local/bin
cp mac-k3d-linux-x86_64 ~/.local/bin/mac-k3d
```

Open **Terminal** (do not rely on double-click). **Start here (controller + worker + eval):** [docs/user-guide.md](docs/user-guide.md). Extra walkthrough: [docs/new-machine.md](docs/new-machine.md).

## Install (developers)

```bash
cargo install --path .
```

Or build locally:

```bash
cargo build --release
./target/release/mac-k3d --help
```

## Quick start

```bash
# First-run wizard, then start (controller) or config (worker)
mac-k3d setup

# Power-user split (same as before)
mac-k3d prepare
mac-k3d start
mac-k3d config --show-jenkins
mac-k3d status
mac-k3d teardown
mac-k3d clean --yes
```

### Worker (eval box) only

```bash
mac-k3d setup -c ~/.config/mac-k3d/worker.yaml
# Role: CI worker — Docker, Java, Jenkins agent (Harbor optional)
# Jenkins URL: press Enter for the wizard default, or type http://localhost:17070 for a controller on this PC
mac-k3d prepare --non-interactive -c ~/.config/mac-k3d/worker.yaml
```

## Configuration

Default config path: `~/.config/mac-k3d/config.yaml`

See [docs/configuration.md](docs/configuration.md) for the full schema.

## Documentation

**Users:** [docs/user-guide.md](docs/user-guide.md) (controller + worker + eval). Extra bootstrap: [docs/new-machine.md](docs/new-machine.md). Index: [docs/README.md](docs/README.md).

- [Commands](docs/commands.md)
- [Configuration](docs/configuration.md)
- [iCode inputs](docs/icode-harness-inputs.md)
- [Secrets](docs/secrets.md)
- [Lab runbooks](docs/testing/README.md) (this team; not start-here)
- [Releases / Historical](docs/README.md#releases-what-each-version-shipped) (what each version implemented; v0.3 cargo path is not start-here)

## License

MIT
