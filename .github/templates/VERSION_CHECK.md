---
title: "chore: rust toolchain needs an upgrade"
labels: debt, automated-issues
---

The rust version specified in `rust-toolchain.toml` ({{env.TOOLCHAIN_VERSION}}) is out of date with the latest stable ({{env.RUST_VERSION}}).

Check the [rust version check]({{env.WORKFLOW_URL}}) workflow for details.

This issue was raised by the workflow at {{env.WORKFLOW_FILE}}.
