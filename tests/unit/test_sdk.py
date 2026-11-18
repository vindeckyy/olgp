import io
import json
import os
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/python"))

import olgp  # noqa: E402


class FakeResponse:
    def __init__(self, payload: bytes) -> None:
        self.payload = payload

    def __enter__(self) -> "FakeResponse":
        return self

    def __exit__(self, *args: object) -> None:
        return None

    def read(self) -> bytes:
        return self.payload


class SdkTest(unittest.TestCase):
    def test_discover_parses_cli_output(self) -> None:
        output = "box1 fp123 127.0.0.1 GPU-A\nbox2 fp456 127.0.0.2 GPU-B\n"
        with mock.patch.object(olgp.subprocess, "check_output", return_value=output):
            boxes = olgp.discover()
        self.assertEqual(len(boxes), 2)
        self.assertEqual(boxes[0].gpu, "GPU-A")

    def test_pair_returns_credential_path(self) -> None:
        with tempfile.TemporaryDirectory() as home:
            with mock.patch.dict(os.environ, {"HOME": home}):
                with mock.patch.object(olgp.subprocess, "run") as run:
                    path = olgp.pair("box1", "123456")
            self.assertTrue(path.endswith("box1.json"))
            run.assert_called_once()

    def test_chat_sends_token_and_returns_text(self) -> None:
        payload = {"choices": [{"message": {"content": "hello"}}]}
        response = FakeResponse(json.dumps(payload).encode())
        client = olgp.Client("http://olgp.local:8787/", "token")
        with mock.patch.object(olgp.urllib.request, "urlopen", return_value=response) as open_mock:
            reply = client.chat("model", [{"role": "user", "content": "hi"}])
        self.assertEqual(reply, "hello")
        request = open_mock.call_args.args[0]
        self.assertEqual(request.get_header("Authorization"), "Bearer token")

    def test_connect_uses_environment(self) -> None:
        with mock.patch.dict(
            os.environ,
            {"OLGP_BASE_URL": "http://box:9999", "OLGP_DEVICE_TOKEN": "env-token"},
        ):
            client = olgp.connect()
        self.assertEqual(client.base_url, "http://box:9999")
        self.assertEqual(client.token, "env-token")



    def test_product_modules_cover_public_routes(self) -> None:
        client = olgp.Client("http://box:8787", "token", memory_token="memory")
        response = FakeResponse(json.dumps({"ok": True}).encode())
        calls = [
            lambda: client.speakers.list(),
            lambda: client.speakers.retrieve("spk"),
            lambda: client.speakers.update("spk", volume=35),
            lambda: client.speakers.play("spk", url="https://example.test/a.wav"),
            lambda: client.speakers.stop("spk"),
            lambda: client.speakers.bluetooth("spk"),
            lambda: client.speakers.startBluetoothPairing("spk"),
            lambda: client.timers.list(),
            lambda: client.timers.create(title="Tea", dueAtMs=1),
            lambda: client.timers.cancel("timer"),
            lambda: client.timers.snooze("timer", 60),
            lambda: client.memos.list(),
            lambda: client.memos.create(title="note"),
            lambda: client.memos.dismiss("memo"),
            lambda: client.phones.ring("phone"),
            lambda: client.audio_memory.status(),
            lambda: client.audio_memory.ready(),
            lambda: client.audio_memory.settings(mode="active"),
            lambda: client.audio_memory.episodes(),
            lambda: client.audio_memory.notes(),
            lambda: client.audio_memory.add_note("remember"),
            lambda: client.audio_memory.query("remember"),
            lambda: client.audio_memory.forget("note"),
            lambda: client.audio_memory.clear(),
            lambda: client.knowledge_bases.create("Docs"),
            lambda: client.knowledge_bases.retrieve("kb"),
            lambda: client.knowledge_bases.delete("kb"),
            lambda: client.knowledge_bases.documents("kb"),
            lambda: client.knowledge_bases.add_text("kb", "a.txt", "alpha"),
            lambda: client.knowledge_bases.add_file("kb", "a.bin", "gamma"),
            lambda: client.knowledge_bases.add_image("kb", "a.png", "data:image/png;base64,AA"),
            lambda: client.knowledge_bases.query("kb", "alpha"),
            lambda: client.images.jobs.create(operation="generate"),
            lambda: client.images.jobs.result("job"),
            lambda: client.images.jobs.cancel("job"),
            lambda: client.recordings.create(True),
            lambda: client.recordings.list(),
            lambda: client.recordings.get("rec"),
            lambda: client.recordings.delete("rec"),
            lambda: client.recordings.append("rec", 0, "AA"),
            lambda: client.recordings.finish("rec"),
            lambda: client.recordings.reprocess("rec"),
        ]
        with mock.patch.object(
            olgp.urllib.request,
            "urlopen",
            side_effect=[response for _ in calls],
        ) as open_mock:
            for call in calls:
                self.assertEqual(call(), {"ok": True})
        self.assertEqual(open_mock.call_count, len(calls))
        memory_requests = [
            call.args[0]
            for call in open_mock.call_args_list
            if "/v1/audio-memory" in call.args[0].full_url
        ]
        self.assertTrue(memory_requests)
        self.assertTrue(
            all(
                request.get_header("X-wici-memory-token") == "memory"
                or request.get_header("X-Wici-Memory-Token") == "memory"
                for request in memory_requests
            )
        )


    def test_engine_and_image_lifecycle_helpers(self) -> None:
        client = olgp.Client("http://box:8787", "token")
        job = {"id": "img-1", "status": "succeeded"}
        responses = [
            FakeResponse(json.dumps({"ok": True}).encode())
            for _ in range(4)
        ] + [FakeResponse(json.dumps(job).encode()) for _ in range(5)]
        with mock.patch.object(
            olgp.urllib.request, "urlopen", side_effect=responses
        ) as open_mock:
            self.assertEqual(client.engine.status(), {"ok": True})
            self.assertEqual(client.engine.capabilities(), {"ok": True})
            self.assertEqual(client.engine.sessions(), {"ok": True})
            self.assertEqual(client.images.frames_process(image="AA"), {"ok": True})
            self.assertEqual(client.images.jobs.retrieve("img-1"), job)
            self.assertEqual(client.images.jobs.observe("img-1"), job)
            self.assertEqual(client.images.jobs.wait("img-1", timeout=1, interval=0), job)
            self.assertEqual(client.images.jobs.result("img-1"), job)
            self.assertEqual(client.images.jobs.cancel("img-1"), job)
        paths = [call.args[0].full_url for call in open_mock.call_args_list]
        self.assertTrue(any(path.endswith("/engine/status") for path in paths))
        self.assertTrue(any(path.endswith("/v2/images/frames:process") for path in paths))
        self.assertTrue(any(path.endswith("/v2/images/jobs/img-1:cancel") for path in paths))


    def test_search_headers_are_sent_when_configured(self) -> None:
        client = olgp.Client(
            "http://box:8787",
            "token",
            search_provider="tavily",
            tavily_key="key",
        )
        response = FakeResponse(json.dumps({"choices": [{"message": {"content": "x"}}]}).encode())
        with mock.patch.object(olgp.urllib.request, "urlopen", return_value=response) as open_mock:
            client.models_list() if hasattr(client, "models_list") else client.chat("model", "hi")
        request = open_mock.call_args.args[0]
        self.assertTrue(
            request.get_header("X-wici-search-provider") == "tavily"
            or request.get_header("X-Wici-Search-Provider") == "tavily"
        )
        self.assertTrue(
            request.get_header("X-wici-tavily-key") == "key"
            or request.get_header("X-Wici-Tavily-Key") == "key"
        )

if __name__ == "__main__":
    unittest.main()
