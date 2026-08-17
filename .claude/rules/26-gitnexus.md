---
paths:
  - "**/*"
---

# GitNexus first for structural code questions

This repo is indexed by GitNexus (MCP server `gitnexus`; graph auto-refreshed
after commits). Default to it over grep for anything structural:

- FIRST tool for "where is X / how does X work": `query({search_query})` —
  execution-flow-grouped results. Use `rg` only for exact text/path lookups.
- Before modifying any function/class/method: `impact({target, direction:
  "upstream"})`; warn the user on HIGH/CRITICAL blast radius.
- Symbol relationships: `context({name})`; connect two points: `trace`.
- Before committing: `detect_changes()` (branch review: `{scope: "compare",
  base_ref: "main"}`).
- Rename via `rename` (call-graph aware), never find-and-replace.

Worktrees: the graph lives in the primary checkout; MCP answers from there
(near-main — fine for architecture). Stale index: `gitnexus analyze` (~100s).
