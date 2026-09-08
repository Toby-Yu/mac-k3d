# mac-k3d

A Rust CLI for preparing, starting, configuring, tearing down, and cleaning up a local development / LoLBench eval environment on **macOS and Linux**:

- **Docker** — Docker Desktop (macOS) or Docker Engine (Linux)
- **k3d** — lightweight Kubernetes (k3s in Docker); required for controller/standalone
- **Jenkins** (optional) — CI/CD in-cluster on controllers; agents on workers
- **Harbor / LoLBench** — host-side eval tooling on workers

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

## Install

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
# Verify prerequisites and write default config
mac-k3d prepare --init-config

# Interactive initializer (Mac or Linux)
mac-k3d prepare

# Start cluster (optionally with Jenkins) — controller / standalone
mac-k3d start
mac-k3d start --jenkins in-cluster

# Merge kubeconfig and show service URLs
mac-k3d config --show-jenkins

# Check status
mac-k3d status

# Stop without deleting data
mac-k3d teardown

# Remove cluster and artifacts
mac-k3d clean --yes
```

### Worker (eval box) only

```bash
mac-k3d prepare -i -c ~/.config/mac-k3d/worker.yaml
# Role: CI worker — Docker, Harbor, Java, Jenkins agent
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
- [Prepare wizard](docs/prepare-wizard.md)
- [Initializer testing checklist](docs/testing-initializer.md)
- [LoLBench Jenkins job (`lolbench_one_task`)](docs/lolbench-jenkins.md)
- [Secrets (Jenkins credentials on controller)](docs/secrets.md)

## License

MIT
