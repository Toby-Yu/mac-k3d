"""Harbor isolation canary (report P0.6): iCode's sandbox, probed instead of used.

P5 runs it with the same flags, mounts and environment as ICodeAgent, plus
`--ak spec=<canary_verdict.py spec output>` and `--disable-verification`. The
install is ICodeAgent's own; run() never starts iCode and never calls the model.
canary_probe.sh writes the facts to /logs/agent, and canary_verdict.py on the
host decides pass or fail.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path
from typing import override

from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

from canary_verdict import render_spec
from icode_harbor_agent import ICodeAgent

PROBE_SRC = Path(__file__).with_name("canary_probe.sh")
PROBE = "/installed-agent/canary_probe.sh"
SPEC = "/installed-agent/canary_spec.txt"
HOST_SCHEMA = "mac-k3d-canary-host-v1"
TOOL_LIST_REASON = (
    "iCode PR 2 has no command that prints its tool set (only `agents` and `models`); "
    "the P0.5 transcript scan covers web tools"
)


class CanaryAgent(ICodeAgent):
    """Record what the agent user can reach, read and write; then exit."""

    def __init__(self, *args, spec: str | None = None, **kwargs):
        super().__init__(*args, **kwargs)
        self._spec_path = Path(spec) if spec else None

    @staticmethod
    @override
    def name() -> str:
        return "canary"

    def _spec(self) -> dict:
        if self._spec_path is None or not self._spec_path.is_file():
            raise RuntimeError("CanaryAgent needs --ak spec=<file from canary_verdict.py spec>")
        return json.loads(self._spec_path.read_text(encoding="utf-8"))

    @override
    async def install(self, environment: BaseEnvironment) -> None:
        spec = self._spec()
        await environment.upload_file(PROBE_SRC, PROBE)
        with tempfile.TemporaryDirectory(prefix="mac-k3d-canary-") as tmp:
            local = Path(tmp) / "canary_spec.txt"
            local.write_text(render_spec(spec), encoding="utf-8")
            await environment.upload_file(local, SPEC)
        await self.exec_as_root(environment, command=f"chmod 755 {PROBE} && chmod 644 {SPEC}")
        # Before iCode's install touches $HOME: anything here came with the image.
        await self.exec_as_agent(environment, command=f"bash {PROBE} home pre", env=self.extra_env)
        await super().install(environment)

    @override
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        spec = self._spec()
        await self.exec_as_agent(
            environment,
            env=self.run_env(),
            command=(
                'export PATH="$HOME/.local/bin:$PATH"; '
                f"bash {PROBE} probe; "
                f"bash {PROBE} home post"
            ),
        )
        await self.exec_as_root(environment, command=f"bash {PROBE} mount")
        host = {
            "schema": HOST_SCHEMA,
            "agent": self.name(),
            "spec_version": spec.get("version", ""),
            "task": spec.get("task", ""),
            "allow_host": spec.get("allow_host"),
            "task_mcp_servers": len(self.mcp_servers),
            "tool_list": {"status": "unavailable", "reason": TOOL_LIST_REASON},
        }
        self.logs_dir.mkdir(parents=True, exist_ok=True)
        (self.logs_dir / "canary_host.json").write_text(json.dumps(host, indent=2) + "\n", encoding="utf-8")
