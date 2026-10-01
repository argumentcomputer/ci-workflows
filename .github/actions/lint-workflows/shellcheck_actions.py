"""Shellcheck the bash `run:` blocks of composite actions.

actionlint can't parse composite action manifests (rhysd/actionlint#46),
so their scripts are extracted and batched through one shellcheck
invocation here, with `${{ }}` expressions masked the way actionlint
masks them in workflow scripts. Covers the actions under `.github/actions`
and a repository that is itself an action, i.e. has `action.yml` at its
root; a manifest without `runs.steps` (a JavaScript or Docker action) has
no scripts and is skipped.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path


def main():
    severity = os.environ.get("SEVERITY", "warning")
    out = Path(tempfile.mkdtemp())
    scripts = []
    manifests = sorted(Path(".").glob("action.y*ml")) + sorted(
        Path(".github/actions").glob("*/action.y*ml")
    )
    for manifest in manifests:
        parsed = subprocess.run(
            ["yq", "-o=json", ".", manifest], check=True, text=True, stdout=subprocess.PIPE
        )
        steps = json.loads(parsed.stdout).get("runs", {}).get("steps") or []
        for i, step in enumerate(steps):
            if step.get("shell") == "bash":
                script = out / f"{manifest.parent.resolve().name}-{i}.sh"
                script.write_text(re.sub(r"\$\{\{.*?\}\}", "EXPR", step["run"]))
                scripts.append(script)
    if scripts:
        sys.exit(
            subprocess.run(
                ["shellcheck", f"--severity={severity}", "--shell=bash", *scripts]
            ).returncode
        )


if __name__ == "__main__":
    main()
