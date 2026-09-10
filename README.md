# mac-k3d

A Rust CLI for preparing, starting, configuring, tearing down, and cleaning up a local development / CI eval environment on **macOS and Linux**:

- **Docker** — Docker Desktop (macOS) or Docker Engine (Linux)
- **k3d** — lightweight Kubernetes (k3s in Docker); required for controller/standalone
- **Jenkins** (optional) — CI/CD in-cluster on controllers; agents on workers. Job `lolbench_one_task` supports `EVAL_MODE=binary` (iCode `-full-` tarball) or `source` (git + uv in the job). See [docs/icode-ci-new-machine.md](docs/icode-ci-new-machine.md).

## Requirements

### macOS
- Docker Desktop
- Homebrew (for auto-install of tools)
- [k3d](https://k3d.io/), [kubectl](https://kubernetes.io/docs/tasks/tools/) (controller/standalone)

### Linux (Ubuntu/Debian tested)
- Docker Engine (`docker.io` or docker-ce); user in `docker` group
- `apt` for auto-install (other distros: install tools manually / specify paths)
- systemd --user for Jenkins agent (run `loginctl enable-linger $USER` so agents survive logout)
- k3d / kubectl / helm for controller/standalone roles

Optional (Jenkins controller): [Helm](https://helm.sh/)

Users do **not** need Rust. Developers who build from source do.

## Install (users)

Download the asset for your OS/arch from the GitHub **Releases** page (`mac-k3d-linux-x86_64`, `mac-k3d-linux-aarch64`, `mac-k3d-darwin-x86_64`, or `mac-k3d-darwin-aarch64`):

```bash
chmod +x mac-k3d-linux-x86_64
# macOS (unsigned GitHub binary):
#   xattr -d com.apple.quarantine ./mac-k3d-darwin-aarch64
./mac-k3d-linux-x86_64          # TTY → setup wizard (controller or worker)
# optional:
mkdir -p ~/.local/bin
cp mac-k3d-linux-x86_64 ~/.local/bin/mac-k3d
```

Open **Terminal** (do not rely on double-click). Then follow [docs/initializer-new-machine.md](docs/initializer-new-machine.md).

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
mac-k3d prepare --non-interactive -c ~/.config/mac-k3d/worker.yaml
```

## Configuration

Default config path: `~/.config/mac-k3d/config.yaml`

See [docs/configuration.md](docs/configuration.md) for the full schema.

## Documentation

- [Architecture](docs/architecture.md)
- [Commands](docs/commands.md)
- [Configuration](docs/configuration.md)
- [Deployment](docs/deployment.md)
- [Setup guide](docs/setup.md)
- [Initialize a new Linux or Mac](docs/initializer-new-machine.md)
- [Prepare wizard](docs/prepare-wizard.md)
- [Initializer testing checklist](docs/testing-initializer.md)
- [iCode CI (user)](docs/icode-ci-new-machine.md)
- [iCode CI testing](docs/testing-icode-ci.md)
- [Jenkins job `lolbench_one_task`](docs/lolbench-jenkins.md)
- [Secrets (Jenkins credentials on controller)](docs/secrets.md)

## License

MIT
