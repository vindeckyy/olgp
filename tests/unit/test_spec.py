import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class SpecContractTest(unittest.TestCase):
    def test_contract_route_count(self) -> None:
        text = (ROOT / "spec/v1/contracts.md").read_text()
        rows = re.findall(r"^\| (GET|POST|PATCH|WS) \|", text, re.M)
        self.assertEqual(len(rows), 15)

    def test_txt_keys_are_nine(self) -> None:
        text = (ROOT / "spec/v1/txt-record.md").read_text()
        keys = re.findall(r"^\| `([a-z_]+)` \|", text, re.M)
        self.assertEqual(len(keys), 9)
        self.assertEqual(
            set(keys),
            {"v", "id", "fp", "api", "rpc", "caps", "gpu", "vram_mib", "box"},
        )

    def test_pairing_does_not_put_code_on_wire(self) -> None:
        text = (ROOT / "spec/v1/protocol.md").read_text()
        self.assertIn("code never crosses the wire", text)


if __name__ == "__main__":
    unittest.main()
