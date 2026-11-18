"""Thin OLGP Python client.

The SDK uses the public HTTP contract. It does not reimplement discovery or
pairing logic locally.
"""

from __future__ import annotations

import asyncio
import inspect
import json
import os
import subprocess
import time
import urllib.request
from urllib.error import HTTPError
from dataclasses import dataclass
from typing import Any
from urllib.parse import quote, urlencode, urlsplit, urlunsplit

__all__ = [
    "audio",
    "BoxInfo",
    "Client",
    "connect",
    "discover",
    "pair",
    "RealtimeSession",
    "engine",
    "speakers",
    "timers",
    "memos",
    "phones",
    "audio_memory",
    "knowledge_bases",
    "images",
    "recordings",
]



class RealtimeSession:
    """One voice-v1 WebSocket session."""

    def __init__(self, websocket: Any) -> None:
        self._ws = websocket
        self._events: asyncio.Queue[Any] = asyncio.Queue()
        self._event_callbacks: list[Any] = []
        self._audio_callbacks: list[Any] = []
        self._closed = False
        self._reader = asyncio.create_task(self._read())

    def on_event(self, callback: Any) -> Any:
        self._event_callbacks.append(callback)
        return callback

    def on_audio(self, callback: Any) -> Any:
        self._audio_callbacks.append(callback)
        return callback

    async def events(self) -> Any:
        while True:
            event = await self._events.get()
            if event is None:
                return
            yield event

    async def send_pcm(self, pcm: bytes) -> None:
        await self._send(b"\x01" + bytes(pcm))

    async def send_text(self, text: str) -> None:
        await self._send_json({"type": "text_input", "text": text})

    async def commit(self) -> None:
        await self._send_json({"type": "commit"})

    async def cancel(self) -> None:
        await self._send_json({"type": "cancel"})

    async def reset(self) -> None:
        await self._send_json({"type": "reset"})

    async def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        await self._ws.close()
        if self._reader is not None and self._reader is not asyncio.current_task():
            self._reader.cancel()
            try:
                await self._reader
            except asyncio.CancelledError:
                pass
        self._events.put_nowait(None)

    async def _send(self, frame: Any) -> None:
        if self._closed:
            raise RuntimeError("realtime session is closed")
        await self._ws.send(frame)

    async def _send_json(self, event: dict[str, Any]) -> None:
        await self._send(json.dumps(event))

    async def _read(self) -> None:
        try:
            while True:
                frame = await self._ws.recv()
                if isinstance(frame, bytes):
                    pcm = frame[1:] if frame[:1] == b"\x01" else frame
                    for callback in tuple(self._audio_callbacks):
                        await self._emit(callback, pcm)
                    self._events.put_nowait({"type": "audio", "audio": pcm})
                    continue
                try:
                    event = json.loads(frame)
                except (TypeError, json.JSONDecodeError):
                    continue
                if not isinstance(event, dict):
                    event = {"type": "message", "data": event}
                for callback in tuple(self._event_callbacks):
                    await self._emit(callback, event)
                self._events.put_nowait(event)
        except Exception as exc:
            if not self._closed:
                event = {
                    "type": "error",
                    "code": "connection_closed",
                    "message": str(exc),
                    "retryable": False,
                }
                for callback in tuple(self._event_callbacks):
                    await self._emit(callback, event)
                self._events.put_nowait(event)
        finally:
            self._events.put_nowait(None)

    @staticmethod
    async def _emit(callback: Any, value: Any) -> None:
        result = callback(value)
        if inspect.isawaitable(result):
            await result


class _Realtime:
    """Realtime sessions bound to a paired client."""

    def __init__(self, client: "Client") -> None:
        self._client = client

    async def connect(self, mode: str = "voice") -> RealtimeSession:
        if mode not in ("voice", "chat"):
            raise ValueError("mode must be 'voice' or 'chat'")
        try:
            import websockets
        except ImportError as exc:
            raise RuntimeError(
                "websockets is required for realtime; install it with `pip install websockets`"
            ) from exc
        parts = urlsplit(self._client.base_url)
        scheme = "wss" if parts.scheme in ("https", "wss") else "ws"
        path = parts.path.rstrip("/") + "/v1/realtime"
        query = urlencode({"mode": mode, "token": self._client.token})
        url = urlunsplit((scheme, parts.netloc, path, query, ""))
        return RealtimeSession(await websockets.connect(url))


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

    class Engine(_Module):
        def status(self) -> Any:
            return self._client._request("GET", "/engine/status")

        def capabilities(self) -> Any:
            return self._client._request("GET", "/engine/capabilities")

        class Sessions(_Module):
            def __call__(self) -> Any:
                return self._client._request("GET", "/engine/sessions")

            def create(self, model: str, task: str) -> Any:
                return self._client._request(
                    "POST", "/engine/sessions", {"model": model, "task": task}
                )

        @property
        def sessions(self) -> "Client.Engine.Sessions":
            return self.Sessions(self._client)

    class Images(_Module):
        def frames_process(self, **fields: Any) -> Any:
            return self._client._request("POST", "/v2/images/frames:process", fields)

        class Jobs(_Module):
            _TERMINAL = frozenset(
                {
                    "succeeded",
                    "completed",
                    "complete",
                    "failed",
                    "error",
                    "cancelled",
                    "canceled",
                    "model_missing",
                }
            )

            def create(self, **fields: Any) -> Any:
                return self._client._request("POST", "/v2/images/jobs", fields)

            def retrieve(self, job_id: str) -> Any:
                return self._client._request("GET", f"/v2/images/jobs/{self._id(job_id)}")

            def observe(self, job_id: str) -> Any:
                return self.retrieve(job_id)

            def wait(
                self, job_id: str, *, timeout: float = 60.0, interval: float = 0.25
            ) -> Any:
                deadline = time.monotonic() + timeout
                while True:
                    job = self.retrieve(job_id)
                    if str(job.get("status", "")).lower() in self._TERMINAL:
                        return job
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError(f"image job {job_id} did not finish in {timeout}s")
                    time.sleep(min(interval, remaining))

            def result(self, job_id: str) -> Any:
                path = f"/v2/images/jobs/{self._id(job_id)}/result"
                try:
                    return self._client._request("GET", path)
                except HTTPError as exc:
                    if exc.code == 409:
                        return json.loads(exc.read())
                    raise

            def result_bytes(self, job_id: str) -> tuple[bytes, str]:
                """Return the raw result body and its Content-Type."""
                path = f"/v2/images/jobs/{self._id(job_id)}/result"
                return self._client._raw_request("GET", path)

            def save(self, job_id: str, path: str | os.PathLike[str]) -> None:
                data, _ = self.result_bytes(job_id)
                with open(path, "wb") as stream:
                    stream.write(data)

            def cancel(self, job_id: str) -> Any:
                return self._client._request(
                    "POST", f"/v2/images/jobs/{self._id(job_id)}:cancel"
                )

        @property
        def jobs(self) -> "Client.Images.Jobs":
            return self.Jobs(self._client)

    class Recordings(_Module):
        class Capture(_Module):
            def __init__(self, client: "Client") -> None:
                super().__init__(client)
                self._seq: dict[str, int] = {}

            def start(self, input: dict[str, Any] | None = None, **fields: Any) -> Any:
                payload = dict(input or {})
                payload.update(fields)
                if "retain_audio" in payload:
                    payload["retainAudio"] = payload.pop("retain_audio")
                return self._client._request("POST", "/v1/audio/recordings", payload)

            def append(self, recording_id: str, pcm: str) -> Any:
                seq = self._seq.get(recording_id, 0)
                self._seq[recording_id] = seq + 1
                return self._client._request(
                    "POST",
                    f"/v1/audio/recordings/{self._id(recording_id)}/audio",
                    {"seq": seq, "pcm": pcm},
                )

            def append_resumable(self, recording_id: str, pcm: str) -> Any:
                return self.append(recording_id, pcm)

            def resume(self, recording_id: str) -> Any:
                recording = self._client._request(
                    "GET", f"/v1/audio/recordings/{self._id(recording_id)}"
                )
                if isinstance(recording, dict) and "chunk_count" in recording:
                    self._seq[recording_id] = int(recording["chunk_count"])
                return recording

        def __init__(self, client: "Client") -> None:
            super().__init__(client)
            self._capture = self.Capture(client)

        @property
        def capture(self) -> "Client.Recordings.Capture":
            return self._capture

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

    def __init__(
        self,
        base_url: str,
        token: str,
        memory_token: str | None = None,
        search_provider: str | None = None,
        tavily_key: str | None = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.memory_token = memory_token or os.environ.get("OLGP_MEMORY_TOKEN", "")
        self.search_provider = search_provider or os.environ.get("OLGP_SEARCH_PROVIDER", "")
        self.tavily_key = tavily_key or os.environ.get("TAVILY_API_KEY", "")
        self._recordings = None

    def _request(
        self, method: str, path: str, payload: dict[str, Any] | None = None
    ) -> Any:
        headers = {"Authorization": f"Bearer {self.token}"}
        if self.memory_token and path.startswith("/v1/audio-memory"):
            headers["X-Wici-Memory-Token"] = self.memory_token
        if self.search_provider:
            headers["X-Wici-Search-Provider"] = self.search_provider
        if self.tavily_key:
            headers["X-Wici-Tavily-Key"] = self.tavily_key
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

    def _raw_request(self, method: str, path: str) -> tuple[bytes, str]:
        request = urllib.request.Request(
            f"{self.base_url}{path}",
            headers={"Authorization": f"Bearer {self.token}"},
            method=method,
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            headers = getattr(response, "headers", {})
            content_type = (
                headers.get("Content-Type")
                or headers.get("content-type")
                or "application/octet-stream"
            )
            return response.read(), content_type

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
    def engine(self) -> "Client.Engine":
        return self.Engine(self)

    @property
    def images(self) -> "Client.Images":
        return self.Images(self)

    @property
    def recordings(self) -> "Client.Recordings":
        if self._recordings is None:
            self._recordings = self.Recordings(self)
        return self._recordings

    @property
    def audio(self) -> "_Audio":
        return _Audio(self.recordings)

    @property
    def realtime(self) -> "_Realtime":
        return _Realtime(self)

    def chat(self, model: str, message: str | list[dict[str, Any]]) -> str:
        """Send one chat completion request and return the reply text."""
        messages = (
            message if isinstance(message, list) else [{"role": "user", "content": message}]
        )
        reply = self._post(
            "/v1/chat/completions", {"model": model, "messages": messages}
        )
        return reply["choices"][0]["message"]["content"]


class _Audio:
    def __init__(self, recordings: "Client.Recordings") -> None:
        self.recordings = recordings


class _DefaultClient:
    """Defer connection setup until a module-level call is made."""

    def _request(
        self, method: str, path: str, payload: dict[str, Any] | None = None
    ) -> Any:
        return connect()._request(method, path, payload)

    def _raw_request(self, method: str, path: str) -> tuple[bytes, str]:
        return connect()._raw_request(method, path)


speakers = Client.Speakers(_DefaultClient())
timers = Client.Timers(_DefaultClient())
memos = Client.Memos(_DefaultClient())
phones = Client.Phones(_DefaultClient())
audio_memory = Client.AudioMemory(_DefaultClient())
knowledge_bases = Client.KnowledgeBases(_DefaultClient())
images = Client.Images(_DefaultClient())
engine = Client.Engine(_DefaultClient())
recordings = Client.Recordings(_DefaultClient())
audio = _Audio(recordings)


def connect(
    base_url: str | None = None,
    token: str | None = None,
    memory_token: str | None = None,
    search_provider: str | None = None,
    tavily_key: str | None = None,
) -> Client:
    return Client(
        base_url or os.environ.get("OLGP_BASE_URL", "http://olgp.local:8787"),
        token or os.environ.get("OLGP_DEVICE_TOKEN", ""),
        memory_token or os.environ.get("OLGP_MEMORY_TOKEN"),
        search_provider or os.environ.get("OLGP_SEARCH_PROVIDER"),
        tavily_key or os.environ.get("TAVILY_API_KEY"),
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


