"""Narrate every scene, measure the audio, and write the timeline.

Duration is a result: each scene lasts its own narration plus a fixed lead-in
and tail. Subtitles are shifted onto that timeline, so picture, voice and text
cannot drift apart.

    python kit/timeline.py
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEAD = 0.35
TAIL = 0.45


def probe(path: Path) -> float:
    out = subprocess.check_output(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
            str(path),
        ]
    )
    return float(out.strip())


def srt_time(seconds: float) -> str:
    ms = round(seconds * 1000)
    h, ms = divmod(ms, 3600_000)
    m, ms = divmod(ms, 60_000)
    s, ms = divmod(ms, 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"


def to_seconds(stamp: str) -> float:
    hms, ms = stamp.split(",")
    h, m, s = [int(part) for part in hms.split(":")]
    return h * 3600 + m * 60 + s + int(ms) / 1000


def parse_srt(text: str):
    items = []
    for block in re.split(r"\n\s*\n", text.strip()):
        lines = [line for line in block.splitlines() if line.strip()]
        if len(lines) < 3:
            continue
        start, end = lines[1].split(" --> ")
        items.append((to_seconds(start), to_seconds(end), " ".join(lines[2:])))
    return items


def main() -> int:
    board = json.loads((ROOT / "storyboard.json").read_text(encoding="utf-8"))
    voice = board["format"]["voice"]
    rate = board["format"].get("rate")
    audio_dir = ROOT / "out" / "audio"
    audio_dir.mkdir(parents=True, exist_ok=True)

    timeline = []
    subtitles = []
    cursor = 0.0
    for scene in board["scenes"]:
        scene_id = scene["id"]
        mp3 = audio_dir / f"{scene_id}.mp3"
        srt = audio_dir / f"{scene_id}.srt"
        command = [
            sys.executable,
            "-m",
            "edge_tts",
            "--voice",
            voice,
            "--text",
            scene["narration"],
            "--write-media",
            str(mp3),
            "--write-subtitles",
            str(srt),
        ]
        if rate:
            command.extend(["--rate", str(rate)])
        subprocess.run(
            command,
            check=True,
            stdout=subprocess.DEVNULL,
        )
        seconds = probe(mp3)
        start = cursor
        duration = LEAD + seconds + TAIL
        timeline.append(
            {
                "id": scene_id,
                "start": round(start, 3),
                "duration": round(duration, 3),
                "audio": round(seconds, 3),
            }
        )
        for a, b, text in parse_srt(srt.read_text(encoding="utf-8-sig")):
            subtitles.append((start + LEAD + a, start + LEAD + b, text))
        cursor += duration

    # A cue never overlaps the next one: two lines at once are unreadable.
    subtitles.sort(key=lambda cue: cue[0])
    for index in range(len(subtitles) - 1):
        start, end, text = subtitles[index]
        next_start = subtitles[index + 1][0]
        if end > next_start - 0.02:
            subtitles[index] = (start, max(start + 0.5, next_start - 0.02), text)

    (ROOT / "out" / "timeline.json").write_text(
        json.dumps(
            {"scenes": timeline, "total": round(cursor, 3), "lead": LEAD, "tail": TAIL},
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )
    with (ROOT / "out" / "all.srt").open("w", encoding="utf-8") as handle:
        for index, (a, b, text) in enumerate(subtitles, start=1):
            handle.write(f"{index}\n{srt_time(a)} --> {srt_time(b)}\n{text}\n\n")

    print(f"{'scene':<6}{'audio':>8}{'duration':>10}{'start':>8}")
    for item in timeline:
        print(f"{item['id']:<6}{item['audio']:>8.2f}{item['duration']:>10.2f}{item['start']:>8.2f}")
    print(f"total {cursor:.2f}s, {len(subtitles)} subtitle lines")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
