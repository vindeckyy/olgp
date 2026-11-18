"""Thin OLGP Python client.

The SDK uses the public HTTP contract. It does not reimplement discovery or
pairing logic locally.
"""

from __future__ import annotations

import json
import os
import subprocess
import urllib.request
from dataclasses import dataclass
from typing import Any

__all__ = ["BoxInfo", "Client", "connect", "discover", "pair"]


@dataclass(frozen=True)
class BoxInfo:
    box_id: str
    fingerprint: str
    address: str
    gpu: str


def discover(timeout_ms: int = 2000) -> list[BoxInfo]:
    """List boxes answering on the LAN through `olgpctl discover`."""
    output = subprocess.check_output(
        ["olgpctl", "discover"], text=True, timeout=max(1, timeout_ms // 1000)
    )
    boxes = []
    for line in output.splitlines():
        parts = line.split()
        if len(parts) >= 4:
            boxes.append(BoxInfo(parts[0], parts[1], parts[2], parts[3]))
    return boxes


def pair(box_id: str, code: str) -> str:
    """Pair through the CLI and return the credential path."""
    subprocess.run(["olgpctl", "pair", box_id, "--code", code], check=True)
    home = os.environ.get("HOME", "/tmp")
    return f"{home}/.config/olgp/boxes/{box_id}.json"


class Client:
    """HTTP client bound to one paired box."""

    def __init__(self, base_url: str, token: str) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token

    def _post(self, path: str, payload: dict[str, Any]) -> dict[str, Any]:
        request = urllib.request.Request(
            f"{self.base_url}{path}",
            data=json.dumps(payload).encode(),
            headers={
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
            },
            method="POST",
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.loads(response.read())

    def chat(self, model: str, message: str) -> str:
        """Send one chat completion request and return the reply text."""
        reply = self._post(
            "/v1/chat/completions",
            {"model": model, "messages": [{"role": "user", "content": message}]},
        )
        return reply["choices"][0]["message"]["content"]


def connect(base_url: str | None = None, token: str | None = None) -> Client:
    return Client(
        base_url or os.environ.get("OLGP_BASE_URL", "http://olgp.local:8787"),
        token or os.environ.get("OLGP_DEVICE_TOKEN", ""),
    )
