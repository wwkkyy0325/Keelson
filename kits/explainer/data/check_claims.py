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

    for scene in board["scenes"]:
        for claim_id in scene.get("claims", []):
            if claim_id not in seen:
                errors.append(f"scene {scene['id']}: unknown claim `{claim_id}`")

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
