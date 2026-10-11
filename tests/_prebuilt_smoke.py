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
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def run(command, cwd, environment, timeout=60):
    result = subprocess.run(command, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
                            capture_output=True, encoding="utf-8", timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"{Path(command[0]).name} failed ({result.returncode}): {result.stderr}")
    return result


def coin_manifests(extracted: Path, python: Path, environment: dict[str, str]) -> None:
    # Use only the extracted runtime: checkout references cannot rescue missing coin bindings.
    assert not (extracted / "tests").exists()
    for name in ("build_pack", "catalog", "panel", "make_emoji_pngs", "collection_migrate"):
        assert not (extracted / "emojikit" / f"{name}.py").exists(), name
    fixture = extracted / "logs" / "coin manifests"
    fixture.mkdir()
    assert (extracted / "logs/output/art.png").is_file(), "public codec asset must already exist"
    names = [f"cryptoemoji{index}_by_YourEmojiBot" for index in (1, 2)]
    state = fixture / "state.json"
    mapping = fixture / "map.json"
    state.write_text(json.dumps({"sets": [
        {"index": 2, "name": names[1], "title": "Fixture Coins 2"},
        {"index": 1, "name": names[0], "title": "Fixture Coins 1"}]}), encoding="utf-8")
    mapping.write_text(json.dumps({"zzz": "111111111", "aaa": "111111111",
                                   "b|b": "222222222"}), encoding="utf-8")
    before = {path: path.read_bytes() for path in (state, mapping, extracted / "logs/output/art.png")}
    observed, failures = [], []

    class Handler(BaseHTTPRequestHandler):
        def setup(self):
            super().setup()
            self.connection.settimeout(5)

        def log_message(self, *args):
            pass

        def do_POST(self):
            size = int(self.headers.get("Content-Length", "0"))
            if not 0 < size <= 4096:
                failures.append("invalid request length")
                self.send_error(400)
                return
            form = parse_qs(self.rfile.read(size).decode("utf-8"))
            name = form.get("name", [None])[0]
            if self.path != "/bottest-only-token/getStickerSet" or name not in names:
                failures.append("unexpected Telegram request")
                self.send_error(400)
                return
            observed.append(name)
            ids = ["222222222", "111111111"] if name == names[0] else ["333333333"]
            body = json.dumps({"ok": True, "result": {"name": name,
                "stickers": [{"custom_emoji_id": cid} for cid in ids]}}).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    try:
        env = {**environment, "TELEGRAM_BOT_TOKEN": "test-only-token",
               "TELEGRAM_API_BASE": f"http://127.0.0.1:{server.server_port}",
               "PYTHONPATH": str(extracted), "PYTHONNOUSERSITE": "1"}
        result = run([str(python), "-X", "importtime", "-m", "coins.write_manifests", "--state", str(state),
                      "--map", str(mapping), "--out-dir", str(fixture)], extracted, env, 30)
        imports = {line.rsplit("|", 1)[-1].strip() for line in result.stderr.splitlines()
                   if line.startswith("import time:")}
        assert "emojikit._native" in imports, "manifest must exercise the installed native bindings"
        assert not any(name == "tests" or name.startswith("tests.") for name in imports)
        assert not imports.intersection(f"emojikit.{name}" for name in
                                       ("build_pack", "catalog", "panel", "make_emoji_pngs", "collection_migrate"))
        assert not failures, failures
        assert observed == names, "state indexes must control actual native HTTP request order"
        manifests = fixture / "manifests"
        assert {path.name for path in manifests.iterdir()} == {"INDEX.md", *(f"{name}.md" for name in names)}
        for index, rows in enumerate((
                "| 1 | b\\|b | 222222222 |\n| 2 | aaa, zzz | 111111111 |\n",
                "| 1 | (unmapped) | 333333333 |\n")):
            count = 2 if index == 0 else 1
            expected = (f"# Fixture Coins {index + 1}\n\n"
                        f"Pack: https://t.me/addemoji/{names[index]}  |  {count} stickers\n\n"
                        "| # | Ticker(s) | Emoji ID |\n|---|-----------|----------|\n" + rows)
            assert (manifests / f"{names[index]}.md").read_text(encoding="utf-8") == expected
        expected_index = ("# Coin Emoji Packs - Manifest Index\n\n"
                          "| Pack | Stickers | Link |\n|------|----------|------|\n"
                          f"| {names[0]} | 2 | https://t.me/addemoji/{names[0]} |\n"
                          f"| {names[1]} | 1 | https://t.me/addemoji/{names[1]} |\n\n"
                          "Total: 2 packs, 3 stickers.\n")
        assert (manifests / "INDEX.md").read_text(encoding="utf-8") == expected_index
        assert before == {path: path.read_bytes() for path in before}
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
        assert not thread.is_alive(), "coin manifest fixture leaked its server"


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
        # A repeated install must refuse before altering the working environment.
        preserved = [extracted / ".venv/pyvenv.cfg", *extracted.joinpath("emojikit").glob("_native*.pyd"),
                     *extracted.joinpath("emojikit").glob("_native*.so")]
        before = {path: hashlib.sha256(path.read_bytes()).hexdigest() for path in preserved}
        retry = subprocess.run([str(python), str(extracted / "scripts/install_prebuilt.py")],
                               cwd=parent, env=env, stdin=subprocess.DEVNULL, capture_output=True,
                               encoding="utf-8", timeout=30)
        assert retry.returncode == 1 and "Existing installation preserved" in retry.stderr
        assert before == {path: hashlib.sha256(path.read_bytes()).hexdigest() for path in preserved}
        assert "numera-emoji" in run([str(python), str(launch), "--help"], parent, env).stdout
        if os.name == "nt":
            run(["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-File",
                 str(extracted / "run.ps1"), "-NoRelaunch", "-Check"], parent, env, 30)
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
        coin_manifests(extracted, python, env)
        print(json.dumps({"installed": extracted.name, "local_temp_cache": True,
                          "source_build": False, "native_cli_and_codecs": True,
                          "retained_coin_native_manifest": True}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
