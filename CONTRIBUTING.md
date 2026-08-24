# Contributing

Contributions are welcome! By submitting a pull request, you agree to the terms
below.

## Contributor License Agreement (CLA)

This project requires a CLA for all contributions. When you open your first pull
request, the CLA Assistant bot will comment with a link to review and sign the
agreement. You only need to sign once — it covers all future contributions to
this repository.

**Why?** The CLA ensures the project maintainer retains the ability to offer the
code under alternative licensing terms (e.g., a commercial license) without
needing to track down every contributor individually. Your contribution remains
publicly available under the project's open-source license (AGPL-3.0) regardless.

## How to contribute

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Ensure tests pass (if applicable)
5. Open a pull request with a clear description of the change

## Verifying gates

A green `pre-commit` run is not by itself evidence that your files were checked,
and neither is a clean commit.

- `pre-commit run --files <paths>` exits 0 when no hook looked at any of those
  paths. The global `exclude:` in `.pre-commit-config.yaml` drops whole trees
  (including `specs/`, `.claude/` and `.specify/`) from every hook's file list,
  and the run then reports success having checked nothing. Use
  `just precommit <paths>` instead: it fails when no hook ran and prints how
  many did.
- Some machines set `core.hooksPath` outside the repository, in which case git
  never runs this repo's pre-commit hooks at all and a successful commit proves
  nothing. Check with `git config --get core.hooksPath`.

So when you report a check as passing, say which hooks ran, not which command
exited 0. When you review, ask for that.

## Code of conduct

Be respectful. Contributions are evaluated on technical merit.
