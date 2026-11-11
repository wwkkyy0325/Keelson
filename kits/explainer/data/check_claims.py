"""Check the claim ledger behind the narration.

Every factual sentence the film asserts has to point at evidence: a quote, a
publisher, an access date, and enough independence that one blog repeating
another is not "two sources". Interpretations name the framework they borrow;
value judgments are labelled as opinion.

    python data/check_claims.py

The ledger lives in `claims.json`; a scene names the claims it uses, so the
chain is claim -> source -> scene -> narration.
"""

import json
import re
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parent.parent


def domain(url: str) -> str:
    return urlparse(url).netloc.lower()


def main() -> int:
    ledger_path = ROOT / "claims.json"
    if not ledger_path.exists():
        print("claims: no claims.json (the film makes no factual claims)")
        return 0
    ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
    board = json.loads((ROOT / "storyboard.json").read_text(encoding="utf-8"))
    scene_ids = {scene["id"] for scene in board["scenes"]}
    errors = []
    warnings = []
    seen = set()
    counts = {"fact": 0, "interpretation": 0, "value": 0}
    conflicts = 0

    for claim in ledger.get("claims", []):
        claim_id = claim.get("id", "?")
        if claim_id in seen:
            errors.append(f"{claim_id}: duplicate id")
        seen.add(claim_id)
        kind = claim.get("type")
        if kind not in counts:
            errors.append(f"{claim_id}: type must be fact, interpretation or value")
            continue
        counts[kind] += 1
        sources = claim.get("sources", [])

        for source in sources:
            for field in ("title", "url", "quote", "accessed"):
                if not str(source.get(field, "")).strip():
                    errors.append(f"{claim_id}: a source is missing `{field}`")

        domains = {domain(source.get("url", "")) for source in sources if source.get("url")}
        primary = [source for source in sources if source.get("kind") == "primary"]
        if kind == "fact":
            if not sources:
                errors.append(f"{claim_id}: a fact needs at least one source")
            if len(domains) < 2 and not primary:
                errors.append(
                    f"{claim_id}: a fact needs two independent domains or a primary source"
                )
            if claim.get("status") == "unverified":
                errors.append(f"{claim_id}: an unverified fact must be cut, not shown")
        if kind == "interpretation" and not str(claim.get("basis", "")).strip():
            errors.append(f"{claim_id}: an interpretation must name its framework or author")
        if kind == "value" and not str(claim.get("label", "")).strip():
            errors.append(f"{claim_id}: a value judgment must carry an on-screen label")

        if claim.get("status") == "conflicted":
            conflicts += 1
            if not str(claim.get("conflict", "")).strip():
                errors.append(f"{claim_id}: a conflicted claim must say how the sources differ")
            if len(sources) < 2:
                errors.append(f"{claim_id}: a conflict needs both sides cited")
        if claim.get("status") == "unverified" and kind != "fact":
            warnings.append(f"{claim_id}: unverified and labelled as such")

        for scene_id in claim.get("scenes", []):
            if scene_id not in scene_ids:
                errors.append(f"{claim_id}: scene `{scene_id}` is not in the storyboard")

    scene_claims = {
        scene["id"]: set(scene.get("claims", [])) for scene in board["scenes"]
    }
    for scene in board["scenes"]:
        for claim_id in scene.get("claims", []):
            if claim_id not in seen:
                errors.append(f"scene {scene['id']}: unknown claim `{claim_id}`")

    # The ledger and the scenes must agree in both directions.
    for claim in ledger.get("claims", []):
        for scene_id in claim.get("scenes", []):
            if scene_id in scene_claims and claim["id"] not in scene_claims[scene_id]:
                errors.append(
                    f"{claim['id']}: says it is used in scene {scene_id}, "
                    "but that scene does not name it"
                )

    # A quote is verbatim; an ellipsis hides what was cut.
    for claim in ledger.get("claims", []):
        for source in claim.get("sources", []):
            quote = str(source.get("quote", ""))
            if "……" in quote or "..." in quote:
                errors.append(f"{claim['id']}: a quote is stitched with an ellipsis")

    # Duration is measured from the audio, never typed into the storyboard.
    raw = (ROOT / "storyboard.json").read_text(encoding="utf-8")
    if re.search(r'"(duration|duration_ms|seconds)"', raw):
        errors.append("storyboard: a duration is typed in; it must come from the audio")

    # An on-screen term must be said in that scene.
    for scene in board["scenes"]:
        headline = scene.get("headline", "")
        if isinstance(headline, list):
            headline = " ".join(str(part) for part in headline)
        onscreen = " ".join(
            str(part)
            for part in (scene.get("kicker", ""), headline, scene.get("visual", ""))
        )
        for entry in board.get("glossary", []):
            if isinstance(entry, dict):
                term = entry.get("term")
                scenes = entry.get("scenes")
            else:
                term = entry
                scenes = None
            if not isinstance(term, str) or not term:
                continue
            if scenes is not None and scene["id"] not in scenes:
                continue
            if term in onscreen and term not in scene.get("narration", ""):
                errors.append(
                    f"scene {scene['id']}: on-screen `{term}` is not in the narration"
                )

    # A scene that shows a fact names its publisher on screen.
    for scene in board["scenes"]:
        kinds = {
            claim.get("type")
            for claim in ledger.get("claims", [])
            if claim["id"] in scene_claims.get(scene["id"], set())
        }
        if "fact" in kinds and not str(scene.get("source_card", "")).strip():
            errors.append(
                f"scene {scene['id']}: shows a fact without a `source_card`"
            )
    unused = [
        claim["id"]
        for claim in ledger.get("claims", [])
        if not any(claim["id"] in claims for claims in scene_claims.values())
    ]
    for claim_id in unused:
        warnings.append(f"{claim_id}: in the ledger but no scene uses it")

    for warning in warnings:
        print(f"WARN  {warning}")
    for error in errors:
        print(f"FAIL  {error}")
    print(
        f"claims: {counts['fact']} fact, {counts['interpretation']} interpretation, "
        f"{counts['value']} value, {conflicts} conflict(s)"
    )
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
