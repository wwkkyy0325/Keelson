"""Derive data/model.json from data/spec.json.

`spec.json` is the only hand-written data: the input image and the kernel.
Every number a scene shows comes from `model.json`, which this script computes.
Scenes never do arithmetic; they only draw.

    python data/build.py
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main():
    spec = json.loads((ROOT / "data" / "spec.json").read_text(encoding="utf-8"))
    image = spec["input"]
    kernel = spec["kernel"]
    size = len(image)
    k = len(kernel)
    if any(len(row) != size for row in image):
        raise SystemExit("the input is not square")
    if any(len(row) != k for row in kernel):
        raise SystemExit("the kernel is not square")
    if k % 2 == 0:
        raise SystemExit("the kernel should have an odd size")
    out_size = size - k + 1

    output = []
    for r in range(out_size):
        row = []
        for c in range(out_size):
            row.append(
                sum(
                    image[r + dr][c + dc] * kernel[dr][dc]
                    for dr in range(k)
                    for dc in range(k)
                )
            )
        output.append(row)

    window = spec["window"]
    model = {
        "size": size,
        "kernelSize": k,
        "outputSize": out_size,
        "input": image,
        "kernel": kernel,
        "output": output,
        "window": {
            "r": window["r"],
            "c": window["c"],
            "value": output[window["r"]][window["c"]],
        },
    }
    (ROOT / "data" / "model.json").write_text(
        json.dumps(model, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(
        f"{size}x{size} input, {k}x{k} kernel -> {out_size}x{out_size} feature map; "
        f"window ({window['r']}, {window['c']}) = {model['window']['value']}"
    )


if __name__ == "__main__":
    main()
