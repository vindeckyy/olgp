"""Deterministic real-time voice decision logic.

The gate uses only features visible before the user stops speaking. It does
not use gold answers or future transcript text.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class GateInput:
    repeat_count: int
    answer_length: int
    copy_ratio: float
    asr_confidence: float
    normalized_position: float


def should_speak(features: GateInput) -> bool:
    """Decide whether a tentative answer is stable enough to speak."""
    if features.repeat_count >= 2 and features.asr_confidence >= 0.80:
        return True
    return (
        features.repeat_count >= 1
        and 1 <= features.answer_length <= 8
        and features.copy_ratio <= 0.35
        and features.asr_confidence >= 0.90
        and features.normalized_position >= 0.35
    )


class VoiceSession:
    """One voice-v1 session with partial transcripts and early answers."""

    def __init__(self, session_id: str, sample_rate: int) -> None:
        self.session_id = session_id
        self.sample_rate = sample_rate
        self.transcript = ""
        self.tentative: str | None = None
        self.final_text: str | None = None
        self.events: list[str] = []

    def stt_partial(self, text: str) -> None:
        self.transcript = text
        self.events.append("stt_partial")

    def answer(self, text: str) -> None:
        self.tentative = text
        self.events.append("text_tentative")

    def decide(self, features: GateInput) -> bool:
        if self.tentative is None:
            return False
        if not should_speak(features):
            return False
        self.final_text = self.tentative
        self.events.append("text")
        self.events.append("end")
        return True

    def cancel(self) -> None:
        self.tentative = None
        self.events.append("cancel")

    def binary_frame(self, kind: int, pcm: bytes) -> bytes:
        """Build one voice-v1 binary frame."""
        if kind != 0x01:
            raise ValueError("only PCM frame kind 0x01 is supported")
        return bytes([kind]) + pcm
