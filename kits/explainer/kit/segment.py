"""Cut a headshot into the parts a rig needs.

The output is a set of RGBA cut-outs that tile the subject exactly once, plus
the anchors where their bones sit. Nothing is invented: every pixel comes from
the reference. The parts are found by colour and geometry — a studio headshot
has separable skin, hair and clothing — so the reference should be frontal and
plainly lit.

    python kit/segment.py assets/portrait.jpg out/portrait

Writes `out/portrait/parts/<name>.png` and `out/portrait/parts.json`.
"""

from __future__ import annotations

import json
import os
import sys

import cv2
import numpy as np

PARTS = ["hair", "face", "nose", "eye-l", "eye-r", "brow-l", "brow-r",
         "mouth", "neck", "body"]


def foreground(image: np.ndarray) -> np.ndarray:
    height, width = image.shape[:2]
    corners = np.concatenate([
        image[2:16, 2:16].reshape(-1, 3),
        image[2:16, width - 16:width - 2].reshape(-1, 3),
    ]).astype(np.float32)
    background = corners.mean(axis=0)
    distance = np.linalg.norm(image.astype(np.float32) - background, axis=2)
    mask = (distance > 44).astype(np.uint8) * 255
    mask = cv2.morphologyEx(mask, cv2.MORPH_OPEN, np.ones((3, 3), np.uint8))
    mask = cv2.morphologyEx(mask, cv2.MORPH_CLOSE, np.ones((5, 5), np.uint8))
    count, labels, stats, _ = cv2.connectedComponentsWithStats(mask, 8)
    if count <= 1:
        raise SystemExit("no subject found: the background is not separable")
    largest = 1 + int(np.argmax(stats[1:, cv2.CC_STAT_AREA]))
    mask = (labels == largest).astype(np.uint8) * 255
    return cv2.erode(mask, np.ones((3, 3), np.uint8), iterations=1)


def neck_row(mask: np.ndarray) -> int:
    ys, xs = np.nonzero(mask)
    top, bottom = int(ys.min()), int(ys.max())
    best_row, best_span = (top + bottom) // 2, 1 << 30
    for y in range(int(top + (bottom - top) * 0.3), int(top + (bottom - top) * 0.62)):
        row = np.nonzero(mask[y])[0]
        if row.size == 0:
            continue
        span = int(row.max() - row.min())
        if span < best_span:
            best_row, best_span = y, span
    return best_row


def blobs(mask: np.ndarray, minimum: int) -> list[tuple[int, int, int, int]]:
    count, _, stats, _ = cv2.connectedComponentsWithStats(mask, 8)
    found = []
    for index in range(1, count):
        area = int(stats[index, cv2.CC_STAT_AREA])
        if area < minimum:
            continue
        found.append((
            int(stats[index, cv2.CC_STAT_LEFT]),
            int(stats[index, cv2.CC_STAT_TOP]),
            int(stats[index, cv2.CC_STAT_WIDTH]),
            int(stats[index, cv2.CC_STAT_HEIGHT]),
        ))
    found.sort(key=lambda box: box[2] * box[3], reverse=True)
    return found


def ellipse_mask(shape, cx, cy, rx, ry) -> np.ndarray:
    mask = np.zeros(shape[:2], np.uint8)
    cv2.ellipse(mask, (int(cx), int(cy)), (int(rx), int(ry)), 0, 0, 360, 255, -1)
    return mask


def largest_component(mask: np.ndarray) -> np.ndarray:
    count, labels, stats, _ = cv2.connectedComponentsWithStats(mask, 8)
    if count <= 1:
        return np.zeros_like(mask)
    largest = 1 + int(np.argmax(stats[1:, cv2.CC_STAT_AREA]))
    return (labels == largest).astype(np.uint8) * 255


def segment(image: np.ndarray) -> tuple[np.ndarray, dict]:
    height, width = image.shape[:2]
    subject = foreground(image)
    cut = neck_row(subject)
    head_zone = subject.copy()
    head_zone[cut:] = 0

    ycc = cv2.cvtColor(image, cv2.COLOR_BGR2YCrCb)
    _, cr, cb = cv2.split(ycc)
    skin = ((cr > 133) & (cr < 180) & (cb > 75) & (cb < 135)).astype(np.uint8) * 255
    skin = cv2.bitwise_and(skin, head_zone)
    skin = cv2.morphologyEx(skin, cv2.MORPH_OPEN, np.ones((5, 5), np.uint8))
    face = largest_component(skin)
    if cv2.countNonZero(face) < 500:
        raise SystemExit("no face found: the skin range did not match")
    skin_source = face.copy()

    # The skin component runs from the forehead down the neck, so its box
    # bottom is not the chin. The chin is where the row width falls away
    # from the cheek line; the neck is everything below it.
    ys = np.nonzero(face.any(axis=1))[0]
    top, bottom = int(ys.min()), int(ys.max())
    widths = [int((face[y] > 0).sum()) for y in range(top, bottom + 1)]
    widest = max(widths)
    peak = top + widths.index(widest)
    chin = bottom
    for y in range(peak, bottom + 1):
        if widths[y - top] < widest * 0.6:
            chin = y
            break
    chin = min(chin, cut - 8)

    face[chin:, :] = 0
    fx, fy, fw, fh = cv2.boundingRect(face)

    grey = cv2.cvtColor(image, cv2.COLOR_BGR2GRAY)
    dark = (grey < 85).astype(np.uint8) * 255

    def band(top: float, bottom: float) -> np.ndarray:
        mask = np.zeros((height, width), np.uint8)
        mask[int(fy + fh * top):int(fy + fh * bottom), fx:fx + fw] = 255
        return mask

    eyes_zone = cv2.bitwise_and(dark, band(0.30, 0.55))
    eyes_zone = cv2.morphologyEx(eyes_zone, cv2.MORPH_OPEN, np.ones((3, 3), np.uint8))
    eye_boxes = [box for box in blobs(eyes_zone, 60)][:2]
    eye_boxes.sort(key=lambda box: box[0])

    brows_zone = cv2.bitwise_and(dark, band(0.16, 0.33))
    brows_zone = cv2.morphologyEx(brows_zone, cv2.MORPH_OPEN, np.ones((3, 3), np.uint8))
    brow_boxes = [box for box in blobs(brows_zone, 60)][:2]
    brow_boxes.sort(key=lambda box: box[0])

    mouth_zone = cv2.bitwise_and(((cr > 150) & (cr.astype(np.int16) - cb.astype(np.int16) > 18)).astype(np.uint8) * 255, band(0.60, 0.84))
    mouth_zone = cv2.morphologyEx(mouth_zone, cv2.MORPH_OPEN, np.ones((5, 5), np.uint8))
    mouth_boxes = blobs(mouth_zone, 200)[:1]

    labels = np.zeros((height, width), np.uint8)  # 0 empty
    names = [None] + PARTS
    index = {name: i + 1 for i, name in enumerate(PARTS)}

    def put(mask: np.ndarray, name: str, grow: int = 0) -> None:
        if grow:
            mask = cv2.dilate(mask, np.ones((grow * 2 + 1, grow * 2 + 1), np.uint8))
        labels[(mask > 0) & (labels == 0)] = index[name]

    # Eyes, brows and mouth first: they sit inside the face and must own their
    # pixels before the face takes them.
    for name, boxes, grow in (
        ("eye-l", eye_boxes[:1], 3),
        ("eye-r", eye_boxes[1:2], 3),
        ("brow-l", brow_boxes[:1], 3),
        ("brow-r", brow_boxes[1:2], 3),
        ("mouth", mouth_boxes, 3),
    ):
        for (bx, by, bw, bh) in boxes:
            put(cv2.rectangle(np.zeros((height, width), np.uint8), (bx, by), (bx + bw, by + bh), 255, -1), name, grow)

    # Nose: between the eyes and the mouth, centred on the face. It is a
    # geometric piece because skin has one colour there; it is small enough
    # that a subtle bone is all it needs.
    nose_cx = fx + fw * 0.5
    nose_cy = fy + fh * 0.47
    put(ellipse_mask(image.shape, nose_cx, nose_cy, fw * 0.10, fh * 0.11), "nose")

    put(face, "face")

    hair = head_zone.copy()
    hair[labels > 0] = 0
    hair[chin:] = 0
    put(hair, "hair")

    neck_zone = np.zeros((height, width), np.uint8)
    neck_zone[chin:cut] = 255
    neck = cv2.bitwise_and(skin_source, neck_zone)
    if cv2.countNonZero(neck) < 200:
        neck = cv2.bitwise_and(subject, neck_zone)
    put(neck, "neck")

    body = subject.copy()
    body[:cut] = 0
    body[labels > 0] = 0
    put(body, "body")

    for name in PARTS:
        mask = (labels == index[name]).astype(np.uint8) * 255
        if cv2.countNonZero(mask) < 200:
            raise SystemExit(f"part `{name}` came out empty")
        labels[labels == index[name]] = index[name]

    return labels, {"neck": int(cut), "face_box": [int(v) for v in (fx, fy, fw, fh)]}


def write_pieces(image: np.ndarray, labels: np.ndarray, out_dir: str) -> dict:
    os.makedirs(os.path.join(out_dir, "parts"), exist_ok=True)
    pieces = []
    for index, name in enumerate(PARTS, start=1):
        mask = (labels == index)
        if not mask.any():
            continue
        rgba = np.dstack([image, np.where(mask, 255, 0).astype(np.uint8)])
        path = os.path.join(out_dir, "parts", name + ".png")
        cv2.imwrite(path, rgba)
        ys, xs = np.nonzero(mask)
        pieces.append({
            "name": name,
            "file": "parts/" + name + ".png",
            "anchor": [float(xs.mean()), float(ys.mean())],
            "box": [int(xs.min()), int(ys.min()), int(xs.max() - xs.min()), int(ys.max() - ys.min())],
        })
    return {"parts": pieces}


def main() -> None:
    if len(sys.argv) < 3:
        raise SystemExit("usage: segment.py <image> <out_dir>")
    image = cv2.imread(sys.argv[1], cv2.IMREAD_COLOR)
    if image is None:
        raise SystemExit(f"cannot read {sys.argv[1]}")
    labels, meta = segment(image)
    document = write_pieces(image, labels, sys.argv[2])
    document.update(meta)
    with open(os.path.join(sys.argv[2], "parts.json"), "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2)
    print(json.dumps({"parts": [p["name"] for p in document["parts"]], "neck": meta["neck"]}))


if __name__ == "__main__":
    main()
