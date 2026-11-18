"""Streaming ASR adapter for the voice-v1 path.

The default backend is deterministic for tests. When sherpa-onnx and a Whisper
model are configured, `sherpa_backend()` creates a real backend. Configuration
comes from the local environment; no model is downloaded or bundled.
"""

from __future__ import annotations

import os
import pathlib
import struct
from dataclasses import dataclass
from typing import Callable, Iterable


@dataclass(frozen=True)
class AsrEvent:
    kind: str
    text: str
    confidence: float


@dataclass(frozen=True)
class SherpaConfig:
    encoder: str
    decoder: str
    tokens: str
    sample_rate: int = 16000
    threads: int = 1

    @classmethod
    def from_env(cls) -> "SherpaConfig":
        return cls(
            encoder=os.environ.get("OLGP_ASR_ENCODER", ""),
            decoder=os.environ.get("OLGP_ASR_DECODER", ""),
            tokens=os.environ.get("OLGP_ASR_TOKENS", ""),
            sample_rate=int(os.environ.get("OLGP_ASR_SAMPLE_RATE", "16000")),
            threads=int(os.environ.get("OLGP_ASR_THREADS", "1")),
        )

    def ready(self) -> bool:
        return all(
            path and pathlib.Path(path).is_file()
            for path in (self.encoder, self.decoder, self.tokens)
        )


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


def pcm_samples(chunk: bytes) -> list[int]:
    """Decode little-endian S16LE bytes, padding an odd trailing byte."""
    payload = chunk + b"\x00" if len(chunk) % 2 else chunk
    return list(struct.unpack(f"<{len(payload) // 2}h", payload))


def sherpa_backend_factory() -> Callable[[bytes], str] | None:
    """Return a real sherpa-onnx transcribe callable when configured."""
    try:
        import sherpa_onnx  # type: ignore
    except ImportError:
        return None

    config = SherpaConfig.from_env()
    if not config.ready():
        return None

    recognizer = sherpa_onnx.OfflineRecognizer.from_whisper(
        encoder=config.encoder,
        decoder=config.decoder,
        tokens=config.tokens,
        num_threads=config.threads,
    )

    def transcribe(chunk: bytes) -> str:
        stream = recognizer.create_stream()
        stream.accept_waveform(config.sample_rate, pcm_samples(chunk))
        recognizer.decode_stream(stream)
        return str(stream.result.text)

    return transcribe


@dataclass(frozen=True)
class Interruption:
    cancelled: bool
    reason: str = ""


class FullDuplexController:
    """Detect user barge-in during synthesized speech using PCM energy."""

    def __init__(self, threshold: int = 1200, min_frames: int = 2) -> None:
        self.threshold = threshold
        self.min_frames = min_frames
        self.speaking = False
        self.loud_frames = 0

    def begin_tts(self) -> None:
        self.speaking = True
        self.loud_frames = 0

    def end_tts(self) -> None:
        self.speaking = False
        self.loud_frames = 0

    def observe_user_pcm(self, frame: bytes) -> Interruption:
        samples = pcm_samples(frame)
        rms = (sum(sample * sample for sample in samples) / len(samples)) ** 0.5 if samples else 0
        if not self.speaking:
            return Interruption(False)
        if rms >= self.threshold:
            self.loud_frames += 1
        else:
            self.loud_frames = 0
        if self.loud_frames >= self.min_frames:
            self.end_tts()
            return Interruption(True, "barge_in")
        return Interruption(False)


def chunk_pcm(pcm: bytes, bytes_per_chunk: int) -> Iterable[bytes]:
    """Split PCM into fixed-size chunks."""
    for start in range(0, len(pcm), bytes_per_chunk):
        yield pcm[start : start + bytes_per_chunk]
