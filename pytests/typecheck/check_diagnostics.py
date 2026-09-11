"""Check each marked rejection, not just the total number of Pyright errors."""

import json
from pathlib import Path
import sys

source = Path(__file__).with_name("check_bad.py")
expected = {
    number
    for number, line in enumerate(source.read_text().splitlines())
    if line.startswith("ext.")
}
report = json.load(sys.stdin)
errors = [
    entry for entry in report["generalDiagnostics"] if entry["severity"] == "error"
]
actual = {entry["range"]["start"]["line"] for entry in errors}
assert len(errors) == len(expected) and actual == expected, (
    f"expected one error at each rejection on lines {sorted(n + 1 for n in expected)}; "
    f"got {errors}"
)
assert all(Path(entry["file"]).resolve() == source.resolve() for entry in errors)
assert all(entry.get("rule") == "reportArgumentType" for entry in errors)
print(f"{len(errors)} expected argument errors at the correct locations")
