// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useState } from 'react';
import { Btn } from '@/ui/Btn';
import { Pill } from '@/ui/Pill';
import { InfoTip } from '@/ui/InfoTip';
import { m } from '@/lib/i18n';
import { useDirectoryPicker } from '@/shared/native';
import type { LastPathKind } from '@/shared/native';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type {
  SourceEntry,
  SourceKind,
  OrganizationState,
} from '../sources-store';
import {
  ALL_SOURCE_KINDS,
  SOURCE_KIND_LABELS,
  REQUIRED_KINDS,
} from '../sources-store';
import { selectBase } from '@/styles/select.css';

export interface StepSourceFoldersProps {
  entries: SourceEntry[];
  onAdd: (path: string, kind: SourceKind) => void;
  onRemove: (index: number) => void;
  onKindChange: (index: number, kind: SourceKind) => void;
  onOrganizationStateChange: (index: number, state: OrganizationState) => void;
  errors: Record<number, string>;
}

/** Map a source kind to the picker's last-path affordance bucket. */
const KIND_TO_LAST_PATH: Record<SourceKind, LastPathKind> = {
  light_frames: 'raw',
  calibration: 'calibration',
  project: 'project',
  inbox: 'inbox',
};

// Per-category explanatory copy for the (?) affordance next to each group
// label (issue #497, #714) — what the category means and how PlateVault
// uses it, including the note that Projects are created later in a guided
// workflow (this folder just registers where to look).
const SOURCE_KIND_HELP: Record<SourceKind, () => string> = {
  light_frames: () => m.setup_kind_light_frames_help(),
  calibration: () => m.setup_kind_calibration_help(),
  project: () => m.setup_kind_project_help(),
  inbox: () => m.setup_kind_inbox_help(),
};

// Required-first display order (issue #496): both required kinds precede
// both optional kinds, otherwise preserving ALL_SOURCE_KINDS' relative order.
// Array.prototype.sort is stable per spec, so this always yields
// [light_frames, project, calibration, inbox].
const ORDERED_SOURCE_KINDS: SourceKind[] = [...ALL_SOURCE_KINDS].sort(
  (a, b) =>
    Number(!REQUIRED_KINDS.includes(a)) - Number(!REQUIRED_KINDS.includes(b)),
);

const REQUIREMENT_SECTIONS = [true, false] as const;

/** Normalize a path for cross-platform prefix/equality comparison. */
function normalizePathForCompare(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

/**
 * Add-time validation across ALL pending sources, independent of kind —
 * folders cannot be nested inside each other (no-overlap rule, #501), and a
 * duplicate must surface a message rather than silently no-op (#502).
 *
 * Existence is checked separately (see `checkPathExists`) since it requires
 * an IPC round-trip; this stays a synchronous, in-memory check.
 */
function findAddTimeConflict(
  entries: SourceEntry[],
  kind: SourceKind,
  path: string,
): string | null {
  const normalized = normalizePathForCompare(path);
  for (const entry of entries) {
    const entryNormalized = normalizePathForCompare(entry.path);
    if (entryNormalized === normalized) {
      return entry.kind === kind
        ? m.setup_sources_error_already_added()
        : m.setup_sources_error_registered_under({
            kind: SOURCE_KIND_LABELS[entry.kind](),
          });
    }
    if (
      normalized.startsWith(`${entryNormalized}/`) ||
      entryNormalized.startsWith(`${normalized}/`)
    ) {
      return m.setup_sources_error_overlaps_existing({ path: entry.path });
    }
  }
  return null;
}

/**
 * Best-effort existence + file-vs-directory check for a manually typed/pasted
 * path (#662, #1056) — the native OS picker already guarantees a real,
 * existing directory, so this only matters for the manual-entry affordance,
 * but runs for both so there is one add-time validation path. Reuses
 * `tools.validate_path` (an existing, side-effect-free `exists() &&
 * is_absolute()` check meant for tool executables, extended with an `isDir`
 * signal) instead of inventing a new backend command. The stricter
 * `roots.register.batch` path validation (`crates/app/core/src/first_run.rs`)
 * still runs, and blocks registration, at Confirm-step flush regardless — this
 * is only meant to surface the error earlier. An IPC failure here is treated
 * as inconclusive (not blocking) for the same reason.
 */
async function checkPathExists(path: string): Promise<string | null> {
  try {
    const result = unwrap(await commands.toolsValidatePath(path));
    if (!result.valid) return m.err_path_not_exists();
    if (result.isDir === false) return m.err_path_not_directory();
    return null;
  } catch {
    return null;
  }
}

/**
 * Step 1 -- Source Folders.
 *
 * One persistent, compact card per source kind. Each card has its own "Add
 * folder" button (type-first by construction) that opens the OS picker and
 * registers the folder under that card's kind. Empty groups collapse to a
 * single header row; required kinds highlight their card to convey met / unmet
 * status. The surrounding wizard still gates "Continue" on
 * getMissingRequiredKinds().
 *
 * Groups render required-first under "Required"/"Optional" section headings
 * (#496); ALL_SOURCE_KINDS still governs which kinds exist and their default
 * order elsewhere (e.g. Confirm step).
 */
export function StepSourceFolders({
  entries,
  onAdd,
  onRemove,
  onOrganizationStateChange,
  errors,
}: StepSourceFoldersProps) {
  // Stable index lookup so per-row remove/advanced map back to the flat array.
  const indexed = entries.map((entry, index) => ({ entry, index }));

  return (
    <div className="pv-step-sources" data-testid="step-sources">
      <p className="pv-step-sources__intro">{m.setup_sources_intro()}</p>

      <div className="pv-step-sources__groups">
        {REQUIREMENT_SECTIONS.map((isRequired) => {
          const sectionName = isRequired ? 'required' : 'optional';
          const headingId = `source-section-${sectionName}-heading`;
          return (
            <section
              key={sectionName}
              className="pv-step-sources__section"
              aria-labelledby={headingId}
            >
              <h2 id={headingId} className="pv-step-sources__section-heading">
                {isRequired
                  ? m.setup_sources_section_required()
                  : m.setup_sources_section_optional()}
              </h2>
              {ORDERED_SOURCE_KINDS.filter(
                (kind) => REQUIRED_KINDS.includes(kind) === isRequired,
              ).map((kind) => (
                <SourceGroup
                  key={kind}
                  kind={kind}
                  rows={indexed.filter(({ entry }) => entry.kind === kind)}
                  allEntries={entries}
                  errors={errors}
                  onAdd={onAdd}
                  onRemove={onRemove}
                  onOrganizationStateChange={onOrganizationStateChange}
                />
              ))}
            </section>
          );
        })}
      </div>
    </div>
  );
}

/** A compact type group card: one header row, plus folder rows only when present. */
function SourceGroup({
  kind,
  rows,
  allEntries,
  errors,
  onAdd,
  onRemove,
  onOrganizationStateChange,
}: {
  kind: SourceKind;
  rows: { entry: SourceEntry; index: number }[];
  allEntries: SourceEntry[];
  errors: Record<number, string>;
  onAdd: (path: string, kind: SourceKind) => void;
  onRemove: (index: number) => void;
  onOrganizationStateChange: (index: number, state: OrganizationState) => void;
}) {
  const isRequired = REQUIRED_KINDS.includes(kind);
  const isMet = rows.length > 0;
  const hasRows = rows.length > 0;
  const [addError, setAddError] = useState<string | null>(null);
  const headingId = `source-group-${kind}-heading`;

  // Add-time validation (#502, #662): reject duplicates/overlaps and a
  // nonexistent path here, before the path ever reaches the wizard's
  // registered-sources buffer, so the user gets immediate, accessible
  // feedback instead of a silent no-op or a failure buried in the Confirm
  // step's aggregate registration banner. Returns whether the add succeeded
  // so callers (e.g. the manual-entry input) know whether to clear their
  // local buffer.
  const handleAdd = async (
    path: string,
    addKind: SourceKind,
  ): Promise<boolean> => {
    const conflict = findAddTimeConflict(allEntries, addKind, path);
    if (conflict) {
      setAddError(conflict);
      return false;
    }
    const notFound = await checkPathExists(path);
    if (notFound) {
      setAddError(notFound);
      return false;
    }
    setAddError(null);
    onAdd(path, addKind);
    return true;
  };

  // Requirement highlight is driven entirely by CSS data-attribute selectors
  // (data-required, data-requirement-met) — no inline style needed.
  return (
    <section
      className="pv-step-sources__group"
      data-testid={`source-group-${kind}`}
      data-required={isRequired ? 'true' : 'false'}
      data-requirement-met={isRequired ? (isMet ? 'true' : 'false') : undefined}
      aria-labelledby={headingId}
    >
      {/* Single compact header row: label + count + status + add button.
          When empty this is the entire card height. */}
      <div
        className="pv-step-sources__group-header"
        data-testid="step-sources-group-header"
      >
        <div className="pv-step-sources__group-summary">
          <h3 id={headingId} className="pv-step-sources__group-header-label">
            {SOURCE_KIND_LABELS[kind]()}
          </h3>
          <InfoTip tip={SOURCE_KIND_HELP[kind]()} />
          {hasRows && (
            <span className="pv-step-sources__group-header-count">
              {rows.length}
            </span>
          )}
          {isRequired ? (
            <Pill
              variant={isMet ? 'ok' : 'warn'}
              data-testid={`requirement-status-${kind}`}
            >
              {isMet
                ? `${m.setup_sources_required()} ✓`
                : m.setup_sources_required()}
            </Pill>
          ) : (
            <Pill variant="ghost" data-testid={`requirement-status-${kind}`}>
              {m.setup_sources_optional()}
            </Pill>
          )}
        </div>
        <AddFolderButton
          kind={kind}
          onAdd={handleAdd}
          validationError={addError}
        />
      </div>

      {/* Folder rows only render when present — no empty-state block. */}
      {hasRows && (
        <div>
          {rows.map(({ entry, index }, i) => (
            <SourceRow
              key={`${entry.path}-${index}`}
              entry={entry}
              error={errors[index]}
              isLast={i === rows.length - 1}
              onRemove={() => onRemove(index)}
              onOrganizationStateChange={(state) =>
                onOrganizationStateChange(index, state)
              }
            />
          ))}
        </div>
      )}
    </section>
  );
}

/** A single, single-line source row: path + org-state toggle + remove. */
function SourceRow({
  entry,
  error,
  isLast: _isLast,
  onRemove,
  onOrganizationStateChange,
}: {
  entry: SourceEntry;
  error?: string;
  isLast: boolean;
  onRemove: () => void;
  onOrganizationStateChange: (state: OrganizationState) => void;
}) {
  const isInbox = entry.kind === 'inbox';
  return (
    <div className="pv-step-sources__row">
      <div
        className="pv-step-sources__row-main"
        data-testid="step-sources-row-main"
      >
        <span className="pv-step-sources__row-path pv-mono" title={entry.path}>
          {entry.path}
        </span>
        {!isInbox && (
          <>
            <select
              className={`${selectBase} pv-step-sources__org-select`}
              data-testid={`org-select-${entry.kind}`}
              value={entry.organizationState}
              onChange={(e) =>
                onOrganizationStateChange(e.target.value as OrganizationState)
              }
              aria-label={m.setup_sources_org_state_aria()}
            >
              <option value="organized">
                {m.setup_sources_org_organized()}
              </option>
              <option value="unorganized">
                {m.setup_sources_org_unorganized()}
              </option>
            </select>
            <InfoTip tip={m.setup_sources_org_state_title()} />
          </>
        )}
        <Btn size="sm" variant="ghost" onClick={onRemove}>
          {m.common_remove()}
        </Btn>
      </div>

      {error && <div className="pv-step-sources__row-error">{error}</div>}
    </div>
  );
}

/**
 * Per-group button that opens the native directory picker and adds the chosen
 * folder under this group's kind (type-first by construction).
 */
function AddFolderButton({
  kind,
  onAdd,
  validationError,
}: {
  kind: SourceKind;
  onAdd: (path: string, kind: SourceKind) => Promise<boolean>;
  validationError: string | null;
}) {
  const { pick, loading, error } = useDirectoryPicker();
  const [manualPath, setManualPath] = useState('');

  const handleChoose = async () => {
    const result = await pick(undefined, KIND_TO_LAST_PATH[kind]);
    if (result.path) {
      await onAdd(result.path, kind);
    }
  };

  const handleAddManualPath = async () => {
    const p = manualPath.trim();
    if (!p) return;
    // Only clear the buffer on success — a rejected path (duplicate, overlap,
    // nonexistent) stays visible so the user can see and fix what they typed.
    if (await onAdd(p, kind)) {
      setManualPath('');
    }
  };

  return (
    <div className="pv-step-sources__add-controls">
      <div
        className="pv-step-sources__add-actions"
        data-testid="step-sources-add-actions"
      >
        <Btn
          size="sm"
          variant="primary"
          data-testid="btn-primary"
          onClick={handleChoose}
          disabled={loading}
          aria-label={m.setup_sources_add_folder_aria({
            kind: SOURCE_KIND_LABELS[kind](),
          })}
        >
          {loading ? m.setup_choosing() : m.setup_add_folder()}
        </Btn>
        {/*
          Manual path entry (#662): the native picker guarantees an existing
          directory but can't be scripted (WebDriver can't drive OS dialogs) and
          can't produce inputs the journey's add-time validation needs to reject
          (duplicate/overlap/nonexistent path) — those require typing/pasting a
          path. Reuses the exact same `onAdd` (→ findAddTimeConflict +
          checkPathExists) registration path as the picker, so both entry points
          get identical validation.
        */}
        <span
          className="pv-step-sources__manual-add"
          data-testid={`manual-add-by-path-${kind}`}
        >
          <input
            className="pv-input pv-step-sources__manual-input pv-mono"
            data-testid={`manual-path-input-${kind}`}
            aria-label={m.setup_sources_manual_path_aria({
              kind: SOURCE_KIND_LABELS[kind](),
            })}
            value={manualPath}
            onChange={(ev) => setManualPath(ev.target.value)}
            onKeyDown={(ev) => {
              if (ev.key === 'Enter') {
                ev.preventDefault();
                void handleAddManualPath();
              }
            }}
          />
          <Btn
            size="sm"
            variant="ghost"
            data-testid={`manual-add-path-btn-${kind}`}
            onClick={() => void handleAddManualPath()}
            disabled={!manualPath.trim()}
          >
            {m.setup_sources_add_by_path()}
          </Btn>
        </span>
      </div>
      {error && (
        <span className="pv-step-sources__picker-error">{error.message}</span>
      )}
      {/* role="alert" announces without requiring focus (#502 a11y ask). */}
      {validationError && (
        <span role="alert" className="pv-step-sources__picker-error">
          {validationError}
        </span>
      )}
    </div>
  );
}
