"""Harbor adapter: official worker iCode drop bind-mounted at /opt/icode-host.

Used for DeepSWE, LoLBench, and SWE-bench Pro. LoLBench's in-repo agent clones
iCode from gitcode inside the sandbox. That repo is not anonymously cloneable,
and this lab does not store a GitCode PAT on Jenkins.
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
    """Run the official iCode binary inside a Harbor task."""

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
                f"cat > /tmp/icode_task.md <<'ICODE_TASK_EOF'\n{instruction}\nICODE_TASK_EOF"
            ),
        )

        submit = ""
        if str(env.get("MAC_K3D_BENCHMARK") or "").strip().lower() == "lolbench":
            submit = 'lolbench-submit "$repo" || true; '

        await self.exec_as_agent(
            environment,
            env=env,
            command=(
                'export PATH="$HOME/.local/bin:$PATH"; '
                'repo=""; '
                'for root in /workspace /app; do '
                '  [ -d "$root" ] || continue; '
                '  hit=$(find "$root" -maxdepth 3 -type d -name .git 2>/dev/null | head -1 || true); '
                '  if [ -n "$hit" ]; then repo=$(dirname "$hit"); break; fi; '
                "done; "
                'if [ -z "$repo" ]; then if [ -d /app ]; then repo=/app; else repo=/workspace; fi; fi; '
                'cd "$repo"; '
                "icode -p /logs/agent/icode-project "
                "run -t /tmp/icode_task.md "
                '-C "$repo" -a code --json 2>&1 | tee /logs/agent/icode.txt; '
                f"{submit}"
            ),
        )
