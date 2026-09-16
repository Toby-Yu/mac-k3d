"""Harbor adapter: official worker iCode drop bind-mounted at /opt/icode-host.

LoLBench's in-repo agents/icode_agent.py clones iCode from gitcode inside the
sandbox. That repo is not anonymously cloneable, and this lab does not store a
GitCode PAT on Jenkins. Use the same *-full-* binary DeepSWE/Pier already uses.
"""

from __future__ import annotations

from typing import override

from harbor.agents.installed.base import BaseInstalledAgent
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

DEEPSEEK_BASE_URL = "https://api.deepseek.com"
DEFAULT_MODEL = "deepseek-v4-pro"
ICODE_HOST = "/opt/icode-host"


class ICodeAgent(BaseInstalledAgent):
    """Run the official iCode binary inside a LoLBench Harbor task."""

    @staticmethod
    @override
    def name() -> str:
        return "icode"

    @override
    def get_version_command(self) -> str | None:
        return "icode --help >/dev/null 2>&1 && echo icode-installed"

    @override
    async def install(self, environment: BaseEnvironment) -> None:
        await self.ensure_system_dependencies(environment, ("git", "curl"))
        await self.exec_as_agent(
            environment,
            command=(
                "set -euo pipefail; "
                f"ls -la {ICODE_HOST} || true; "
                "bin=''; "
                f"if [ -x {ICODE_HOST}/icode ]; then bin={ICODE_HOST}/icode; "
                f"else bin=$(find {ICODE_HOST} -type f -name icode -print -quit); fi; "
                '[ -n "$bin" ] || { echo missing icode binary in /opt/icode-host bind mount; exit 1; }; '
                'mkdir -p "$HOME/.local/bin"; '
                'cp -f "$bin" "$HOME/.local/bin/icode"; '
                'chmod +x "$HOME/.local/bin/icode"; '
                'export PATH="$HOME/.local/bin:$PATH"; '
                "icode --help >/dev/null"
            ),
            env=self.extra_env,
        )

    @override
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        env = dict(self.extra_env)
        env.setdefault("ICODE_API_BASE", DEEPSEEK_BASE_URL)
        env.setdefault("ICODE_PROVIDER", "DeepSeek")
        env.setdefault("ICODE_MODEL", DEFAULT_MODEL)

        await self.exec_as_agent(
            environment,
            command=(
                "set -eu; mkdir -p /logs/agent/icode /logs/agent/icode-project; "
                f"cat > /tmp/lolbench_task.md <<'LOLBENCH_TASK_EOF'\n{instruction}\nLOLBENCH_TASK_EOF"
            ),
        )

        await self.exec_as_agent(
            environment,
            env=env,
            command=(
                'export PATH="$HOME/.local/bin:$PATH"; '
                'repo=$(dirname "$(find /workspace -maxdepth 2 -type d -name .git 2>/dev/null | head -1)"); '
                '[ -n "$repo" ] && [ "$repo" != "." ] || repo=/workspace; cd "$repo"; '
                "icode -p /logs/agent/icode-project "
                "run -t /tmp/lolbench_task.md "
                '-C "$repo" -a code --json 2>&1 | tee /logs/agent/icode.txt; '
                'lolbench-submit "$repo" || true'
            ),
        )
