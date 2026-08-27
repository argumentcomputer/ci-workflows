"""Create or update a GitHub issue via the gh CLI.

Reads its configuration from the INPUT_* environment variables set in
action.yml.
"""

import json
import os
import subprocess
import sys


def gh(*args):
    return subprocess.run(
        ["gh", *args], check=True, text=True, stdout=subprocess.PIPE
    ).stdout


def main():
    repo = os.environ["GITHUB_REPOSITORY"]
    title = os.environ["INPUT_TITLE"]
    labels = os.environ.get("INPUT_LABELS", "")
    update_existing = os.environ.get("INPUT_UPDATE_EXISTING", "true") == "true"

    body = os.environ.get("INPUT_BODY", "")
    body_file = os.environ.get("INPUT_BODY_FILE", "")
    if bool(body) == bool(body_file):
        sys.exit("::error::Set exactly one of `body` and `body-file`")
    if body_file:
        with open(body_file) as f:
            body = f.read()

    issues = json.loads(
        gh("issue", "list", "--repo", repo, "--state", "open",
           "--limit", "100", "--json", "number,title")
    )
    existing = [issue["number"] for issue in issues if issue["title"] == title]
    if existing:
        if update_existing:
            gh("issue", "edit", str(existing[0]), "--repo", repo, "--body", body)
            print(f"Updated existing issue #{existing[0]}")
        else:
            print(f"Open issue #{existing[0]} already exists, skipping")
        return

    url = gh("issue", "create", "--repo", repo, "--title", title, "--body", body).strip()
    print(f"Created issue {url}")
    if labels:
        try:
            gh("issue", "edit", url, "--repo", repo, "--add-label", labels)
        except subprocess.CalledProcessError:
            print(f"::warning::Could not apply labels {labels!r}")


if __name__ == "__main__":
    main()
