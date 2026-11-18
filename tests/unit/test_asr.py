import os
import pathlib
import struct
import sys
import types
import tempfile
import unittest
from unittest import mock
ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "engine/olgp-engine/src"))

import asr  # noqa: E402


class AsrTest(unittest.TestCase):
    def test_partial_then_final(self) -> None:
        engine = asr.StreamingAsr(asr.EchoBackend(), chunk_ms=500)
        first = engine.push(bytes([1] * 32))
        second = engine.push(bytes([2] * 32))
        final = engine.finish()
        self.assertEqual(first.kind, "stt_partial")
        self.assertIn("chunk-01", second.text)
        self.assertEqual(final.kind, "stt_final")

    def test_chunk_pcm(self) -> None:
        chunks = list(asr.chunk_pcm(b"abcdef", 2))
        self.assertEqual(chunks, [b"ab", b"cd", b"ef"])

    def test_sherpa_backend_factory_transcribes(self) -> None:
        class FakeStream:
            def __init__(self) -> None:
                self.result = types.SimpleNamespace(text="hello")

            def accept_waveform(self, rate: int, samples: list[int]) -> None:
                self.rate = rate
                self.samples = samples

        class FakeRecognizer:
            def create_stream(self) -> FakeStream:
                return FakeStream()

            def decode_stream(self, stream: FakeStream) -> None:
                return None

        fake = types.SimpleNamespace(
            OfflineRecognizer=types.SimpleNamespace(
                from_whisper=lambda **kwargs: FakeRecognizer()
            )
        )
        with tempfile.TemporaryDirectory() as root:
            paths = [pathlib.Path(root) / name for name in ("encoder", "decoder", "tokens")]
            for path in paths:
                path.write_bytes(b"model")
            env = {
                "OLGP_ASR_ENCODER": str(paths[0]),
                "OLGP_ASR_DECODER": str(paths[1]),
                "OLGP_ASR_TOKENS": str(paths[2]),
            }
            with mock.patch.dict(os.environ, env), mock.patch.dict(sys.modules, {"sherpa_onnx": fake}):
                factory = asr.sherpa_backend_factory()
        self.assertIsNotNone(factory)
        self.assertEqual(factory(b"abc\x00"), "hello")

    def test_full_duplex_barge_in(self) -> None:
        controller = asr.FullDuplexController(threshold=100, min_frames=2)
        controller.begin_tts()
        self.assertFalse(controller.observe_user_pcm(struct.pack("<hh", 1000, 1000)).cancelled)
        self.assertTrue(controller.observe_user_pcm(struct.pack("<hh", 1000, 1000)).cancelled)
        self.assertFalse(controller.speaking)

    def test_sherpa_factory_is_optional(self) -> None:
        self.assertTrue(callable(asr.sherpa_backend_factory()) or asr.sherpa_backend_factory() is None)


if __name__ == "__main__":
    unittest.main()
