"""Hosted release acceptance: install and run only an extracted binary package."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def run(command, cwd, environment, timeout=60):
    result = subprocess.run(command, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
                            capture_output=True, encoding="utf-8", timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"{Path(command[0]).name} failed ({result.returncode}): {result.stderr}")
    return result


def main() -> int:
    from scripts.local_environment import environment
    archives = list((ROOT / "dist").glob("*.zip"))
    if len(archives) != 1:
        raise ValueError("Expected one platform archive")
    archive = archives[0]
    expected = archive.with_suffix(".zip.sha256").read_text(encoding="utf-8").split()[0]
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == expected
    with tempfile.TemporaryDirectory(prefix="prebuilt smoke ", dir=ROOT / "logs") as folder:
        parent = Path(folder)
        with zipfile.ZipFile(archive) as package:
            for member in package.infolist():
                path = Path(member.filename)
                if path.is_absolute() or ".." in path.parts:
                    raise ValueError("Archive member escaped extraction root")
            package.extractall(parent)
        extracted = next(path for path in parent.iterdir() if path.is_dir())
        env = environment(extracted)
        env["NUMERA_EMOJI_MAPPER_NO_DOTENV"] = "1"
        env["PYTHONPATH"] = str(extracted)
        # The installer receives an unrelated CWD; all paths remain package-local.
        run([sys.executable, str(extracted / "scripts/install_prebuilt.py")], parent, env, 240)
        python = extracted / (".venv/Scripts/python.exe" if os.name == "nt" else ".venv/bin/python")
        launch = extracted / "scripts/launch_prebuilt.py"
        assert "numera-emoji" in run([str(python), str(launch), "--help"], parent, env).stdout
        run([str(python), "-c", "from emojikit.similarity import require_native; require_native()"], extracted, env)
        fixture = extracted / "logs" / "fixture.tgs"
        shutil.copyfile(ROOT / "tests/fixtures/lottie/red_circle_512.tgs", fixture)
        helper = extracted / "logs" / "codec_smoke.py"
        helper.write_text('''import os,pathlib,tempfile
from PIL import Image
from emojikit import media, media_lottie
root=pathlib.Path.cwd().resolve()
for key in ('TEMP','TMP','TMPDIR','PIP_CACHE_DIR','XDG_CACHE_HOME','PYTHONPYCACHEPREFIX'):
 assert pathlib.Path(os.environ[key]).resolve().is_relative_to(root),key
assert pathlib.Path(tempfile.gettempdir()).resolve().is_relative_to(root)
media.to_static_png(root/'assets/numera-emoji-mapper-logo.png',root/'logs/art.png')
with Image.open(root/'logs/art.png') as image: assert image.size==(100,100)
media_lottie.lottie_still_webp(root/'logs/fixture.tgs',root/'logs/motion.webp',size=72)
with Image.open(root/'logs/motion.webp') as image: assert image.size==(72,72)
try:
 media.to_video_webm(root/'logs/art.png',root/'logs/art.webm')
except __import__('subprocess').CalledProcessError as error:
 print(error.stderr.decode('utf-8',errors='replace') if isinstance(error.stderr,bytes) else error.stderr,file=__import__('sys').stderr)
 raise
media.validate_video(root/'logs/art.webm')
print('actual PNG/TGS/VP9 codecs and package-local paths verified')
''', encoding="utf-8")
        run([str(python), str(helper)], extracted, env, 90)
        input_dir = extracted / "logs" / "input"
        input_dir.mkdir()
        shutil.copyfile(extracted / "assets/numera-emoji-mapper-logo.png", input_dir / "art.png")
        run([str(python), str(launch), "make-emoji-pngs", "--in", str(input_dir),
             "--out", str(extracted / "logs" / "output")], parent, env, 90)
        assert (extracted / "logs/output/art.png").is_file()
        assert not (extracted / "native/target").exists()
        assert not (extracted / "tests").exists()
        print(json.dumps({"installed": extracted.name, "local_temp_cache": True,
                          "source_build": False, "native_cli_and_codecs": True}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
