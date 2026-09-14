"""Pier 0.3.1 adapter: install/run eval/pier-agent-icode scripts inside the sandbox."""

from __future__ import annotations

import json
import shlex
from pathlib import Path
from typing import Any

from pier.agents.installed.base import BaseInstalledAgent, with_prompt_template
from pier.environments.base import BaseEnvironment
from pier.models.agent.context import AgentContext
from pier.models.agent.install import AgentInstallSpec, InstallStep
from pier.models.agent.network import NetworkAllowlist

PKG = Path(__file__).resolve().parent / "pier-agent-icode"
SANDBOX_AGENT = "/opt/icode-agent"
SANDBOX_ICODE = "/opt/icode-host"
INSTRUCTION_FILE = "/tmp/instruction.md"


class ICodeAgent(BaseInstalledAgent):
    """Installed agent that copies Harbor-style install.sh/run.sh into the trial env."""

    @staticmethod
    def name() -> str:
        return "icode"

    def version(self) -> str | None:
        return self._version or "0.1.0"

    def network_allowlist(self) -> NetworkAllowlist:
        return NetworkAllowlist(domains=["api.deepseek.com", "api.deepseek.ai"])

    def install_spec(self) -> AgentInstallSpec:
        return AgentInstallSpec(
            agent_name=self.name(),
            version=self.version(),
            steps=[
                InstallStep(
                    user="root",
                    run=f"mkdir -p {SANDBOX_AGENT} {SANDBOX_ICODE}",
                )
            ],
        )

    async def setup(self, environment: BaseEnvironment) -> None:
        await super().setup(environment)
        install_sh = PKG / "install.sh"
        run_sh = PKG / "run.sh"
        if not install_sh.is_file() or not run_sh.is_file():
            raise RuntimeError(f"missing pier-agent-icode scripts under {PKG}")
        await environment.upload_file(install_sh, f"{SANDBOX_AGENT}/install.sh")
        await environment.upload_file(run_sh, f"{SANDBOX_AGENT}/run.sh")
        env = {
            "ICODE_SOURCE_HOST": self._get_env("ICODE_SOURCE_HOST") or SANDBOX_ICODE,
        }
        bin_host = self._get_env("ICODE_BIN_HOST")
        if bin_host:
            env["ICODE_BIN_HOST"] = bin_host
        release = self._get_env("ICODE_RELEASE_URL")
        if release:
            env["ICODE_RELEASE_URL"] = release
        await self.exec_as_root(
            environment,
            f"chmod +x {SANDBOX_AGENT}/install.sh {SANDBOX_AGENT}/run.sh && {SANDBOX_AGENT}/install.sh",
            env=env,
        )

    @with_prompt_template
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        quoted = shlex.quote(instruction)
        await environment.exec(
            command=f"printf '%s\\n' {quoted} > {INSTRUCTION_FILE}",
            user="root",
        )
        env = {
            "INSTRUCTION_FILE": INSTRUCTION_FILE,
            "DEEPSEEK_API_KEY": self._get_env("DEEPSEEK_API_KEY") or "",
            "DEEPSEEK_MODEL": self.model_name
            or self._get_env("DEEPSEEK_MODEL")
            or "deepseek-v4-pro",
            "ICODE_BIN": self._get_env("ICODE_BIN") or "icode",
            "AGENT_OUTPUT_DIR": "/logs/agent",
        }
        await self.exec_as_agent(
            environment,
            command=f"{SANDBOX_AGENT}/run.sh",
            env=env,
        )

    def populate_context_post_run(self, context: AgentContext) -> None:
        blob: dict[str, Any] = {}
        for path in self.logs_dir.rglob("*"):
            if not path.is_file() or path.suffix not in {".json", ".jsonl"}:
                continue
            if path.stat().st_size > 2_000_000:
                continue
            try:
                data = json.loads(path.read_text(encoding="utf-8", errors="ignore"))
            except (OSError, json.JSONDecodeError):
                continue
            if isinstance(data, dict):
                blob.update(data)
                usage = data.get("token_usage")
                if isinstance(usage, dict):
                    blob.setdefault("prompt_tokens", usage.get("prompt"))
                    blob.setdefault("completion_tokens", usage.get("completion"))
        prompt = _as_int(blob.get("prompt_tokens") or blob.get("prompt"))
        completion = _as_int(blob.get("completion_tokens") or blob.get("completion"))
        if prompt is not None:
            context.n_input_tokens = prompt
        if completion is not None:
            context.n_output_tokens = completion
        meta = context.metadata if isinstance(context.metadata, dict) else {}
        if blob.get("llm_model_id") or blob.get("exit_code") is not None:
            meta = {
                **meta,
                "icode_llm_model_id": blob.get("llm_model_id"),
                "icode_exit_code": blob.get("exit_code"),
            }
            context.metadata = meta


def _as_int(value: Any) -> int | None:
    if value is None or isinstance(value, bool):
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None
