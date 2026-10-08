import importlib.util
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "generator", ROOT / "scripts/generate-debian-maintainer-scripts.py"
)
generator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(generator)


class MaintainerScriptsTest(unittest.TestCase):
    def test_packaged_stub_is_embedded_and_modes_preserved(self):
        source = ROOT / "distribution/debian"
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            generator.generate(source, output)
            preinst = (output / "preinst").read_bytes()
            embedded = preinst.split(b"<<'VECTOR_CONFIG_STUB'\n", 1)[1].split(
                b"\nVECTOR_CONFIG_STUB\n", 1
            )[0] + b"\n"
            self.assertEqual(embedded, (source / "vector.yaml").read_bytes())
            for name in ("preinst", "postinst", "postrm"):
                self.assertEqual(
                    (output / name).stat().st_mode,
                    (source / "scripts" / name).stat().st_mode,
                )
            for name in ("postinst", "postrm"):
                self.assertEqual(
                    (output / name).read_bytes(),
                    (source / "scripts" / name).read_bytes(),
                )

    def test_new_stub_content_is_used_on_regeneration(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            output = Path(directory) / "output"
            (source / "scripts").mkdir(parents=True)
            (source / "scripts/preinst").write_bytes(b"@VECTOR_CONFIG_STUB@\n")
            for stub in (b"# first\n", b"# quotes '$' and backslashes \\\n\n"):
                (source / "vector.yaml").write_bytes(stub)
                generator.generate(source, output)
                self.assertEqual((output / "preinst").read_bytes(), stub)

    def test_invalid_inputs_fail_before_writing_output(self):
        for template, stub in (
            (b"no placeholder\n", b"# stub\n"),
            (b"@VECTOR_CONFIG_STUB@\n" * 2, b"# stub\n"),
            (b"@VECTOR_CONFIG_STUB@\n", b"# missing newline"),
            (b"@VECTOR_CONFIG_STUB@\n", b"VECTOR_CONFIG_STUB\n"),
        ):
            with self.subTest(template=template, stub=stub):
                with tempfile.TemporaryDirectory() as directory:
                    source = Path(directory) / "source"
                    output = Path(directory) / "output"
                    (source / "scripts").mkdir(parents=True)
                    (source / "scripts/preinst").write_bytes(template)
                    (source / "vector.yaml").write_bytes(stub)
                    with self.assertRaises(ValueError):
                        generator.generate(source, output)
                    self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
