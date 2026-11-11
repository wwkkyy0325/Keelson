"""Recompute the displayed numbers, independently of the renderer.

The scenes show data/model.json; this script recomputes it from the spec and
fails if the two disagree. `kit/qa.py` runs it before a film is finished.

    python data/check.py
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main():
    spec = json.loads((ROOT / "data" / "spec.json").read_text(encoding="utf-8"))
    model = json.loads((ROOT / "data" / "model.json").read_text(encoding="utf-8"))
    image = spec["input"]
    kernel = spec["kernel"]
    size = len(image)
    k = len(kernel)
    out_size = size - k + 1

    computed = [
        [
            sum(
                image[r + dr][c + dc] * kernel[dr][dc]
                for dr in range(k)
                for dc in range(k)
            )
            for c in range(out_size)
        ]
        for r in range(out_size)
    ]
    if computed != model["output"]:
        raise SystemExit("model.json is not the convolution of spec.json")
    window = model["window"]
    value = computed[window["r"]][window["c"]]
    if value != window["value"]:
        raise SystemExit(
            f"the example window says {window['value']}, the convolution says {value}"
        )
    if model.get("outputSize") != out_size:
        raise SystemExit("model.json has the wrong output size")
    print(f"convolution: {out_size}x{out_size} cells recomputed, all agree")


if __name__ == "__main__":
    main()
