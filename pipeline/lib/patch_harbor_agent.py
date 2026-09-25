"""Harbor agent that applies a pre-written patch and exits.

The LLM call already happened on the host. Harbor's verifier then grades the
tree the same way it grades an iCode attempt.
"""

from __future__ import annotations

from typing import override

from harbor.agents.installed.base import BaseInstalledAgent
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

PATCH_HOST = "/opt/baseline-patch/agent.patch"


class PatchAgent(BaseInstalledAgent):
    """Copy a mounted unified diff into the task repo."""

    @staticmethod
    @override
    def name() -> str:
        return "patch"

    @override
    def get_version_command(self) -> str | None:
        return "git --version"

    @override
    async def install(self, environment: BaseEnvironment) -> None:
        await self.ensure_system_dependencies(environment, ("git",))

    @override
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        env = dict(self.extra_env)
        submit = ""
        if str(env.get("MAC_K3D_BENCHMARK") or "").strip().lower() == "lolbench":
            submit = 'lolbench-submit "$repo" || true; '
        await self.exec_as_agent(
            environment,
            env=env,
            command=(
                "set -eu; "
                "mkdir -p /logs/agent /logs/artifacts; "
                f'if [ ! -s {PATCH_HOST} ]; then echo "empty patch" | tee /logs/agent/notes.txt; exit 0; fi; '
                f"cp -f {PATCH_HOST} /logs/agent/agent.patch; "
                f"cp -f {PATCH_HOST} /logs/artifacts/model.patch; "
                'repo=""; '
                "for root in /workspace /app; do "
                '  [ -d "$root" ] || continue; '
                '  hit=$(find "$root" -maxdepth 3 -type d -name .git 2>/dev/null | head -1 || true); '
                '  if [ -n "$hit" ]; then repo=$(dirname "$hit"); break; fi; '
                "done; "
                'if [ -z "$repo" ]; then if [ -d /app ]; then repo=/app; else repo=/workspace; fi; fi; '
                'cd "$repo"; '
                f"git apply --verbose {PATCH_HOST} || echo 'git apply failed' | tee /logs/agent/notes.txt; "
                f"{submit}"
            ),
        )
