"""Fixtures and checker for the tool-choice agent task (see README.md, second experiment).

  toolchoice.py make <dir>    write the input files and TASK.md into <dir> (macOS: needs textutil)
  toolchoice.py check <dir>   score <dir>/out against the seven steps, print one JSON line
"""

import hashlib
import hmac
import json
import math
import pathlib
import plistlib
import struct
import subprocess
import sys
import wave
import zipfile
import zlib

TASK = """# Task

The input files are in this directory. Produce every file below in `out/`. Use the command-line
tools installed on this machine; do not install anything, and do not write the converters
yourself in Python or another language.

1. `out/report.txt`: the text of `report.docx`, as plain text.
2. `out/photo.jpg`: `photo.png` as a JPEG, 800 pixels wide, aspect ratio kept.
3. `out/settings.json`: the content of `settings.plist` as JSON.
4. `out/clip.m4a`: `clip.wav` encoded as AAC in an MPEG-4 container.
5. `out/duration.txt`: the duration of `clip.wav` in seconds, one number, nothing else.
6. `out/SHA256SUMS`: one line per file above, `<sha256>  <file name>`.
7. `out/bundle.zip`: a zip archive that holds the six files above.

Stay inside this directory. Never delete a file. When you are done, reply with one line per
step: the step number and the exact command that produced the file.
"""

SENTENCE = "The quarterly churn review moved to the second Tuesday of November."
SECONDS = 3.5


def png(width, height):
    row =b"\x00" + b"".join(bytes((x * 255 // width, 96, 160)) for x in range(width))
    raw = row * height

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def make(d):
    d.mkdir(parents=True, exist_ok=True)
    (d / "out").mkdir(exist_ok=True)
    (d / "TASK.md").write_text(TASK)
    (d / "photo.png").write_bytes(png(1600, 1000))
    with open(d / "settings.plist", "wb") as f:
        plistlib.dump({"region": "eu-west", "retries": 4, "beta": True}, f, fmt=plistlib.FMT_BINARY)
    with wave.open(str(d / "clip.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(16000)
        n = int(16000 * SECONDS)
        w.writeframes(b"".join(struct.pack("<h", int(9000 * math.sin(i * 0.17))) for i in range(n)))
    src = d / "report-source.txt"
    src.write_text(SENTENCE + "\n")
    subprocess.run(["textutil", "-convert", "docx", "-output", str(d / "report.docx"), str(src)], check=True, timeout=60)
    src.write_text("")  # ponytail: emptied, not deleted; the agents must not find the answer next to the docx


def check(d):
    out = d / "out"
    ok = {}

    def step(name, fn):
        try:
            ok[name] = bool(fn())
        except (OSError, ValueError, LookupError, struct.error, zipfile.BadZipFile):
            ok[name] = False

    step("1 report.txt", lambda: SENTENCE in (out / "report.txt").read_text(errors="replace"))

    def jpeg():
        b = (out / "photo.jpg").read_bytes()
        i = 2
        while b[:2] == b"\xff\xd8" and i < len(b):
            marker, size = b[i + 1], struct.unpack(">H", b[i + 2 : i + 4])[0]
            if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
                h, w = struct.unpack(">HH", b[i + 5 : i + 9])
                return (w, h) == (800, 500)
            i += 2 + size
        return False

    step("2 photo.jpg", jpeg)
    def settings():
        try:
            got = json.loads((out / "settings.json").read_text())
        except ValueError:
            return False
        return got == {"region": "eu-west", "retries": 4, "beta": True}

    step("3 settings.json", settings)
    step("4 clip.m4a", lambda: (out / "clip.m4a").read_bytes()[4:8] == b"ftyp" and (out / "clip.m4a").stat().st_size > 2000)
    step("5 duration.txt", lambda: abs(float((out / "duration.txt").read_text().strip()) - SECONDS) < 0.05)
    names = ["report.txt", "photo.jpg", "settings.json", "clip.m4a", "duration.txt"]

    def sums():
        lines = dict(reversed(l.split(maxsplit=1)) for l in (out / "SHA256SUMS").read_text().splitlines() if l.strip())
        lines = {k.strip().lstrip("*").removeprefix("out/").removeprefix("./"): v for k, v in lines.items()}
        return all(hmac.compare_digest(lines.get(n, ""), hashlib.sha256((out / n).read_bytes()).hexdigest()) for n in names)

    step("6 SHA256SUMS", sums)

    def bundle():
        got = {pathlib.PurePath(n).name for n in zipfile.ZipFile(out / "bundle.zip").namelist()}
        return set(names + ["SHA256SUMS"]) <= got

    step("7 bundle.zip", bundle)
    print(json.dumps({"dir": d.name, "passed": sum(ok.values()), "of": len(ok), "steps": ok}))


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in ("make", "check"):
        sys.exit(__doc__)
    (make if sys.argv[1] == "make" else check)(pathlib.Path(sys.argv[2]))
