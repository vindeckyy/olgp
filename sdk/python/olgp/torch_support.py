"""Small PyTorch-style device helper for OLGP.

This does not monkey-patch PyTorch. It provides the familiar
`device = olgp.gpu("olgp:0", model=...)` shape and forwards inference through
the public HTTP contract.
"""

from __future__ import annotations

from dataclasses import dataclass

from . import Client, connect


@dataclass(frozen=True)
class OlgpDevice:
    client: Client
    model: str
    name: str = "olgp:0"

    def __str__(self) -> str:
        return self.name

    def generate(self, prompt: str) -> str:
        return self.client.chat(
            self.model,
            [{"role": "user", "content": prompt}],
        )


def gpu(name: str = "olgp:0", model: str = "olgp-model", client: Client | None = None) -> OlgpDevice:
    """Return a device-like handle bound to a paired box."""
    return OlgpDevice(client=client or connect(), model=model, name=name)
