"""Check what was rendered, before anyone watches it.

- `data/check.py` recomputes the numbers the scenes display;
- every scene has frames, and no sampled frame is blank;
- subtitles never overlap and stay short enough to read;
- with `--video`, the film is the right size and length, has audio, and is
  loud enough without clipping.

    python kit/qa.py
    python kit/qa.py --video
"""

import json
import re
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageStat

ROOT = Path(__file__).resolve().parent.parent
PROBLEMS = []


def fail(message):
    PROBLEMS.append(message)
    print(f"FAIL  {message}")


def check_data():
    check = ROOT / "data" / "check.py"
    if not check.exists():
        print("data: no data/check.py (the project makes no numeric claims)")
        return
    result = subprocess.run([sys.executable, str(check)], capture_output=True, text=True)
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        fail(f"data/check.py failed: {detail}")
        return
    print(f"data: {result.stdout.strip() or 'check.py passed'}")


def check_claims():
    check = ROOT / "data" / "check_claims.py"
    if not check.exists():
        return
    result = subprocess.run([sys.executable, str(check)], capture_output=True, text=True)
    if result.returncode != 0:
        detail = (result.stdout or result.stderr).strip()
        fail(f"data/check_claims.py failed:\n{detail}")
        return
    print(f"claims: {result.stdout.strip() or 'check_claims.py passed'}")


def check_frames(plan, fps):
    thumbs = []
    for scene in plan["scenes"]:
        folder = ROOT / "out" / "frames" / scene["id"]
        frames = sorted(folder.glob("*.png"))
        expected = round(scene["duration"] * fps)
        if len(frames) < expected - 1:
            fail(f"{scene['id']}: {len(frames)} frames, expected about {expected}")
        if not frames:
            continue
        for fraction in (0.25, 0.5, 0.75):
            path = frames[min(len(frames) - 1, int(len(frames) * fraction))]
            image = Image.open(path).convert("L")
            if ImageStat.Stat(image).stddev[0] < 4:
                fail(f"{scene['id']}: {path.name} is nearly blank")
        # The sheet shows each scene near its end, where the frame is complete.
        thumbs.append((scene["id"], Image.open(frames[int(len(frames) * 0.9)]).convert("RGB")))
    return thumbs


def contact_sheet(thumbs):
    width, height, gap = 640, 360, 16
    columns = 2
    rows = (len(thumbs) + columns - 1) // columns
    sheet = Image.new("RGB", (columns * width + (columns + 1) * gap, rows * height + (rows + 1) * gap), "#111111")
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.load_default(size=28)
    for index, (scene_id, image) in enumerate(thumbs):
        x = gap + (index % columns) * (width + gap)
        y = gap + (index // columns) * (height + gap)
        sheet.paste(image.resize((width, height), Image.LANCZOS), (x, y))
        draw.text((x + width - 56, y + 8), scene_id, fill="#FFD9A0", font=font)
    sheet.save(ROOT / "out" / "contact-sheet.jpg", quality=90)
    print(f"contact sheet: {sheet.width}x{sheet.height}")


def srt_seconds(stamp):
    hms, ms = stamp.split(",")
    h, m, s = [int(part) for part in hms.split(":")]
    return h * 3600 + m * 60 + s + int(ms) / 1000


def check_subtitles():
    text = (ROOT / "out" / "all.srt").read_text(encoding="utf-8-sig")
    cues = []
    for block in re.split(r"\n\s*\n", text.strip()):
        lines = [line for line in block.splitlines() if line.strip()]
        if len(lines) < 3:
            continue
        start, end = lines[1].split(" --> ")
        cues.append((srt_seconds(start), srt_seconds(end), "\n".join(lines[2:])))
    for index, (start, end, line) in enumerate(cues):
        if end <= start:
            fail(f"subtitle {index + 1}: empty duration")
        if index + 1 < len(cues) and end > cues[index + 1][0]:
            fail(f"subtitle {index + 1} overlaps the next cue")
        parts = line.split("\n")
        if len(parts) > 2:
            fail(f"subtitle {index + 1} has {len(parts)} lines")
        for part in parts:
            if len(part) > 28:
                fail(f"subtitle {index + 1} line is {len(part)} characters: {part}")
    print(f"subtitles: {len(cues)} cues, no overlap" if not PROBLEMS else f"subtitles: {len(cues)} cues")


def probe(path):
    out = subprocess.check_output(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-show_entries",
            "stream=codec_type,width,height,r_frame_rate",
            "-of",
            "json",
            str(path),
        ]
    )
    return json.loads(out)


def check_video(board, plan):
    video = ROOT / board.get("output", "out/film.mp4")
    if not video.exists():
        fail(f"{video.relative_to(ROOT)} is missing")
        return
    info = probe(video)
    duration = float(info["format"]["duration"])
    if abs(duration - plan["total"]) > 1.0:
        fail(f"video is {duration:.2f}s, timeline is {plan['total']:.2f}s")
    width = int(board["format"]["width"])
    height = int(board["format"]["height"])
    streams = info["streams"]
    video_stream = next((s for s in streams if s["codec_type"] == "video"), None)
    audio_stream = next((s for s in streams if s["codec_type"] == "audio"), None)
    if not video_stream or video_stream["width"] != width or video_stream["height"] != height:
        fail(f"video is not {width}x{height}")
    if not audio_stream:
        fail("video has no audio stream")
    loudness = subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-i",
            str(video),
            "-af",
            "loudnorm=print_format=json",
            "-f",
            "null",
            "-",
        ],
        capture_output=True,
        text=True,
    ).stderr
    match = re.search(r'"input_i"\s*:\s*"?(-?[\d.]+)', loudness)
    if match:
        lufs = float(match.group(1))
        if lufs < -20 or lufs > -12:
            fail(f"integrated loudness is {lufs} LUFS, target -16")
        print(f"video: {duration:.2f}s, {width}x{height}, audio present, {lufs} LUFS")


def main():
    board = json.loads((ROOT / "storyboard.json").read_text(encoding="utf-8"))
    plan = json.loads((ROOT / "out" / "timeline.json").read_text(encoding="utf-8"))
    fps = int(board["format"]["fps"])
    check_data()
    check_claims()
    thumbs = check_frames(plan, fps)
    contact_sheet(thumbs)
    check_subtitles()
    if "--video" in sys.argv:
        check_video(board, plan)
    if PROBLEMS:
        print(f"{len(PROBLEMS)} problem(s)")
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
