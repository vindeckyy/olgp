import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "engine/olgp-engine/src"))

import voice  # noqa: E402


class VoiceTest(unittest.TestCase):
    def test_strong_repeat_fires_early(self) -> None:
        features = voice.GateInput(2, 3, 0.1, 0.85, 0.2)
        self.assertTrue(voice.should_speak(features))

    def test_weak_answer_waits(self) -> None:
        features = voice.GateInput(1, 20, 0.8, 0.70, 0.9)
        self.assertFalse(voice.should_speak(features))

    def test_session_emits_events_in_order(self) -> None:
        session = voice.VoiceSession("s1", 24000)
        session.stt_partial("what is")
        session.answer("water")
        self.assertTrue(session.decide(voice.GateInput(2, 5, 0.0, 0.9, 0.4)))
        self.assertEqual(
            session.events,
            ["stt_partial", "text_tentative", "text", "end"],
        )
        self.assertEqual(session.final_text, "water")

    def test_session_waits_on_weak_answer(self) -> None:
        session = voice.VoiceSession("s4", 16000)
        session.answer("maybe")
        self.assertFalse(session.decide(voice.GateInput(0, 4, 0, 90, 50)))
        self.assertIsNone(session.final_text)
    def test_session_cancel_clears_tentative(self) -> None:
        session = voice.VoiceSession("s2", 16000)
        session.answer("maybe")
        session.cancel()
        self.assertIsNone(session.tentative)
        self.assertFalse(session.decide(voice.GateInput(3, 5, 0, 1, 1)))

    def test_binary_frame_prefix(self) -> None:
        session = voice.VoiceSession("s3", 24000)
        self.assertEqual(session.binary_frame(0x01, b"\x00\x01"), b"\x01\x00\x01")
        with self.assertRaises(ValueError):
            session.binary_frame(0x02, b"")


if __name__ == "__main__":
    unittest.main()
