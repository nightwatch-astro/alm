# Research: Processing Tool Launch

**Spec**: 011-processing-tool-launch | **Date**: 2026-05-20

## R1 — Supported Tools and Profile Boundary

### Decision

V1 ships first-class `ToolProfile` entries for:

| `tool_id`          | Display name        | Role                                  |
| ------------------ | ------------------- | ------------------------------------- |
| `pixinsight`       | PixInsight          | Deep-sky integration / post-processing |
| `siril`            | Siril               | Deep-sky alternative                  |
| `planetary_suite`  | Planetary Suite     | Composite of capture + stack tools    |

`planetary_suite` is itself a profile that points at one preferred binary
(AutoStakkert! by default) but is described to the user as a workflow
bundle. Firecapture is reachable from the same project type via the same
profile when configured; v1 does not expose a separate `firecapture` row
because launching capture software from an archive-stage project is rare.

### Alternatives Considered

- **Per-binary profile for every tool in the workflow**: rejected; the
  user does not think "Open in AutoStakkert" — they think "Open in my
  planetary tools". A profile is the user-facing abstraction.
- **Hardcoded tool list in Rust**: rejected because adding a new tool
  would require a release.
- **User-defined profiles in v1**: deferred; the args-template grammar
  must be tightened first.

### Implications

- `ToolProfile` schema must carry a "label tells more than `executable`"
  story: display name, optional secondary binaries.
- `WorkflowBinding` (which profile a project uses) is owned by the
  project's workflow declaration (spec 009 + future workflow specs);
  this spec only consumes the resolved `tool_id`.

## R2 — Path Discovery

### Decision

Settings is authoritative. Auto-discovery is invoked from **two entry
points** (A2): (a) the first open of Settings → Tool Workflows, and (b)
the first-run wizard's "Detect tools" step (spec 003). Both pre-fill empty
path fields and tag them "auto-detected". The user must explicitly save to
adopt them. No path is auto-applied without user confirmation.

Discovery heuristics per OS:

| Tool             | Windows                                                              | macOS                                                                | Linux                                                              |
| ---------------- | -------------------------------------------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------ |
| PixInsight       | `%ProgramFiles%\PixInsight\bin\PixInsight.exe`                       | `/Applications/PixInsight.app`                                       | `/opt/PixInsight/bin/PixInsight.sh`, then `$PATH`                  |
| Siril            | `%ProgramFiles%\Siril\bin\siril.exe`                                 | `/Applications/Siril.app`, then `$PATH`                              | `$PATH` lookup for `siril`, then `siril-cli`                       |
| Planetary Suite  | `%ProgramFiles%\AutoStakkert\AutoStakkert!*.exe` (glob, pick newest) | `/Applications/AutoStakkert.app` if present; else surface "not found" | `$PATH` lookup; otherwise blank                                    |

`$PATH` lookup is always tried as a last resort. Windows registry probing
is deferred: `%ProgramFiles%` + a small set of fallbacks covers the
common cases without pulling in the `winreg` crate.

### Alternatives Considered

- **Settings-only, no discovery**: too unfriendly for the P1 happy path.
- **Auto-apply discovered paths**: violates "no silent configuration"
  and would mask the case where the user wants a non-standard install.
- **Registry / Spotlight indexing**: deferred; too much surface area for
  v1.

### Implications

- The auto-detect function is a pure read against the filesystem; it
  must be safe to run repeatedly with no side effects.
- A "Re-run auto-detect" button in Settings lets the user refresh
  candidate paths after installing a tool.

## R3 — Argument Templates and Working Directory

### Decision

Each `ToolProfile` carries an `args_template: string[]` where each entry
is a literal string or a token from a closed vocabulary:

| Token       | Meaning                                                                    |
| ----------- | -------------------------------------------------------------------------- |
| `{folder}`  | Project source-view folder (preferred) or project root.                    |
| `{file}`    | Optional single-file argument (used by Siril `.ssf` scripts in a future spec); omitted in v1. |

Unknown tokens cause profile validation to fail at load time. Tool-by-tool
shape:

| Tool             | `args_template`        | `supports_open_folder` | Notes                                                              |
| ---------------- | ---------------------- | ---------------------- | ------------------------------------------------------------------ |
| PixInsight       | `["{folder}"]`         | `true`                 | PixInsight opens the folder in the file explorer panel.            |
| Siril            | `[]`                   | `false`                | Siril does not accept a folder arg. Sets a bundle id, so no `cwd`. |
| Planetary Suite  | `[]`                   | `false`                | AutoStakkert! opens its own dialog; `cwd` anchors it.              |

Where a working directory is applied at all it is the resolved project working
folder, regardless of whether the folder token is present in `args_template`.
This is the "set cwd, optionally pass folder" behaviour FR-009 requires. The
macOS `open -b` bundle-id arm applies none: Launch Services starts the app, so
the caller's `current_dir` would set the cwd of `open` rather than of the tool.

### Alternatives Considered

- **Free-form shell command string**: rejected for v1 due to quoting and
  injection risk; the closed token vocabulary keeps audit-vector hashing
  stable.
- **Pass `--open` style flags**: tool-specific; the template grammar
  already covers this without naming flags in the use case.

### Implications

- Profile validation runs on Settings save and on app start; invalid
  profiles surface as a Settings warning rather than crashing the use
  case.
- The audit record hashes the rendered argument vector so future
  regression tests can detect silent template drift.

## R4 — Detach Model and "Launched From Here" Tracking

### Decision

Spawned processes are detached so closing the desktop app never kills the
tool. The app records:

- `pid` (Option) at spawn time. On Windows we have it directly; on macOS
  via `open -a` we may not, so the field is nullable.
- `launched_at` timestamp.
- `project_id`, `tool_id`.

We do not poll the PID to know when the tool exits. Instead, spec 012
will observe artifacts emitted *under known folders* (project source
view, project root, or a per-tool drop folder). The launch row stores
enough context to correlate any artifact appearing under those folders
back to a `launch_id`.

Re-launch policy: if the most recent `ToolLaunch` for a (project, tool)
pair has no `completed_at` and its `pid` is still alive (best-effort
check), the UI warns "Tool already running for this project" before
spawning a second instance. The user can proceed; we record the second
launch as a separate row.

### Alternatives Considered

- **Tight process supervision**: violates constitution III (the
  application would start to look like a process orchestrator for
  PixInsight). Rejected.
- **Heuristic "back-to-app" signal via OS focus events**: too fragile
  across platforms; deferred to spec 012 if needed.
- **No persisted launch record**: would block spec 012 entirely.

### Implications

- `ToolLaunch.completed_at` is not written by this spec; spec 012 owns its
  update.
- The `launch_id` is the stable handle spec 012 will consume.

## R5 — macOS Quarantine and Translocation (R-MacQuarantine)

### Decision

macOS quarantine/translocation is treated as a user responsibility to
resolve, not a setup-time check. If `open -b <bundle_id>` returns a
translocation or quarantine error (detected via a non-zero exit code combined
with a recognisable stderr pattern such as "is damaged" or translocation path
prefix), the app surfaces a notification:

> "macOS quarantined this app. Run
> `xattr -dr com.apple.quarantine <path>` and retry."

The notification is advisory; the error code `macos.quarantine.detected` is
returned in the launch response (not always fatal — the user may retry after
resolving quarantine). The app does NOT attempt to remove quarantine
attributes itself.

### Alternatives Considered

- **Pre-flight quarantine check**: too invasive; would require scanning all
  `.app` bundles at settings-open time. Deferred.
- **Silent ignore**: violates constitution II (non-silent mutation boundary).

### Implications

- `macos.quarantine.detected` is an advisory error code added to the
  `tool.launch` contract errors enum.
- O3 (macOS quarantine) from the original open questions is resolved by this
  research decision.
