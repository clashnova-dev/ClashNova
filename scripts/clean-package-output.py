"""Remove only generated installers before packaging, retaining Rust compilation caches."""
import argparse
import pathlib
import shutil
import tempfile
import unittest

TARGETS = (
    "x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin", "x86_64-apple-darwin",
)

def clean(root: pathlib.Path, target: str) -> None:
    if target not in TARGETS:
        raise ValueError("Unsupported package target")
    base = root.resolve()
    for path in (base / "src-tauri" / "target" / target / "release" / "bundle",
                 base / "release-artifacts"):
        if path.is_symlink():
            path.unlink()
        elif path.exists():
            if not path.resolve().is_relative_to(base):
                raise ValueError("Generated output escaped the checkout")
            shutil.rmtree(path)

class OutputTests(unittest.TestCase):
    def test_old_installers_are_removed_but_compiler_cache_and_configs_are_kept(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            release = root / "src-tauri/target" / TARGETS[0] / "release"
            stale = release / "bundle/nsis/ClashNova_0.1.2_x64-setup.exe"
            stale.parent.mkdir(parents=True)
            stale.write_bytes(b"old-installer")
            cache = release / ".fingerprint/cache"
            cache.parent.mkdir()
            cache.write_bytes(b"compiled")
            user_config = root / "config.json"
            user_config.write_bytes(b"kept")
            artifacts = root / "release-artifacts"
            artifacts.mkdir()
            (artifacts / "old.sig").write_bytes(b"old-signature")
            clean(root, TARGETS[0])
            self.assertFalse(stale.exists())
            self.assertFalse(artifacts.exists())
            self.assertEqual(cache.read_bytes(), b"compiled")
            self.assertEqual(user_config.read_bytes(), b"kept")
            clean(root, TARGETS[0])  # Repeated CI attempts are safe.

    def test_target_cannot_select_an_arbitrary_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            sentinel = root / "preserved"
            sentinel.write_bytes(b"kept")
            with self.assertRaises(ValueError):
                clean(root, "../../preserved")
            self.assertEqual(sentinel.read_bytes(), b"kept")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--test", action="store_true")
    args = parser.parse_args()
    if args.test:
        unittest.main(argv=[__file__], exit=True)
    elif args.target:
        clean(pathlib.Path(__file__).resolve().parents[1], args.target)
        print("Removed cached package outputs; retained compilation cache:", args.target)
    else:
        parser.error("--target or --test is required")
