# Data Model: Processing Tool Launch

**Spec**: 011-processing-tool-launch | **Date**: 2026-05-22

## ToolProfile

Seeded by `crates/workflow/profiles/`; mutable fields (`executable_path`,
`enabled`) are stored in settings and joined at read time.

```
ToolProfile {
  id:                   String          // stable tool_id, e.g. "pixinsight"
                                        // MUST match [a-z0-9_]+ (C2)
  name:                 String          // display name
  executable_path:      String?         // user-configured; null when unset
  bundle_id:            String?         // macOS only; null on Windows/Linux (R-BundleId)
                                        // seed values: pixinsight="com.pixinsight.PixInsight"
                                        //              siril="org.free-astro.siril"
                                        //              startools="com.startools.startools"
                                        //              astropixelprocessor="com.astropixelprocessor.app"
  args_template:        String[]        // closed-vocabulary tokens; see R3
  supports_open_folder: bool            // whether {folder} is meaningful as an arg
  enabled:              bool            // settings flag; false hides the CTA
  auto_detected:        bool            // true when path came from R2 discovery
                                        // and the user has not yet saved an override
  platform_hints?: {
    detach_strategy:    DetachStrategy  // "spawn_detached" | "open_bundle_id" | "setsid"
  }
}
```

```
DetachStrategy = "spawn_detached"   // Windows default
               | "open_bundle_id"   // macOS .app bundles — uses "open -b <bundle_id>" (A1)
               | "setsid"           // Linux + macOS plain binaries
```

### macOS launch rule (R-BundleId)

- If `bundle_id` is set: `open -b <bundle_id> --args <rendered_argv>`. No
  working directory is applied, and none can be: Launch Services starts the
  app, so the caller's `current_dir` would set the cwd of `open` rather than
  of the tool. `open -b` also does not report the child PID.
- If `bundle_id` is null: fall back to direct executable + `setsid`-style
  detach via `pre_exec`.
- On quarantine/translocation error from `open -b`: surface R-MacQuarantine
  notification (see `research.md` R5).

### Invariants

- `id` MUST match `[a-z0-9_]+`. Derivation rule: lowercase source name,
  remove spaces. Reject identifiers with other characters (C2).
- `args_template` may only contain literal strings and tokens from the
  closed set `{folder}`, `{file}`.
- `supports_open_folder == false` ⇒ `args_template` MUST NOT include
  `{folder}`.
- `enabled` is settings-owned; static profile rows always carry the
  current Settings join in memory before the use case reads them.
- `executable_path` is normalised to an absolute path on save; relative
  paths are rejected.
- `bundle_id` is nullable; null on non-macOS platforms and when not
  configured for a macOS tool.

## ToolLaunch

Persisted in SQLite via the `crates/persistence/` crates; one row per launch
attempt (success or failure that reached the spawn step).

```
ToolLaunch {
  id:            Uuid              // launch_id surfaced to spec 012
  project_id:    Uuid
  tool_id:       String            // FK to ToolProfile.id
  launched_at:   Timestamp
  pid:           u32?              // null when OS does not surface it
  completed_at:  Timestamp?        // reserved for spec 012; v1 always null
  outcome:       LaunchOutcome
  audit_id:      Uuid              // back-reference to crates/audit entry
  working_dir:   String            // resolved cwd that was passed to the child
  args_hash:     String            // BLAKE3(canonicalized_executable_path || rendered_argv)
                                   // executable_path canonicalized before hashing;
                                   // argv rendered with consistent separator (R-Hash-Exec)
}
```

```
LaunchOutcome = "spawned"
              | "spawn_failed"      // OS error before/at spawn
              | "tool_not_configured"
              | "executable_not_found"
```

### Invariants

- `outcome == "spawned"` ⇒ `pid` may be null (macOS `open -a`) but
  `working_dir` and `args_hash` MUST be present.
- `outcome != "spawned"` ⇒ `pid` is null; `working_dir` and `args_hash`
  MAY be null (when failure happened before render).
- `completed_at` is not written by this spec; spec 012 owns its update.

## WorkflowBinding (external reference)

This spec consumes — and does not redefine — the project's workflow
binding, which is owned by future workflow specs. For now it is read as:

```
WorkflowBinding {
  project_id:  Uuid
  tool_id:     String              // matches ToolProfile.id
}
```

Resolution rule used by the launch use case: each project carries a
canonical `tool` field (existing in spec 009 `Project`). The launch use
case treats `Project.tool` as the `tool_id` lookup key, lower-cased and
namespaced (`"PixInsight"` → `"pixinsight"`). When a future spec
introduces per-project multi-tool bindings, the CTA dispatch will pass
the chosen `tool_id` explicitly and this string fallback is dropped.

## Derived Views

### ToolProfileSummary (returned by `tool.profile.list`)

```
ToolProfileSummary {
  id:                   String
  name:                 String
  configured:           bool          // executable_path is set and not blank
  available:            bool          // configured AND file exists at scan time
  supports_open_folder: bool
  auto_detected:        bool
}
```

`configured` is a settings-state read; `available` is a filesystem read
that the Settings UI debounces. The launch use case does its own
freshness check at dispatch time and does not trust this view.

## Storage Notes

- `tool_launches` table indexed on `(project_id, launched_at desc)` for
  the "Tool launches" project-detail section (mockup already lays out
  this group, see `ProjectsPage.tsx` line 431-433).
- `tool_profiles` settings rows live in the existing settings table
  (spec 018) under namespace `tool_workflows`. The static profile seed
  is held in code and merged on read.
- `args_hash` and `working_dir` are written before audit so the audit
  entry can reference an existing row.
