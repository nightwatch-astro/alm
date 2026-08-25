# Journey prep plans, August 2026

Working artifacts from a journey validation run. These files are not journey
definitions. Only a journey document under `docs/journeys/` defines a journey's
steps and expectations.

## What a prep plan is

An offline prep unit wrote each `J*-drive-plan.md`. The unit read the journey
document and the product source. It then wrote down what it expects the running
app to do. It never started the app.

**A prep plan is static evidence derived from source without running the app.**
Where a plan and the product disagree, the product is the fact and the plan is
wrong.

No journey has a run record yet:

```
find docs/journeys -path '*/runs/*.md' | wc -l   # 0
```

So a plan records what one reader concluded from source. It does not record an
observed result. Before you rely on a selector or a count, re-derive it.

## Contents

Each drive plan holds four things:

- an inventory of expectations
- a map of selectors
- a recipe for fixtures
- a list of known gaps

| File | Journey |
| --- | --- |
| `J01-drive-plan.md` | `J01-first-run-setup-data-sources` |
| `J02-drive-plan.md` | `J02-ingest-review-reclassify-confirm-move` |
| `J03-drive-plan.md` | `J03-ingest-confirm-catalogue-in-place` |
| `J04-drive-plan.md` | `J04-sessions-review-derived` |
| `J06-drive-plan.md` | `J06-cleanup-scan-review-apply` |

J05 and J07 through J10 have no prep plan.

`triage-astro-plan-6w2v2.md` triages 29 static findings. Earlier work already
turned those findings into beads. Only this report says which severities the
triage corrected, and which items it reclassified as not-findings. That
reasoning is why the file is here.

## Machine-specific values

Placeholders stand in for values that belong to one machine:

- `<repo>` for the repository checkout
- `<journey-host>` for the address of the Windows host
- `<user>` for the Windows account name
- `<scratch>` for the scratch directory

Before you run a command that quotes a placeholder, substitute the real value.
