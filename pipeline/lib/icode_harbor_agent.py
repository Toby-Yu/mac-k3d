"""Harbor adapter: official worker iCode drop bind-mounted at /opt/icode-host.

Used for DeepSWE, LoLBench, and SWE-bench Pro. LoLBench's in-repo agent clones
iCode from gitcode inside the sandbox. That repo is not anonymously cloneable,
and this lab does not store a GitCode PAT on Jenkins.
"""

from __future__ import annotations

from pathlib import Path
from typing import override

from harbor.agents.installed.base import BaseInstalledAgent
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

DEEPSEEK_BASE_URL = "https://api.deepseek.com/v1"
DEFAULT_MODEL = "deepseek-v4-pro"
ICODE_HOST = "/opt/icode-host"
CAPTURE_SRC = Path(__file__).with_name("icode_capture.sh")
CAPTURE = "/installed-agent/icode_capture.sh"


async def install_capture(agent: BaseInstalledAgent, environment: BaseEnvironment) -> None:
    """Upload the capture script every agent shares: `base` before work, `capture` after."""
    await environment.upload_file(CAPTURE_SRC, CAPTURE)
    await agent.exec_as_root(environment, command=f"chmod 755 {CAPTURE}")


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
        await install_capture(self, environment)
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

    def run_env(self) -> dict[str, str]:
        """Environment of the iCode run; CanaryAgent probes with the same one."""
        env = dict(self.extra_env)
        env.setdefault("ICODE_API_BASE", DEEPSEEK_BASE_URL)
        base = str(env.get("ICODE_API_BASE") or "").lower()
        env.setdefault(
            "ICODE_PROVIDER",
            "DeepSeek" if "deepseek.com" in base else "OpenAI",
        )
        env.setdefault("ICODE_REASONING_EFFORT", "high")
        env.setdefault("ICODE_MODEL", DEFAULT_MODEL)
        return env

    @override
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        env = self.run_env()

        await self.exec_as_agent(
            environment,
            command=(
                "set -eu; mkdir -p /logs/agent/icode /logs/agent/icode-project; "
                f"cat > /tmp/icode_task.md <<'ICODE_TASK_EOF'\n{instruction}\nICODE_TASK_EOF"
            ),
        )

        # Harbor prefixes this script with `set -o pipefail`. iCode exits 1 when
        # the turn is not ok (including a tool path that is too long). That exit
        # must not become NonZeroAgentExitCodeError; the verifier still grades
        # whatever patch was written. Turn pipefail off, keep iCode's status,
        # and let the shell itself exit 0. Same command for every benchmark;
        # icode_capture.sh hands the patch to each suite's grader. A declared
        # repo that is not a git top-level stops the trial before iCode starts.
        command = (
            'export PATH="$HOME/.local/bin:$PATH"; '
            f"repo=$(bash {CAPTURE} base) || "
            '{ echo "capture: repo check failed (see /logs/agent/capture.json)" >&2; exit 3; }; '
            'cd "$repo"; '
            "set +o pipefail; "
            "icode -p /logs/agent/icode-project "
            "run -t /tmp/icode_task.md "
            '-C "$repo" -a code --json 2>&1 | tee /logs/agent/icode.txt; '
            'printf "%s\\n" "${PIPESTATUS[0]}" > /logs/agent/icode-exit.txt; '
            f"bash {CAPTURE} capture; "
            'printf "%s\\n" "$?" > /logs/agent/capture-exit.txt'
        )
        await self.exec_as_agent(environment, env=env, command=command)
