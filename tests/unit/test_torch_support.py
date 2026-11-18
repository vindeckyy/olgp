import pathlib
import sys
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/python"))

import olgp
import olgp.torch_support as torch_support  # noqa: E402


class TorchSupportTest(unittest.TestCase):
    def test_device_string_and_generate(self) -> None:
        client = olgp.Client("http://box:8787", "token")
        device = torch_support.gpu("olgp:0", "model", client)
        self.assertEqual(str(device), "olgp:0")
        with mock.patch.object(client, "chat", return_value="answer") as chat:
            self.assertEqual(device.generate("question"), "answer")
        chat.assert_called_once()


if __name__ == "__main__":
    unittest.main()
