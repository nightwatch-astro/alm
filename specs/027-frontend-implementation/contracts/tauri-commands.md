# Frontend → Backend Contract: Tauri Commands

The frontend invokes Tauri commands via `@tauri-apps/api` invoke(). Each command maps to a Rust function in the backend. During frontend development, these are mocked in `src/api/mocks.ts`.

This document defines the command interface that the frontend depends on. The actual Rust implementations are defined in backend specs (002-026).

## Query Commands (read-only)

| Command | Args | Returns | Used By |
|---------|------|---------|---------|
| `sessions.list` | `{ filters?, sort?, group_by? }` | `Session[]` | Sessions page |
| `sessions.get` | `{ id }` | `SessionDetail` | Session detail |
| `sessions.calendar` | `{ start_month, end_month }` | `CalendarData` | Calendar view |
| `calibration.masters.list` | `{ group_by?, filters? }` | `CalibrationMaster[]` | Calibration page |
| `calibration.masters.get` | `{ id }` | `MasterDetail` | Master detail |
| `calibration.matches` | `{ session_id }` | `MatchCandidate[]` | Session detail, wizard |
| `targets.list` | `{ search? }` | `Target[]` | Targets page |
| `targets.get` | `{ id }` | `TargetDetail` | Target detail |
| `projects.list` | `{ filters? }` | `Project[]` | Projects page |
| `projects.get` | `{ id }` | `ProjectDetail` | Project detail |
| `plans.list` | `{ filters? }` | `FilesystemPlan[]` | Plans page |
| `plans.get` | `{ id }` | `PlanDetail` | Plan review |
| `audit.list` | `{ filters?, pagination? }` | `{ entries: AuditEntry[], total }` | Audit page |
| `audit.export` | `{ filters? }` | `string` (JSONL content) | Audit export |
| `settings.get` | `{ scope }` | `SettingsData` | Settings panes |
| `roots.list` | — | `LibraryRoot[]` | Data sources |
| `equipment.list` | — | `Equipment[]` | Equipment settings |
| `review.queue` | `{ filter? }` | `ReviewItem[]` | Review queue |
| `preferences.get` | — | `AppPreferences` | App startup |
| `search.global` | `{ query }` | `SearchResult[]` | Command palette |

## Mutation Commands

| Command | Args | Returns | Used By |
|---------|------|---------|---------|
| `sessions.transition` | `{ id, action, metadata? }` | `Result<Session>` | Review queue, detail |
| `sessions.split` | `{ id, split_at_index }` | `Result<{ original: Session, new: Session }>` | none -- SUPERSEDED with FR-015 |
| `sessions.merge` | `{ ids }` | `Result<Session>` | none -- SUPERSEDED with FR-015 |
| `projects.create_plan` | `{ wizard_state }` | `FilesystemPlan` | Wizard step 6 |
| `plans.approve` | `{ id, delete_acknowledged? }` | `Result<Plan>` | Plan review |
| `plans.apply` | `{ id }` | `OperationHandle` | Plan review |
| `plans.discard` | `{ id }` | `Result<void>` | Plan review |
| `settings.update` | `{ scope, values }` | `Result<void>` | Settings panes |
| `roots.register` | `{ path, category, scan_settings }` | `Result<LibraryRoot>` | Setup, Data sources |
| `roots.remap` | `{ root_id, new_path }` | `Result<RemapVerification>` | Root recovery |
| `roots.remap.apply` | `{ root_id, verified }` | `Result<void>` | Root recovery |
| `scan.start` | `{ root_ids? }` | `OperationHandle` | Setup, Data sources |
| `preferences.set` | `{ key, value }` | `Result<void>` | Density, sidebar, views |
| `tour.complete_step` | `{ step }` | `Result<void>` | Tour |

`sessions.split` and `sessions.merge` are registered fixture stubs returning
hardcoded sessions (spec 029 FR-005, T021). No frontend module invokes either
one, so no product flow depends on their result shapes. Their removal is tracked
outside this specification.

## Native API Commands (Tauri built-in)

| API | Used By | Notes |
|-----|---------|-------|
| `dialog.open({ directory: true })` | DirPicker | Native OS directory picker |
| `window.appWindow.setTitle()` | Shell | Dynamic title bar text |
| `event.listen()` | LogPanel, StatusBar | Progress events from long-running ops |

## Operation Handles (long-running)

Long-running commands return an `OperationHandle` and emit progress events:

```typescript
type OperationHandle = { operation_id: string; kind: string };

// Events emitted via Tauri event system
type ProgressEvent = {
  operation_id: string;
  discovered: number;
  total: number;
  current_item: string;
  elapsed_ms: number;
  warnings: string[];
  completion_state?: 'completed' | 'failed' | 'paused';
};
```

The LogPanel subscribes to all progress events and displays them.
