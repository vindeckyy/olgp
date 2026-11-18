"""Streaming ASR adapter for the voice-v1 path.

The default backend is deterministic for tests. When sherpa-onnx is installed,
`sherpa_backend()` creates a backend that forwards PCM chunks to a configured
streaming recognizer.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Iterable


@dataclass(frozen=True)
class AsrEvent:
    kind: str
    text: str
    confidence: float


class EchoBackend:
    """Deterministic test backend that emits one partial per chunk."""

    def transcribe(self, chunk: bytes) -> str:
        count = sum(chunk[:32]) / 32 if chunk else 0
        return f"chunk-{int(count):02x}"


class StreamingAsr:
    """Turn PCM chunks into partial transcript events at a fixed cadence."""

    def __init__(self, backend: EchoBackend, chunk_ms: int = 500) -> None:
        self.backend = backend
        self.chunk_ms = chunk_ms
        self.transcript: list[str] = []

    def push(self, chunk: bytes) -> AsrEvent:
        text = self.backend.transcribe(chunk)
        self.transcript.append(text)
        return AsrEvent("stt_partial", " ".join(self.transcript), 0.95)

    def finish(self) -> AsrEvent:
        text = " ".join(self.transcript)
        return AsrEvent("stt_final", text, 0.95)


def sherpa_backend_factory() -> Callable[[bytes], str] | None:
    """Return a sherpa-onnx transcribe callable when the package is installed."""
    try:
        import sherpa_onnx  # type: ignore
    except ImportError:
        return None

    def transcribe(chunk: bytes) -> str:
        recognizer = sherpa_onnx.OfflineRecognizer.from_whisper(
            encoder="", decoder="", tokens="", num_threads=1
        )
        stream = recognizer.create_stream()
        stream.accept_waveform(16000, list(chunk))
        recognizer.decode_stream(stream)
        return str(stream.result.text)

    return transcribe


def chunk_pcm(pcm: bytes, bytes_per_chunk: int) -> Iterable[bytes]:
    """Split PCM into fixed-size chunks."""
    for start in range(0, len(pcm), bytes_per_chunk):
        yield pcm[start : start + bytes_per_chunk]
