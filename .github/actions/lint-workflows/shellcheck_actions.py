"""Shellcheck the bash `run:` blocks of composite actions.

actionlint can't parse composite action manifests (rhysd/actionlint#46),
so their scripts are extracted and batched through one shellcheck
invocation here, with `${{ }}` expressions masked the way actionlint
masks them in workflow scripts.
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
    for manifest in sorted(Path(".github/actions").glob("*/action.y*ml")):
        parsed = subprocess.run(
            ["yq", "-o=json", ".", manifest], check=True, text=True, stdout=subprocess.PIPE
        )
        for i, step in enumerate(json.loads(parsed.stdout)["runs"]["steps"]):
            if step.get("shell") == "bash":
                script = out / f"{manifest.parent.name}-{i}.sh"
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
