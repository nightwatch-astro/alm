# Tracing conventions

## Structured fields over format-string interpolation

Use named fields instead of interpolating values into the message string:

```rust
// preferred
tracing::warn!(project_id, error = %e, "manifest write failed");

// avoid
tracing::warn!("manifest write failed for project {project_id}: {e}");
```

**Why**: named fields are indexed and filterable in structured log viewers
(`tracing-subscriber` JSON format, etc.).  A static message string also
makes log aggregation and alerting easier — the message never changes between
invocations, only field values do.

## Field naming

| Value type | Convention | Example |
|---|---|---|
| Error / display | `error = %e` | `tracing::warn!(error = %e, "…")` |
| Debug-formatted | `error = ?e` | for types without `Display` |
| Plain string or `&str` | bare field or `= value` | `tracing::info!(project_id, …)` |
| Formatted display (path, URL) | `name = %value.display()` | `dir = %dir.display()` |

## Lint / ratchet

There is no automated clippy rule that catches tracing interpolation today.
To verify the codebase has not regressed, run:

```sh
rg 'tracing::(info|warn|debug|error|trace)!\(".*\{[^:{}]' crates/
```

A clean result means no format-string interpolation remains in tracing calls.
