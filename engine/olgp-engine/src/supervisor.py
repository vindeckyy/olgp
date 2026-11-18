"""Supervise the loopback inference engine used by OLGP.

This module never binds to a LAN address. The node gateway is the only public
surface.
"""

from __future__ import annotations

import os
import pathlib
import shlex
import tomllib
from typing import Any

import blake3

ALIASES = {
    "qwen25-0p5": "small",
    "qwen25-1p5": "medium",
    "qwen15-moe-a2p7": "moe",
    "turbo": "turbo",
}


def load_profiles(path: pathlib.Path) -> dict[str, dict[str, Any]]:
    """Load the model profile table."""
    with path.open("rb") as handle:
        return tomllib.load(handle)


def resolve_model(
    alias: str,
    model_dir: pathlib.Path,
    profiles: dict[str, dict[str, Any]],
) -> pathlib.Path:
    """Resolve a public alias to a local GGUF file."""
    if alias == "turbo":
        configured = os.environ.get("OLGP_TURBO_GGUF")
        if not configured:
            raise ValueError("OLGP_TURBO_GGUF is required for the turbo profile")
        path = pathlib.Path(configured)
        if not path.is_file():
            raise FileNotFoundError(f"turbo GGUF not found: {path}")
        return path

    profile_name = ALIASES.get(alias)
    if profile_name is None:
        raise ValueError(f"unknown model alias: {alias}")
    profile = profiles.get(profile_name)
    if profile is None:
        raise ValueError(f"missing profile: {profile_name}")
    matches = sorted(model_dir.glob(profile["filename_glob"]))
    if not matches:
        raise FileNotFoundError(f"no GGUF matching {profile['filename_glob']} in {model_dir}")
    return matches[0]


def build_command(
    alias: str,
    model_path: pathlib.Path,
    port: int,
    profiles: dict[str, dict[str, Any]],
) -> list[str]:
    """Build the exact loopback server command for an alias."""
    profile_name = ALIASES.get(alias)
    if profile_name is None:
        raise ValueError(f"unknown model alias: {alias}")
    profile = profiles[profile_name]
    if profile["engine"] == "llama.cpp":
        return [
            "llama-server",
            "-m", str(model_path),
            "-ngl", "99",
            "--ctx-size", str(profile["context"]),
            "-b", str(profile["batch"]),
            "--port", str(port),
            "--host", "127.0.0.1",
        ]
    if profile["engine"] == "inferssd":
        return ["inferssd", "serve", "-m", str(model_path), "--port", str(port)]
    raise ValueError(f"unsupported engine: {profile['engine']}")


def content_hash(path: pathlib.Path) -> str:
    """Return the BLAKE3 hash of a local weight file."""
    digest = blake3.blake3()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_hash(path: pathlib.Path) -> None:
    """Verify a weight file against its sidecar hash file."""
    sidecar = pathlib.Path(f"{path}.blake3")
    expected = sidecar.read_text().strip() if sidecar.exists() else ""
    if not expected:
        raise FileNotFoundError(f"missing hash sidecar: {sidecar}")
    actual = content_hash(path)
    if actual != expected:
        raise ValueError(f"content hash mismatch for {path}")


def command_preview(parts: list[str]) -> str:
    """Render a command for logs without losing quoting."""
    return " ".join(shlex.quote(part) for part in parts)
