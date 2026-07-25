// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TargetDetail — spec 036 gen-3 detail pane for a single canonical target.
 *
 * Split by responsibility (refactor sweep #982): `AltitudeGraph.tsx` is the
 * self-contained tonight-altitude SVG chart; `target-detail-format.ts` is
 * pure display-formatting helpers; `useTargetDetailMutations.ts` is the
 * alias/display-alias/notes edit state + mutation handlers;
 * `useTargetTonight.ts` derives tonight planner data; `TonightPanel.tsx`
 * renders the altitude graph + stats; `LinkedItemsList.tsx` renders linked
 * sessions/projects; `AliasesSection.tsx`/`NotesSection.tsx` are the alias
 * and notes editors. This file is Props + data loading + the render.
 */

import { useNavigate } from '@tanstack/react-router';
import type { TargetListItem } from '@/bindings/index';
import {
  useTargetDetail,
  useTargetSessions,
  useTargetProjects,
  useTargetNotes,
  useTargetAstroFormat,
} from './store';
import type { TargetDetailV3 } from './store';
import {
  DetailPane,
  DetailPanel,
  PropertyTable,
  type PropertyDef,
} from '@/components';
import { Pill, Section, EmptyState, Skeleton, Btn } from '@/ui';
import { m } from '@/lib/i18n';
import { USABLE_ALT_DEG } from './planner-altitude';
import type { SensorConfig } from './planner-derive';
import type { ObservingNight } from './astro/moon-state';
import { kindLabel } from './target-detail-format';
import { useTargetDetailMutations } from './useTargetDetailMutations';
import { useTargetTonight } from './useTargetTonight';
import { TonightPanel } from './TonightPanel';
import { LinkedSessionsList, LinkedProjectsList } from './LinkedItemsList';
import { AliasesSection } from './AliasesSection';
import { NotesSection } from './NotesSection';

// ── Types ─────────────────────────────────────────────────────────────────────

interface Props {
  targetId: string;
  /** The selected list row — supplies constellation/magnitude + tonight stats. */
  item?: TargetListItem | null;
  /** Usable-altitude threshold (from Settings) for img-time / visible-tonight. */
  usableAltDeg?: number;
  /**
   * The shared observing night (spec 047), or `null` when no observing site
   * exists (site gate). Drives the real lunar distance + filter guidance
   * shown alongside the real (spec 044 Track B) tonight altitude graph.
   */
  night?: ObservingNight | null;
  /**
   * Camera sensor configuration (FR-035/FR-036, T046): when OSC the
   * imaging-time stat collapses to the strictest-band single-pass window and
   * a per-line breakdown appears for narrowband passbands (FR-037).
   * `null`/absent keeps the per-filter model unchanged (FR-038).
   */
  sensorConfig?: SensorConfig | null;
  /**
   * #658: called after an alias add/remove or display-alias set/clear
   * mutation succeeds, so the caller can refetch the list payload the
   * Targets page filters/renders from — otherwise a fresh user alias stays
   * unsearchable and a new display label never propagates to the list row
   * until an unrelated remount refetches it.
   */
  onMutated?: () => void;
}

type LoadState =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'loaded'; data: TargetDetailV3 };

// ── TargetDetail ────────────────────────────────────────────────────────────

export function TargetDetail({
  targetId,
  item = null,
  usableAltDeg = USABLE_ALT_DEG,
  night = null,
  sensorConfig = null,
  onMutated,
}: Props) {
  const navigate = useNavigate();

  const detailQuery = useTargetDetail(targetId);
  const { data: sessions = [], loading: sessionsLoading } =
    useTargetSessions(targetId);
  const { data: projects = [], loading: projectsLoading } =
    useTargetProjects(targetId);
  const { data: notes = null } = useTargetNotes(targetId);
  // Sexagesimal RA/Dec (adopt target-match): backend-formatted, carry-safe
  // rounding (replaces the hand-rolled fmtRa/fmtDec, which could round a
  // seconds value up to an invalid ":60"). Only fires once the detail has
  // loaded (mirrors the pre-migration effect's `loadState.status==='loaded'` gate).
  const { data: astroFormat = null } = useTargetAstroFormat(
    targetId,
    detailQuery.data?.raDeg ?? null,
    detailQuery.data?.decDeg ?? null,
    !!detailQuery.data,
  );

  // Tonight planner data — hook must be called unconditionally (React rules of
  // hooks), so it accepts nullable coordinates and degrades cleanly.
  const tonight = useTargetTonight({
    targetId,
    raDeg: detailQuery.data?.raDeg ?? null,
    decDeg: detailQuery.data?.decDeg ?? null,
    item,
    usableAltDeg,
    night,
    sensorConfig,
  });

  const loadState: LoadState = detailQuery.error
    ? { status: 'error', message: m.targets_detail_load_error() }
    : detailQuery.loading || !detailQuery.data
      ? { status: 'loading' }
      : { status: 'loaded', data: detailQuery.data };

  const {
    aliasInput,
    setAliasInput,
    aliasError,
    actionError,
    displayAliasInput,
    setDisplayAliasInput,
    displayAliasEditing,
    setDisplayAliasEditing,
    notesEditing,
    setNotesEditing,
    notesDraft,
    setNotesDraft,
    notesSaving,
    notesSaved,
    setNotesSaved,
    notesError,
    setNotesError,
    handleNotesSave,
    handleAliasAdd,
    handleAliasRemove,
    handleDisplayAliasSet,
    handleDisplayAliasClear,
  } = useTargetDetailMutations({
    targetId,
    serverDisplayAlias: detailQuery.data?.displayAlias,
    notes,
    onMutated,
  });

  // ── Render ──────────────────────────────────────────────────────────────────

  if (loadState.status === 'loading') {
    return (
      <DetailPane data-testid="target-detail">
        <Skeleton count={5} label={m.common_loading()} />
      </DetailPane>
    );
  }

  if (loadState.status === 'error') {
    return (
      <DetailPane data-testid="target-detail">
        <EmptyState
          title={m.settings_advanced_log_error()}
          desc={loadState.message}
        />
      </DetailPane>
    );
  }

  const detail = loadState.data;

  // Derive catalog pills from aliases with kind='designation' (non-primary).
  const catalogPills = detail.aliases
    .filter(
      (a) => a.kind === 'designation' && a.alias !== detail.primaryDesignation,
    )
    .slice(0, 4);

  // Common name (first common_name alias, if any).
  const commonName =
    detail.aliases.find((a) => a.kind === 'common_name')?.alias ?? null;

  // RA and Dec on separate lines (user decision 2026-07-17): the mono face is
  // wider than the old sans, so the joined form wrapped mid-coordinate.
  const raDecStr = astroFormat
    ? `${astroFormat.raSexagesimal}\n${astroFormat.decSexagesimal}`
    : null;

  // Identity facts split across two tabular columns (left-packed).
  const identityA: PropertyDef[] = [
    {
      key: 'desig',
      label: m.targets_col_designation(),
      value: detail.primaryDesignation,
    },
    {
      key: 'type',
      label: m.cmp_target_search_type_label(),
      value: detail.objectType.replace(/_/g, ' '),
    },
    {
      key: 'constellation',
      label: m.targets_prop_constellation(),
      value: item?.constellation ?? null,
    },
    {
      key: 'radec',
      label: m.targets_prop_ra_dec(),
      value: raDecStr,
      mono: true, // spec 055 FR-005: RA/Dec coordinate values render mono.
    },
  ];
  const identityB: PropertyDef[] = [
    {
      key: 'magnitude',
      label: m.targets_prop_magnitude(),
      value: item?.magnitude ?? null,
    },
    {
      key: 'source',
      label: m.projects_wizard_col_source(),
      value: detail.source,
    },
    {
      // Always applicable to a target identity (data-model.md: target-identity
      // fields are always applicable) — present the row even when unresolved
      // (spec-030 Q16 / FR-135) instead of omitting it for a non-SIMBAD target.
      key: 'simbad',
      label: m.targets_prop_simbad_oid(),
      value: detail.simbadOid ?? null,
    },
  ];

  // Title identity (h2).
  const titleContent = (
    <h2 className="pv-planner__title">
      {detail.effectiveLabel}
      {commonName && commonName !== detail.effectiveLabel && (
        <span className="pv-planner__subtitle"> — {commonName}</span>
      )}
    </h2>
  );

  // Pills + "New project" action.
  const titleExtraContent = (
    <>
      <div className="pv-planner__actions">
        <Btn
          size="sm"
          variant="primary"
          onClick={() => {
            void navigate({
              to: '/projects/new',
              search: { targetId: detail.id },
            });
          }}
        >
          {m.targets_detail_new_project()}
        </Btn>
      </div>
      <div className="pv-planner__pill-row">
        <Pill variant="neutral">{detail.objectType.replace(/_/g, ' ')}</Pill>
        {catalogPills.map((a) => (
          <Pill key={a.id} variant="ghost">
            <span title={m.targets_detail_alias_kind_title({ kind: a.kind })}>
              <span className="pv-target-detail__alias-kind">
                [{kindLabel(a.kind)}]
              </span>
              {a.alias}
            </span>
          </Pill>
        ))}
      </div>
    </>
  );

  return (
    <DetailPanel
      fill
      title={titleContent}
      titleExtra={titleExtraContent}
      data-testid="target-detail"
    >
      {/* #816 → #1107: this div used to be the pane's own scroll region,
          because DetailPanel rendered `children` as a bare sibling of the
          header with nothing establishing overflow — so everything below
          (identity/tonight, coverage, links, display label, aliases, projects,
          notes, back button) was silently clipped by the pane's
          overflow:hidden.
          DetailPanel now always wraps children in `.pv-detailpanel__content`,
          which owns the scrolling for every page, so this is a plain layout
          div. Do NOT give it `overflow-y` again — that nests a second scroller
          inside the shared one and produces double scrollbars. */}
      <div className="pv-planner__scroll" data-testid="planner-scroll">
        {/* ── Identity + Tonight — left-packed: [facts A][facts B][tonight] ── */}
        <div className="pv-planner__cols">
          <div className="pv-planner__col">
            <PropertyTable mode="view" properties={identityA} />
          </div>
          <div className="pv-planner__col">
            <PropertyTable mode="view" properties={identityB} />
          </div>

          <TonightPanel
            tonight={tonight}
            effectiveLabel={detail.effectiveLabel}
            usableAltDeg={usableAltDeg}
            sensorConfig={sensorConfig}
          />
        </div>

        {/* ── Coverage bars ────────────────────────────────────────────────── */}
        <div className="pv-planner__coverage">
          <p className="pv-planner__section-title">{m.common_coverage()}</p>
          <div className="pv-planner__coverage-list">
            <span className="pv-planner__coverage-stub">
              {m.targets_detail_no_coverage()}
            </span>
          </div>
        </div>

        {/* ── Linked sessions + projects ───────────────────────────────────── */}
        <div className="pv-planner__links">
          <div>
            <p className="pv-planner__link-col-title">{m.common_sessions()}</p>
            <LinkedSessionsList sessions={sessions} loading={sessionsLoading} />
          </div>
          <div>
            <p className="pv-planner__link-col-title">{m.common_projects()}</p>
            <LinkedProjectsList projects={projects} loading={projectsLoading} />
          </div>
        </div>

        {/* ── Display label ────────────────────────────────────────────────── */}
        <Section title={m.targets_detail_display_label_title()}>
          {displayAliasEditing ? (
            <div className="pv-target-detail__display-alias-edit">
              <input
                aria-label={m.targets_detail_display_label_title()}
                placeholder={detail.primaryDesignation}
                value={displayAliasInput}
                onChange={(e) => setDisplayAliasInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void handleDisplayAliasSet();
                  if (e.key === 'Escape') setDisplayAliasEditing(false);
                }}
                className="pv-target-detail__text-input"
                // eslint-disable-next-line jsx-a11y/no-autofocus -- focus management: the inline display-label editor mounts on demand and must receive focus so the user can type immediately
                autoFocus
              />
              <button
                onClick={handleDisplayAliasSet}
                className="pv-target-detail__action-btn"
              >
                {m.common_save()}
              </button>
              {detail.displayAlias != null && (
                <button
                  onClick={handleDisplayAliasClear}
                  className="pv-target-detail__action-btn pv-target-detail__action-btn--muted"
                >
                  {m.common_clear()}
                </button>
              )}
              <button
                onClick={() => setDisplayAliasEditing(false)}
                className="pv-target-detail__action-btn pv-target-detail__action-btn--muted"
              >
                {m.common_cancel()}
              </button>
            </div>
          ) : (
            <div className="pv-target-detail__display-alias-view">
              <span className="pv-target-detail__display-alias-value">
                {detail.displayAlias ?? (
                  <em className="pv-target-detail__display-alias-placeholder">
                    {m.targets_detail_display_label_unset()}
                  </em>
                )}
              </span>
              <button
                onClick={() => setDisplayAliasEditing(true)}
                className="pv-target-detail__edit-btn"
              >
                {detail.displayAlias != null
                  ? m.common_edit()
                  : m.targets_detail_set_alias()}
              </button>
            </div>
          )}
        </Section>

        {/* ── Aliases ──────────────────────────────────────────────────────── */}
        <AliasesSection
          aliases={detail.aliases}
          aliasInput={aliasInput}
          setAliasInput={setAliasInput}
          aliasError={aliasError}
          actionError={actionError}
          onAdd={handleAliasAdd}
          onRemove={handleAliasRemove}
        />

        {/* ── Observing notes (spec 023 US4) ──────────────────────────────── */}
        <NotesSection
          notes={notes}
          editing={notesEditing}
          setEditing={setNotesEditing}
          draft={notesDraft}
          setDraft={setNotesDraft}
          saving={notesSaving}
          saved={notesSaved}
          setSaved={setNotesSaved}
          error={notesError}
          setError={setNotesError}
          onSave={handleNotesSave}
        />

        {/* Back button */}
        <button
          className="pv-target-detail__back-btn"
          onClick={() => navigate({ to: '/targets' })}
        >
          {m.targets_detail_back()}
        </button>
      </div>
    </DetailPanel>
  );
}
