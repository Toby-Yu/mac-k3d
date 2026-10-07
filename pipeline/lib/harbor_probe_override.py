"""Swap the image Harbor's egress-control kernel probe runs in.

Harbor 0.22 enforces the agent allowlist and `no-network` only when
`DockerEnvironment._egress_control_kernel_support()` returns True. That probe
runs a pinned alpine image; on a host whose Docker store cannot unpack that
image the probe fails and Harbor refuses isolation. `env/egress`
(`harbor_egress.py`) tries Harbor's image first and exports
`MAC_K3D_EGRESS_PROBE_IMAGE` only when Docker cannot run it. The script and
everything the trial enforces stay Harbor's own.

Imported by `icode_harbor_agent.py` before anything else. Harbor's `Trial`
loads the agent before it creates the environment, so this runs before the
first probe.
"""

from __future__ import annotations

import os
import sys

ENV = "MAC_K3D_EGRESS_PROBE_IMAGE"
ATTR = "_EGRESS_CONTROL_KERNEL_PROBE_IMAGE"
PROBE = "_egress_control_kernel_support"


def apply(image: str | None = None) -> bool:
    """Point Harbor's probe at `image` (default: $MAC_K3D_EGRESS_PROBE_IMAGE)."""
    image = (image if image is not None else os.environ.get(ENV, "")).strip()
    if not image:
        return False
    try:
        from harbor.environments.docker.docker import DockerEnvironment
    except ImportError as exc:
        print(f"WARNING: {ENV} set but Harbor's DockerEnvironment is not importable ({exc})", file=sys.stderr)
        return False
    probe = getattr(DockerEnvironment, PROBE, None)
    if not hasattr(DockerEnvironment, ATTR) or probe is None:
        print(
            f"WARNING: {ENV} set but this Harbor has no DockerEnvironment.{ATTR}; probe image left unchanged",
            file=sys.stderr,
        )
        return False
    setattr(DockerEnvironment, ATTR, image)
    cache_clear = getattr(probe, "cache_clear", None)
    if callable(cache_clear):
        cache_clear()
    return True


apply()
