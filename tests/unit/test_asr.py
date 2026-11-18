import pathlib
import sys
import unittest

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

    def test_sherpa_factory_is_optional(self) -> None:
        self.assertTrue(callable(asr.sherpa_backend_factory()) or asr.sherpa_backend_factory() is None)


if __name__ == "__main__":
    unittest.main()
