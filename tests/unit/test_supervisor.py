import pathlib
import sys
import tempfile
import unittest
import os
from unittest import mock

import blake3

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "engine/olgp-engine/src"))

import supervisor  # noqa: E402


class SupervisorTest(unittest.TestCase):
    def setUp(self) -> None:
        self.profiles = supervisor.load_profiles(
            ROOT / "engine/olgp-engine/profiles.toml"
        )

    def test_profiles_have_four_aliases(self) -> None:
        self.assertEqual(len(set(supervisor.ALIASES)), 4)
        self.assertEqual(set(supervisor.ALIASES), {
            "qwen25-0p5", "qwen25-1p5", "qwen15-moe-a2p7", "turbo"
        })

    def test_dense_command_binds_loopback_only(self) -> None:
        command = supervisor.build_command(
            "qwen25-0p5", pathlib.Path("/models/model.gguf"), 8080, self.profiles
        )
        self.assertEqual(command[0], "llama-server")
        self.assertIn("127.0.0.1", command)
        self.assertIn("99", command)

    def test_moe_command_uses_inferssd(self) -> None:
        command = supervisor.build_command(
            "qwen15-moe-a2p7", pathlib.Path("/models/moe.gguf"), 8080, self.profiles
        )
        self.assertEqual(command[:2], ["inferssd", "serve"])

    def test_hash_sidecar_is_verified(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            model = pathlib.Path(directory) / "model.gguf"
            model.write_bytes(b"olgp weights")
            model.with_name(model.name + ".blake3").write_text(
                blake3.blake3(b"olgp weights").hexdigest()
            )
            supervisor.verify_hash(model)
            model.with_name(model.name + ".blake3").write_text("00" * 32)
            with self.assertRaises(ValueError):
                supervisor.verify_hash(model)

    def test_resolve_model_and_missing_model(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "model-Q4_K_M.gguf").write_bytes(b"x")
            resolved = supervisor.resolve_model("qwen25-0p5", root, self.profiles)
            self.assertEqual(resolved.name, "model-Q4_K_M.gguf")
            with self.assertRaises(FileNotFoundError):
                supervisor.resolve_model(
                    "qwen15-moe-a2p7",
                    root,
                    {"moe": dict(self.profiles["moe"], filename_glob="missing-*.gguf")},
                )


    def test_turbo_profile_requires_environment_path(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            model = pathlib.Path(directory) / "turbo.gguf"
            model.write_bytes(b"x")
            with mock.patch.dict(os.environ, {"OLGP_TURBO_GGUF": str(model)}):
                resolved = supervisor.resolve_model("turbo", pathlib.Path(directory), self.profiles)
            self.assertEqual(resolved, model)
        with mock.patch.dict(os.environ, {}, clear=True):
            with self.assertRaises(ValueError):
                supervisor.resolve_model("turbo", pathlib.Path("/tmp"), self.profiles)
        with mock.patch.dict(os.environ, {"OLGP_TURBO_GGUF": "/missing/turbo.gguf"}):
            with self.assertRaises(FileNotFoundError):
                supervisor.resolve_model("turbo", pathlib.Path("/tmp"), self.profiles)

    def test_missing_profile_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            supervisor.resolve_model("qwen25-0p5", pathlib.Path("/tmp"), {})

    def test_unknown_alias_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            supervisor.resolve_model("missing", pathlib.Path("/tmp"), self.profiles)
        with self.assertRaises(ValueError):
            supervisor.build_command("missing", pathlib.Path("/tmp/x"), 8080, self.profiles)

    def test_unsupported_engine_is_rejected(self) -> None:
        profiles = {"small": dict(self.profiles["small"], engine="bad")}
        with self.assertRaises(ValueError):
            supervisor.build_command("qwen25-0p5", pathlib.Path("/tmp/x"), 8080, profiles)

    def test_missing_hash_sidecar_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            model = pathlib.Path(directory) / "model.gguf"
            model.write_bytes(b"x")
            with self.assertRaises(FileNotFoundError):
                supervisor.verify_hash(model)

    def test_command_preview_quotes_spaces(self) -> None:
        self.assertEqual(supervisor.command_preview(["a", "b c"]), "a 'b c'")


if __name__ == "__main__":
    unittest.main()
