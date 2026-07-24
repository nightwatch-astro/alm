// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useState, useEffect } from 'react';
import { m } from '@/lib/i18n';
import { useNavigate, useSearch } from '@tanstack/react-router';
import { WizardShell, Btn } from '@/ui';
import type { WizardStep } from '@/ui';
import { StepName, type StepNameData } from './StepName';
import { StepSources, type StepSourcesData } from './StepSources';
import { StepCalibration, type CalibrationMapping } from './StepCalibration';
import { StepViews, type StepViewsData } from './StepViews';
import { StepLayout, type StepLayoutData } from './StepLayout';
import { StepReview } from './StepReview';
import { queryKeys } from '@/data/queryKeys';
import { useQuery } from '@tanstack/react-query';
import { callCreateProject } from '@/features/projects/store';
import { addToast } from '@/shared/toast';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import {
  createProjectErrorCode,
  findDuplicateProjectName,
  mapCreateProjectErrorCode,
  projectCreateErrorField,
  type ProjectCreateErrorField,
} from '@/features/projects/projectCreateErrors';
import { page, pageBar } from '@/styles/app-shell.css';

const STORAGE_KEY = 'alm-project-wizard-draft';

/**
 * #719 SC-004: the wizard collected per-filter flat mappings and shared
 * dark/bias/dark-flat picks in step 3, then discarded them at create time
 * (only `{name, tool, path, initialSources, notes}` was ever sent — no
 * contract field exists for a batch of calibration assignments). Persist
 * them the same way the post-create Calibration page does: one
 * `calibration.match.assign` call per (session, master) pair. Best-effort —
 * the project is already created by the time this runs, so a failure here
 * is surfaced nowhere; the same assignment remains available from the
 * Calibration page.
 */
async function assignWizardCalibrationSelections(
  sessionIds: string[],
  calibration: CalibrationMapping,
): Promise<void> {
  const hasSelection =
    Boolean(calibration.sharedDarkId) ||
    Boolean(calibration.sharedBiasId) ||
    Boolean(calibration.sharedDarkFlatId) ||
    Object.values(calibration.flatMappings).some(Boolean);
  if (sessionIds.length === 0 || !hasSelection) return;

  let sessions: Array<{ id: string; sessionKey: { filter: string } }> = [];
  try {
    sessions = unwrap(await commands.sessionsList(null, null)).filter((s) =>
      sessionIds.includes(s.id),
    );
  } catch {
    return;
  }

  async function assign(sessionId: string, masterId: string): Promise<void> {
    try {
      await commands.calibrationMatchAssign({
        contractVersion: '1.0',
        requestId: crypto.randomUUID(),
        sessionId,
        masterId,
        override: false,
      });
    } catch {
      // Non-fatal — see docstring above.
    }
  }

  for (const session of sessions) {
    if (calibration.sharedDarkId) {
      await assign(session.id, calibration.sharedDarkId);
    }
    if (calibration.sharedBiasId) {
      await assign(session.id, calibration.sharedBiasId);
    }
    if (calibration.sharedDarkFlatId) {
      await assign(session.id, calibration.sharedDarkFlatId);
    }
    const flatMasterId = calibration.flatMappings[session.sessionKey.filter];
    if (flatMasterId) {
      await assign(session.id, flatMasterId);
    }
  }
}

interface WizardData {
  name: StepNameData;
  sources: StepSourcesData;
  calibration: CalibrationMapping;
  views: StepViewsData;
  layout: StepLayoutData;
}

const INITIAL_DATA: WizardData = {
  name: { name: '', workflowProfile: 'pixinsight', target: null },
  sources: { selectedSessionIds: [] },
  calibration: {
    flatMappings: {},
    sharedDarkId: '',
    sharedBiasId: '',
    sharedDarkFlatId: '',
  },
  views: { strategy: 'junction' },
  layout: { namingPattern: '' },
};

function loadDraft(): WizardData {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return { ...INITIAL_DATA, ...JSON.parse(raw) };
  } catch {
    // ignore
  }
  return INITIAL_DATA;
}

function saveDraft(data: WizardData): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
  } catch {
    // ignore
  }
}

function clearDraft(): void {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
}

// Render-time factory so labels re-read the active locale (spec 046 #8).
function stepLabels(): string[] {
  return [
    m.projects_wizard_step_name_profile(),
    m.projects_wizard_step_sources_lights(),
    m.projects_wizard_step_calibration(),
    m.projects_wizard_step_source_views(),
    m.projects_wizard_step_naming_layout(),
    m.projects_wizard_step_review_create(),
  ];
}

function profileLabelFor(profile: string): string | undefined {
  switch (profile) {
    case 'pixinsight':
      return m.projects_wizard_profile_pixinsight();
    case 'siril':
      return m.projects_wizard_profile_siril();
    case 'planetary':
      return m.projects_wizard_profile_planetary_lunar();
    default:
      return undefined;
  }
}

// Map wizard workflowProfile to the ProjectTool enum expected by the backend.
const PROFILE_TO_TOOL: Record<string, 'PixInsight' | 'Siril'> = {
  pixinsight: 'PixInsight',
  siril: 'Siril',
  planetary: 'Siril',
};

/**
 * Derive a safe folder name from the project name (kebab-case, no special
 * chars). The backend anchors this relative name to the registered project
 * folder (registered_sources.kind = 'project'), so no 'projects/' prefix
 * here — that would nest a redundant level under the folder the user already
 * chose during setup. Shared by `handleCreate` and the review step (#599) so
 * both show the identical real path instead of a fixture.
 */
function deriveProjectPath(trimmedName: string): string {
  const safeName = trimmedName
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return safeName || 'new-project';
}

export function WizardPage() {
  const navigate = useNavigate();
  const [currentStep, setCurrentStep] = useState(0);
  const [wizardData, setWizardData] = useState<WizardData>(loadDraft);
  const [creating, setCreating] = useState(false);
  // Per-field projects.create error (WP-008-B): 'name'/'tool' route back to the
  // name step (StepName owns both fields); 'path'/'general' have no dedicated
  // step (path is derived from the project name, never user-edited) so they
  // surface inline next to the Create button on the review step.
  const [createError, setCreateError] = useState<{
    field: ProjectCreateErrorField;
    message: string;
  } | null>(null);

  // Render-time so labels re-read the active locale (spec 046 #8).
  const labels = stepLabels();

  // In mock mode, allow skipping all validation to walk through the wizard quickly
  const devSkip = import.meta.env.VITE_USE_MOCKS === 'true';

  // #612/#783: a caller (e.g. TargetDetailV2's "+ New project here") can pass
  // a real target id via `?targetId=`. `strict: false` reads it without this
  // route declaring its own search schema. Resolve it to a real target once
  // and prefill the name-step target picker, instead of the prior behaviour
  // of fabricating a "From target context" label from typed text (#783).
  const search: { targetId?: string } = useSearch({ strict: false });
  const incomingTargetId = search.targetId;

  // #599: real frame count for the review step's sources summary, sharing
  // the same `sessions.list` cache StepSources/StepViews already populate.
  const { data: allSessions } = useQuery({
    queryKey: queryKeys.sessions.all(),
    queryFn: async () => unwrap(await commands.sessionsList(null, null)),
  });
  useEffect(() => {
    if (!incomingTargetId || wizardData.name.target) return;
    let cancelled = false;
    void (async () => {
      try {
        const detail = unwrap(
          await commands.targetGet({ targetId: incomingTargetId }),
        );
        if (cancelled) return;
        setWizardData((prev) =>
          prev.name.target
            ? prev
            : {
                ...prev,
                name: {
                  ...prev.name,
                  name: prev.name.name || detail.primaryDesignation,
                  target: {
                    targetId: detail.id,
                    primaryDesignation: detail.primaryDesignation,
                    commonName: detail.displayAlias ?? null,
                  },
                },
              },
        );
      } catch {
        // Non-fatal: the wizard still works with no prefilled target.
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [incomingTargetId]);

  // Save draft on step/data change
  useEffect(() => {
    saveDraft(wizardData);
  }, [currentStep, wizardData]);

  // Clear a stale name/tool create-error once the user actually edits the
  // field it is attached to (the corresponding backend rule may no longer
  // apply). `next` is compared against the wizardData snapshot captured when
  // this error was raised — NOT just "onChange fired" — because StepName is
  // only mounted while on step 0: routing back here after a duplicate-name
  // rejection remounts it fresh, and its resync effect calls react-hook-form's
  // `reset(data)` to restore the draft. That `reset()` itself notifies the
  // same `watch()` subscription that drives this onChange, with values
  // IDENTICAL to what's already in `wizardData` — no user action involved. An
  // unconditional clear here nulled the just-set banner before the user ever
  // saw it (the reported "flashes and clears" bug). Only a genuine change to
  // the field the error is attached to should dismiss it.
  function clearNameToolCreateError(next: StepNameData): void {
    setCreateError((prev) => {
      if (!prev) return prev;
      if (prev.field === 'name') {
        return next.name.trim() !== wizardData.name.name.trim() ? null : prev;
      }
      if (prev.field === 'tool') {
        return next.workflowProfile !== wizardData.name.workflowProfile
          ? null
          : prev;
      }
      return prev;
    });
  }

  // Step validation — devSkip bypasses all gates so you can walk through without data
  function canAdvance(): boolean {
    if (devSkip) return true;
    switch (currentStep) {
      case 0:
        return wizardData.name.name.trim().length > 0;
      case 1:
        // #719 FR-004/SC-001: zero-source creation must be reachable — the
        // backend already supports it (`initial_sources: []` → lifecycle
        // `setup_incomplete`); the wizard no longer blocks leaving this step.
        return true;
      case 2:
        return true; // Calibration is optional
      case 3:
        return true; // View strategy has a default
      case 4:
        return true; // Layout has defaults
      default:
        return false;
    }
  }

  function handleNext() {
    if (currentStep < labels.length - 1 && canAdvance()) {
      setCurrentStep(currentStep + 1);
    }
  }

  function handleBack() {
    if (currentStep > 0) {
      setCurrentStep(currentStep - 1);
    }
  }

  function handleCancel() {
    clearDraft();
    void navigate({ to: '/projects' });
  }

  // T078c: wire actual project creation at step 5 (Review & create).
  // WP-008-B: ported CreateProjectDialog's live duplicate-name pre-check and
  // per-field error mapping (see @/features/projects/projectCreateErrors) so
  // failures land on the step/field they're actionable from, instead of one
  // generic error toast.
  async function handleCreate() {
    if (creating) return;
    setCreating(true);
    setCreateError(null);
    try {
      const trimmedName = wizardData.name.name.trim();

      if (await findDuplicateProjectName(trimmedName)) {
        setCreateError({
          field: 'name',
          message: m.projects_create_name_duplicate(),
        });
        setCurrentStep(0);
        return;
      }

      const tool =
        PROFILE_TO_TOOL[wizardData.name.workflowProfile] ?? 'PixInsight';
      const path = deriveProjectPath(trimmedName);

      const result = await callCreateProject({
        requestId: crypto.randomUUID(),
        name: trimmedName,
        tool,
        path,
        initialSources: wizardData.sources.selectedSessionIds,
        notes: null,
        // #719 SC-001/#612: carry the real target picked in StepName (or
        // prefilled from `?targetId=`) instead of discarding it.
        canonicalTargetId: wizardData.name.target?.targetId ?? null,
      });

      await assignWizardCalibrationSelections(
        wizardData.sources.selectedSessionIds,
        wizardData.calibration,
      );

      clearDraft();

      // mkdir-only scaffolding auto-apply (user decision 2026-07-04,
      // supersedes D16's "View plan" toast which linked to a wrong page):
      // the backend auto-applies the folder plan when every action is a
      // directory creation and reports the outcome in `scaffoldApplied`.
      // - true  → folders exist on disk: confirm.
      // - false → creation failed; the plan record remains reviewable.
      // - null/undefined → plan needs manual review (non-mkdir actions).
      //
      // #604: every toast carries a "View project" navigation affordance —
      // the project record exists in all three outcomes above, even when
      // folder scaffolding failed.
      const viewProjectAction = {
        label: m.projects_wizard_toast_view_project(),
        onClick: () =>
          void navigate({
            to: '/projects',
            search: { selected: result.projectId },
          }),
      };
      if (result.scaffoldApplied === false) {
        addToast({
          message: m.projects_wizard_toast_folders_failed({
            name: trimmedName,
          }),
          variant: 'error',
          action: viewProjectAction,
        });
      } else if (result.scaffoldApplied) {
        addToast({
          message: m.projects_wizard_toast_created_folders({
            name: trimmedName,
          }),
          variant: 'success',
          action: viewProjectAction,
        });
      } else {
        addToast({
          message: m.projects_wizard_toast_created({ name: trimmedName }),
          variant: 'success',
          action: viewProjectAction,
        });
      }

      // Navigate back to projects list; the list re-fetches automatically.
      void navigate({ to: '/projects' });
    } catch (err: unknown) {
      const code = createProjectErrorCode(err);
      const field = projectCreateErrorField(code);
      setCreateError({ field, message: mapCreateProjectErrorCode(code) });
      if (field === 'name' || field === 'tool') {
        setCurrentStep(0);
      }
    } finally {
      setCreating(false);
    }
  }

  const steps: WizardStep[] = labels.map((label, i) => ({
    label,
    completed: i < currentStep,
  }));

  const projectLabel = wizardData.name.name || m.projects_create_title();
  const profileLabel =
    profileLabelFor(wizardData.name.workflowProfile) ??
    wizardData.name.workflowProfile;

  // Computed summary counts
  const flatsMapped = Object.values(wizardData.calibration.flatMappings).filter(
    Boolean,
  ).length;
  const darkSelected = wizardData.calibration.sharedDarkId ? 1 : 0;
  const biasSelected = wizardData.calibration.sharedBiasId ? 1 : 0;
  const darkFlatSelected = wizardData.calibration.sharedDarkFlatId ? 1 : 0;
  // #776: real master count feeding StepViews' scope/items row.
  const selectedMasterCount =
    flatsMapped + darkSelected + biasSelected + darkFlatSelected;

  // Back / Next button labels per wireframe
  const backLabels = [
    '',
    m.setup_wizard_back(),
    m.projects_wizard_back_to_sources(),
    m.projects_wizard_back_calibration(),
    m.projects_wizard_back_source_views(),
    m.setup_wizard_back(),
  ];
  const nextLabels = [
    m.projects_wizard_next_sources(),
    m.projects_wizard_next_calibration(),
    m.projects_wizard_next_source_views(),
    m.projects_wizard_next_naming(),
    m.projects_wizard_next_review(),
    '',
  ];

  // Summary panel (right rail)
  const summary = (
    <div className="pv-wizard-page__summary">
      <div className="pv-wizard-page__summary-heading">
        {m.projects_wizard_summary_title()}
      </div>
      <div>
        <div className="pv-wizard-page__summary-project-name">
          {projectLabel}
        </div>
        <div className="pv-wizard-page__summary-profile">{profileLabel}</div>
      </div>

      <div>
        <div className="pv-wizard-page__summary-section-heading">
          {m.projects_wizard_summary_selected()}
        </div>
        <div className="pv-wizard-page__summary-list">
          <SummaryRow
            label={m.projects_wizard_summary_lights_label()}
            value={`${wizardData.sources.selectedSessionIds.length} sess`}
          />
          <SummaryRow
            label={m.projects_wizard_summary_darks_label()}
            value={m.projects_wizard_summary_master_count({
              count: darkSelected,
            })}
          />
          <SummaryRow
            label={m.projects_wizard_flats_label()}
            value={m.projects_wizard_summary_master_count({
              count: flatsMapped,
            })}
          />
          <SummaryRow
            label={m.common_bias()}
            value={m.projects_wizard_summary_master_count({
              count: biasSelected,
            })}
          />
        </div>
      </div>

      {currentStep < 5 && (
        <div>
          <div className="pv-wizard-page__summary-section-heading">
            {m.projects_wizard_summary_coming_up()}
          </div>
          <div className="pv-wizard-page__summary-list">
            {labels.slice(currentStep + 1).map((label, i) => (
              <div
                key={label}
                className={
                  'pv-wizard-page__coming-up-item' +
                  (i < labels.length - currentStep - 2
                    ? ' pv-wizard-page__coming-up-item--sep'
                    : '')
                }
              >
                {currentStep + i + 2}. {label}
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="pv-wizard-page__footprint">
        <div className="pv-wizard-page__footprint-label">
          {m.projects_wizard_footprint_label()}
        </div>
        <div className="pv-mono pv-wizard-page__footprint-value">
          {m.projects_wizard_footprint_value()}
        </div>
        <div className="pv-wizard-page__footprint-note">
          {m.projects_wizard_footprint_note()}
        </div>
      </div>

      {/* Navigation buttons in the summary rail */}
      <div className="pv-wizard-page__summary-nav">
        {currentStep > 0 && (
          <Btn size="sm" onClick={handleBack}>
            {backLabels[currentStep]}
          </Btn>
        )}
        {currentStep < 5 && (
          <Btn
            variant="primary"
            size="sm"
            onClick={handleNext}
            disabled={!canAdvance()}
            className="pv-wizard-page__flex-fill"
          >
            {nextLabels[currentStep]}
          </Btn>
        )}
        {currentStep === 5 && (
          <>
            {/* path/general projects.create errors have no dedicated step or
                field (path is derived from the project name, never user-
                edited), so they surface here next to the action that raised
                them — matching CreateProjectDialog's inline serverError. */}
            {createError &&
              (createError.field === 'path' ||
                createError.field === 'general') && (
                <span role="alert" className="pv-field-error">
                  {createError.message}
                </span>
              )}
            <Btn
              variant="primary"
              size="sm"
              onClick={() => void handleCreate()}
              disabled={creating || !wizardData.name.name.trim()}
              className="pv-wizard-page__flex-fill"
              data-testid="wizard-create-btn"
            >
              {creating
                ? m.projects_create_creating()
                : m.projects_create_btn()}
            </Btn>
          </>
        )}
      </div>
    </div>
  );

  // T078c layout fix: min-height: 0 prevents flex overflow that caused the
  // wizard to render at the bottom of the window instead of filling the main area.
  return (
    <div className={`${page} pv-wizard-page`}>
      {/* Wizard toolbar — styled consistently with other page toolbars */}
      <div className={`${pageBar} pv-wizard-page__toolbar`}>
        <span className="pv-wizard-page__toolbar-title">
          {m.projects_wizard_toolbar_title()} {projectLabel}
        </span>
        <span className="pv-wizard-page__spacer" />
        {devSkip && (
          <Btn
            size="sm"
            onClick={() => {
              clearDraft();
              setWizardData(INITIAL_DATA);
              setCurrentStep(0);
            }}
          >
            {m.projects_wizard_reset_btn()}
          </Btn>
        )}
        <Btn size="sm" onClick={handleCancel}>
          {m.common_cancel()}
        </Btn>
      </div>

      {/* Sub-toolbar: workflow profile breadcrumb */}
      <div className={`${pageBar} pv-wizard-page__subbbar`}>
        <span>
          {m.projects_wizard_workflow_profile_label()} {profileLabel}
        </span>
        <span>&middot;</span>
        {/* #783: only render when a real target was resolved (picked in
            StepName or carried over via `?targetId=`) — never fabricated
            from the typed project name. */}
        {wizardData.name.target && (
          <span>
            {m.projects_wizard_from_target_label()}{' '}
            {wizardData.name.target.commonName ??
              wizardData.name.target.primaryDesignation}
          </span>
        )}
        <span className="pv-wizard-page__subbar-note">
          {m.projects_wizard_subbar_note()}
        </span>
      </div>

      {/* WizardShell fills the remaining space */}
      <WizardShell
        steps={steps}
        currentStep={currentStep}
        summary={summary}
        className="pv-wizard-page__flex-fill--noscroll"
      >
        {/* Step title + description */}
        <div className="pv-wizard-page__step-header">
          <h2 className="pv-wizard-page__step-title" data-wizard-step-heading>
            {m.projects_wizard_step_label()} {currentStep + 1} &middot;{' '}
            {labels[currentStep]}
          </h2>
        </div>

        {/* Step content */}
        {currentStep === 0 && (
          <StepName
            data={wizardData.name}
            onChange={(name) => {
              clearNameToolCreateError(name);
              setWizardData({ ...wizardData, name });
            }}
            serverError={
              createError &&
              (createError.field === 'name' || createError.field === 'tool')
                ? { field: createError.field, message: createError.message }
                : null
            }
          />
        )}
        {currentStep === 1 && (
          <StepSources
            data={wizardData.sources}
            onChange={(sources) => setWizardData({ ...wizardData, sources })}
          />
        )}
        {currentStep === 2 && (
          <StepCalibration
            selectedSessionIds={wizardData.sources.selectedSessionIds}
            data={wizardData.calibration}
            onChange={(calibration) =>
              setWizardData({ ...wizardData, calibration })
            }
          />
        )}
        {currentStep === 3 && (
          <StepViews
            data={wizardData.views}
            onChange={(views) => setWizardData({ ...wizardData, views })}
            selectedSessionIds={wizardData.sources.selectedSessionIds}
            selectedMasterCount={selectedMasterCount}
          />
        )}
        {currentStep === 4 && (
          <StepLayout
            data={wizardData.layout}
            nameData={wizardData.name}
            strategy={wizardData.views.strategy}
            onChange={(layout) => setWizardData({ ...wizardData, layout })}
          />
        )}
        {currentStep === 5 && (
          <StepReview
            wizardState={{
              name: wizardData.name.name.trim(),
              path: deriveProjectPath(wizardData.name.name.trim()),
              profileLabel,
              targetLabel:
                wizardData.name.target?.commonName ??
                wizardData.name.target?.primaryDesignation ??
                null,
              sessionCount: wizardData.sources.selectedSessionIds.length,
              frameCount: (allSessions ?? [])
                .filter((s) =>
                  wizardData.sources.selectedSessionIds.includes(s.id),
                )
                .reduce((acc, s) => acc + s.frameCount, 0),
              masterCount: selectedMasterCount,
              viewStrategy: wizardData.views.strategy,
            }}
          />
        )}
      </WizardShell>
    </div>
  );
}

function SummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="pv-wizard-page__summary-row">
      <span className="pv-wizard-page__summary-row-label">{label}</span>
      <span className="pv-mono">{value}</span>
    </div>
  );
}
