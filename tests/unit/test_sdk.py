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


if __name__ == "__main__":
    unittest.main()
