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
from urllib.parse import quote

__all__ = [
    "BoxInfo",
    "Client",
    "connect",
    "discover",
    "pair",
    "speakers",
    "timers",
    "memos",
    "phones",
    "audio_memory",
    "knowledge_bases",
    "images",
    "recordings",
]



class _Module:
    def __init__(self, client: "Client") -> None:
        self._client = client

    @staticmethod
    def _id(value: Any) -> str:
        return quote(str(value), safe="")

class Client:
    """HTTP client bound to one paired box."""

    _Module = _Module

    class Speakers(_Module):
        def list(self) -> Any:
            return self._client._request("GET", "/v1/speakers")

        def retrieve(self, speaker_id: str) -> Any:
            return self._client._request("GET", f"/v1/speakers/{self._id(speaker_id)}")

        def update(self, speaker_id: str, **changes: Any) -> Any:
            return self._client._request("PATCH", f"/v1/speakers/{self._id(speaker_id)}", changes)

        def play(self, speaker_id: str, url: str = "") -> Any:
            return self._client._request("POST", f"/v1/speakers/{self._id(speaker_id)}:play", {"url": url})

        def stop(self, speaker_id: str) -> Any:
            return self._client._request("POST", f"/v1/speakers/{self._id(speaker_id)}:stop")

        def bluetooth(self, speaker_id: str) -> Any:
            return self._client._request("GET", f"/v1/speakers/{self._id(speaker_id)}/bluetooth")

        def startBluetoothPairing(self, speaker_id: str) -> Any:
            return self._client._request(
                "POST", f"/v1/speakers/{self._id(speaker_id)}:startBluetoothPairing"
            )

    class Timers(_Module):
        def list(self) -> Any:
            return self._client._request("GET", "/v1/timers")

        def create(self, **fields: Any) -> Any:
            return self._client._request("POST", "/v1/timers", fields)

        def cancel(self, timer_id: str) -> Any:
            return self._client._request("POST", f"/v1/timers/{self._id(timer_id)}:cancel")

        def snooze(self, timer_id: str, delay_sec: int = 60) -> Any:
            return self._client._request(
                "POST", f"/v1/timers/{self._id(timer_id)}:snooze", {"delaySec": delay_sec}
            )

    class Memos(_Module):
        def list(self) -> Any:
            return self._client._request("GET", "/v1/memos")

        def create(self, **fields: Any) -> Any:
            return self._client._request("POST", "/v1/memos", fields)

        def dismiss(self, memo_id: str) -> Any:
            return self._client._request("POST", f"/v1/memos/{self._id(memo_id)}:dismiss")

    class Phones(_Module):
        def ring(self, phone_id: str, duration_sec: int = 5) -> Any:
            return self._client._request(
                "POST", f"/v1/phones/{self._id(phone_id)}:ring", {"durationSec": duration_sec}
            )

    class AudioMemory(_Module):
        def status(self) -> Any:
            return self._client._request("GET", "/v1/audio-memory/status")

        def ready(self) -> Any:
            return self._client._request("GET", "/v1/audio-memory/health/ready")

        def settings(self, **changes: Any) -> Any:
            path = "/v1/audio-memory/settings"
            return self._client._request("PATCH" if changes else "GET", path, changes or None)

        def episodes(self) -> Any:
            return self._client._request("GET", "/v1/audio-memory/episodes")

        def notes(self) -> Any:
            return self._client._request("GET", "/v1/audio-memory/notes")

        def add_note(self, text: str, kind: str = "fact") -> Any:
            return self._client._request(
                "POST", "/v1/audio-memory/notes", {"text": text, "kind": kind}
            )

        def query(self, question: str) -> Any:
            return self._client._request("POST", "/v1/audio-memory:query", {"question": question})

        def forget(self, note_id: str) -> Any:
            return self._client._request("POST", "/v1/audio-memory:forget", {"id": note_id})

        def clear(self) -> Any:
            return self._client._request("POST", "/v1/audio-memory:clear")

    class KnowledgeBases(_Module):
        def create(self, name: str = "Knowledge base") -> Any:
            return self._client._request("POST", "/v1/knowledge-bases", {"name": name})

        def retrieve(self, knowledge_base_id: str) -> Any:
            return self._client._request("GET", f"/v1/knowledge-bases/{self._id(knowledge_base_id)}")

        def delete(self, knowledge_base_id: str) -> Any:
            return self._client._request("DELETE", f"/v1/knowledge-bases/{self._id(knowledge_base_id)}")

        def documents(self, knowledge_base_id: str) -> Any:
            return self._client._request(
                "GET", f"/v1/knowledge-bases/{self._id(knowledge_base_id)}/documents"
            )

        def add_text(self, knowledge_base_id: str, filename: str, text: str) -> Any:
            return self._client._request(
                "POST",
                f"/v1/knowledge-bases/{self._id(knowledge_base_id)}/documents:text",
                {"filename": filename, "text": text},
            )

        def add_file(self, knowledge_base_id: str, filename: str, content: str, mime: str = "application/octet-stream") -> Any:
            return self._client._request(
                "POST",
                f"/v1/knowledge-bases/{self._id(knowledge_base_id)}/documents:file",
                {"filename": filename, "content": content, "mime": mime},
            )

        def add_image(self, knowledge_base_id: str, filename: str, value: str) -> Any:
            return self._client._request(
                "POST",
                f"/v1/knowledge-bases/{self._id(knowledge_base_id)}/documents:image",
                {"filename": filename, "value": value},
            )

        def query(self, knowledge_base_id: str, question: str, top_k: int = 5) -> Any:
            return self._client._request(
                "POST",
                f"/v1/knowledge-bases/{self._id(knowledge_base_id)}:query",
                {"query": question, "topK": top_k},
            )

    class Images(_Module):
        class Jobs(_Module):
            def create(self, **fields: Any) -> Any:
                return self._client._request("POST", "/v2/images/jobs", fields)

            def result(self, job_id: str) -> Any:
                return self._client._request(
                    "GET", f"/v2/images/jobs/{self._id(job_id)}/result"
                )

            def cancel(self, job_id: str) -> Any:
                return self._client._request(
                    "POST", f"/v2/images/jobs/{self._id(job_id)}:cancel"
                )

        @property
        def jobs(self) -> "Client.Images.Jobs":
            return self.Jobs(self._client)

    class Recordings(_Module):
        def create(self, retain_audio: bool = False) -> Any:
            return self._client._request(
                "POST", "/v1/audio/recordings", {"retainAudio": retain_audio}
            )

        def list(self) -> Any:
            return self._client._request("GET", "/v1/audio/recordings")

        def get(self, recording_id: str) -> Any:
            return self._client._request(
                "GET", f"/v1/audio/recordings/{self._id(recording_id)}"
            )

        def delete(self, recording_id: str) -> Any:
            return self._client._request(
                "DELETE", f"/v1/audio/recordings/{self._id(recording_id)}"
            )

        def append(self, recording_id: str, seq: int, pcm: str) -> Any:
            return self._client._request(
                "POST",
                f"/v1/audio/recordings/{self._id(recording_id)}/audio",
                {"seq": seq, "pcm": pcm},
            )

        def finish(self, recording_id: str) -> Any:
            return self._client._request(
                "POST", f"/v1/audio/recordings/{self._id(recording_id)}:finish"
            )

        def reprocess(self, recording_id: str) -> Any:
            return self._client._request(
                "POST", f"/v1/audio/recordings/{self._id(recording_id)}:reprocess"
            )

    def __init__(self, base_url: str, token: str, memory_token: str | None = None) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.memory_token = memory_token or os.environ.get("OLGP_MEMORY_TOKEN", "")

    def _request(
        self, method: str, path: str, payload: dict[str, Any] | None = None
    ) -> Any:
        headers = {"Authorization": f"Bearer {self.token}"}
        if self.memory_token and path.startswith("/v1/audio-memory"):
            headers["X-Wici-Memory-Token"] = self.memory_token
        if method in ("POST", "PATCH"):
            headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode() if payload is not None else None
        if data is None and method in ("POST", "PATCH"):
            data = b"{}"
        request = urllib.request.Request(
            f"{self.base_url}{path}", data=data, headers=headers, method=method
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.loads(response.read())

    def _post(self, path: str, payload: dict[str, Any]) -> Any:
        return self._request("POST", path, payload)

    @property
    def speakers(self) -> "Client.Speakers":
        return self.Speakers(self)

    @property
    def timers(self) -> "Client.Timers":
        return self.Timers(self)

    @property
    def memos(self) -> "Client.Memos":
        return self.Memos(self)

    @property
    def phones(self) -> "Client.Phones":
        return self.Phones(self)

    @property
    def audio_memory(self) -> "Client.AudioMemory":
        return self.AudioMemory(self)

    @property
    def knowledge_bases(self) -> "Client.KnowledgeBases":
        return self.KnowledgeBases(self)

    @property
    def images(self) -> "Client.Images":
        return self.Images(self)

    @property
    def recordings(self) -> "Client.Recordings":
        return self.Recordings(self)

    def chat(self, model: str, message: str | list[dict[str, Any]]) -> str:
        """Send one chat completion request and return the reply text."""
        messages = (
            message if isinstance(message, list) else [{"role": "user", "content": message}]
        )
        reply = self._post(
            "/v1/chat/completions", {"model": model, "messages": messages}
        )
        return reply["choices"][0]["message"]["content"]


class _DefaultClient:
    """Defer connection setup until a module-level call is made."""

    def _request(
        self, method: str, path: str, payload: dict[str, Any] | None = None
    ) -> Any:
        return connect()._request(method, path, payload)


speakers = Client.Speakers(_DefaultClient())
timers = Client.Timers(_DefaultClient())
memos = Client.Memos(_DefaultClient())
phones = Client.Phones(_DefaultClient())
audio_memory = Client.AudioMemory(_DefaultClient())
knowledge_bases = Client.KnowledgeBases(_DefaultClient())
images = Client.Images(_DefaultClient())
recordings = Client.Recordings(_DefaultClient())


def connect(
    base_url: str | None = None,
    token: str | None = None,
    memory_token: str | None = None,
) -> Client:
    return Client(
        base_url or os.environ.get("OLGP_BASE_URL", "http://olgp.local:8787"),
        token or os.environ.get("OLGP_DEVICE_TOKEN", ""),
        memory_token or os.environ.get("OLGP_MEMORY_TOKEN"),
    )


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


