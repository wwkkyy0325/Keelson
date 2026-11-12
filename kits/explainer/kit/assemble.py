"""Put the scenes together: frames + narration -> one film, then burn the
subtitles and normalize the loudness.

    python kit/assemble.py
"""

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "out"
SCENES = OUT / "scenes"


def run(args, cwd=None):
    subprocess.run(args, check=True, cwd=cwd)


def font_option() -> str:
    """Point libass at a CJK font.

    A native Windows ffmpeg finds Microsoft YaHei through fontconfig. WSL
    does not index the mounted Windows fonts, so the burn renders tofu boxes
    unless the directory is named here.
    """
    windows = Path("/mnt/c/Windows/Fonts")
    if windows.is_dir():
        return f":fontsdir={windows}"
    if Path("C:/Windows/Fonts").is_dir():
        return ""
    probe = subprocess.run(["fc-match", "Microsoft YaHei"], capture_output=True, text=True)
    if probe.returncode != 0 or "DejaVu" in probe.stdout:
        raise SystemExit(
            "no CJK font for subtitles: install one, or mount C:/Windows/Fonts"
        )
    return ""


def main():
    board = json.loads((ROOT / "storyboard.json").read_text(encoding="utf-8"))
    plan = json.loads((OUT / "timeline.json").read_text(encoding="utf-8"))
    fps = int(board["format"]["fps"])
    lead = plan["lead"]
    tail = plan["tail"]
    output = ROOT / (board.get("output") or "out/film.mp4")
    SCENES.mkdir(parents=True, exist_ok=True)

    for scene in plan["scenes"]:
        scene_id = scene["id"]
        frames = sorted((OUT / "frames" / scene_id).glob("*.png"))
        expected = round(scene["duration"] * fps)
        if len(frames) < expected - 1:
            raise SystemExit(f"{scene_id}: {len(frames)} frames, expected {expected}")
        run(
            [
                "ffmpeg",
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-framerate",
                str(fps),
                "-i",
                str(OUT / "frames" / scene_id / "%04d.png"),
                "-i",
                str(OUT / "audio" / f"{scene_id}.mp3"),
                "-filter_complex",
                f"[1:a]adelay={round(lead * 1000)}|{round(lead * 1000)},apad=pad_dur={tail}[a]",
                "-map",
                "0:v",
                "-map",
                "[a]",
                "-c:v",
                "libx264",
                "-crf",
                "17",
                "-preset",
                "medium",
                "-pix_fmt",
                "yuv420p",
                "-r",
                str(fps),
                "-t",
                f"{scene['duration']:.3f}",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                str(SCENES / f"{scene_id}.mp4"),
            ]
        )
        print(f"scene {scene_id}: {scene['duration']:.2f}s")

    listing = "".join(f"file '{SCENES / (s['id'] + '.mp4')}'\n" for s in plan["scenes"])
    (OUT / "concat.txt").write_text(listing, encoding="utf-8")
    run(
        [
            "ffmpeg",
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            str(OUT / "concat.txt"),
            "-c",
            "copy",
            str(OUT / "video-nosub.mp4"),
        ]
    )

    # Burn the subtitles and normalize the mix in one pass. The style is part
    # of the kit: one font, one size, one bottom band.
    subtitles = (
        "subtitles=all.srt"
        + font_option()
        + ":force_style='FontName=Microsoft YaHei,FontSize=16,"
        "PrimaryColour=&H00FFFFFF,OutlineColour=&H00191A1E,BorderStyle=1,"
        "Outline=2,Shadow=0,MarginV=46'"
    )
    run(
        [
            "ffmpeg",
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            "video-nosub.mp4",
            "-vf",
            subtitles,
            "-af",
            "loudnorm=I=-16:TP=-1.5:LRA=11",
            "-c:v",
            "libx264",
            "-crf",
            "18",
            "-preset",
            "medium",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            str(output),
        ],
        cwd=OUT,
    )
    print(f"film: {output}")


if __name__ == "__main__":
    main()
