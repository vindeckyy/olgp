"""Generate a short WAV fixture and exercise the deterministic voice path."""

from __future__ import annotations

import pathlib
import struct
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "engine/olgp-engine/src"))

import voice  # noqa: E402


def main() -> int:
    rate = 16000
    samples = rate * 3
    pcm = b"".join(struct.pack("<h", 0) for _ in range(samples))
    header = b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVEfmt "
    header += struct.pack("<IHHIIHH", 16, 1, 1, rate, rate * 2, 2, 16)
    header += b"data" + struct.pack("<I", len(pcm))
    output = ROOT / "artifacts/e2e/voice-fixture.wav"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(header + pcm)

    session = voice.VoiceSession("fixture", rate)
    session.stt_partial("what is the answer")
    session.answer("water")
    if not session.decide(voice.GateInput(2, 5, 0.0, 0.95, 0.5)):
        return 1
    frame = session.binary_frame(0x01, pcm[:32])
    if frame[:1] != b"\x01":
        return 1
    print(f"voice fixture ok: {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
