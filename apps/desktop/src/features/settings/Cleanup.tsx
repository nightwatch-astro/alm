// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// spec 018 — owned keys: blockPermanentDelete, defaultProtection,
// protectedCategories. Loads from settings.get('cleanup') on mount and
// auto-saves via the save() prop on change.
//
// Cleanup policy (issue #804): the per-data-type actions that `cleanup_scan`
// and `cleanup_plan_generate` actually read. Backed by the `cleanup.policy.get`
// / `cleanup.policy.update` commands, NOT by the `cleanup` settings scope.
// This replaces the former 15-row `CLEANUP_TYPES` fixture table, which wrote a
// `cleanupTypeOverrides` blob no scan path ever consulted — it looked like the
// cleanup configuration surface while having zero effect on cleanup.
import { useState, useEffect, useRef } from 'react';
import { Toggle, SegControl, Pill, Banner } from '@/ui';
import {
  getSettings,
  cleanupPolicyGet,
  cleanupPolicyUpdate,
  type CleanupAction,
  type CleanupPolicyEntry,
} from './settingsIpc';
import { m } from '@/lib/i18n';
import {
  SettingsSection,
  SettingsRow,
  RestoreDefaultsBtn,
} from './SettingsKit';
import { selectBase } from '@/styles/select.css';

const CLEANUP_KEYS = [
  'blockPermanentDelete',
  'defaultProtection',
  'protectedCategories',
];

// 2-level model (issue #506): the third "normal" level is retired.
type DefaultProtection = 'protected' | 'unprotected';

/**
 * The data types the backend policy model knows (`DataType::as_str` in
 * crates/app/core/src/cleanup_generator.rs). `isProtected` marks the types the
 * backend maps to protected categories (`masters`/`finals`), so opting them
 * into a destructive action warrants the impact warning.
 */
const POLICY_DATA_TYPES: ReadonlyArray<{
  id: string;
  label: () => string;
  isProtected: boolean;
}> = [
  {
    id: 'intermediate',
    label: () => m.settings_cleanup_type_intermediate(),
    isProtected: false,
  },
  {
    id: 'master',
    label: () => m.settings_cleanup_type_master(),
    isProtected: true,
  },
  {
    id: 'final',
    label: () => m.settings_cleanup_type_final(),
    isProtected: true,
  },
];

/** All-Keep, no auto-run — mirrors `default_cleanup_policy()` on the backend. */
const DEFAULT_ACTIONS: Record<string, CleanupAction> = {
  intermediate: 'keep',
  master: 'keep',
  final: 'keep',
};

function entriesToActions(
  entries: CleanupPolicyEntry[],
): Record<string, CleanupAction> {
  const actions = { ...DEFAULT_ACTIONS };
  for (const entry of entries) {
    if (entry.dataType in actions) actions[entry.dataType] = entry.action;
  }
  return actions;
}

function actionsToEntries(
  actions: Record<string, CleanupAction>,
): CleanupPolicyEntry[] {
  return POLICY_DATA_TYPES.map((type) => ({
    dataType: type.id,
    action: actions[type.id] ?? 'keep',
  }));
}

interface CleanupProps {
  save: (scope: string, values: Record<string, unknown>) => void;
}

export function Cleanup({ save }: CleanupProps) {
  // ── spec 018 owned state ─────────────────────────────────────────────────
  const [blockPermanentDelete, setBlockPermanentDelete] = useState(true);
  const [defaultProtection, setDefaultProtection] =
    useState<DefaultProtection>('protected');
  // ── Cleanup policy (issue #804), owned by cleanup.policy.* ───────────────
  const [actions, setActions] =
    useState<Record<string, CleanupAction>>(DEFAULT_ACTIONS);
  const [autoOnCompletion, setAutoOnCompletion] = useState(false);

  const applyValues = (vals: Record<string, unknown>) => {
    if (typeof vals?.blockPermanentDelete === 'boolean') {
      setBlockPermanentDelete(vals.blockPermanentDelete);
    }
    if (vals?.defaultProtection && typeof vals.defaultProtection === 'string') {
      setDefaultProtection(vals.defaultProtection as DefaultProtection);
    }
  };

  // Guards the mount-time fetch below against clobbering a user edit that
  // happens while it's still in flight: the fetch has no way to know its
  // response is stale once the user has already changed something, so it
  // must check this ref (not just `cancelled`, which only covers unmount)
  // before applying its (now-outdated) snapshot.
  const editedRef = useRef(false);

  // Load persisted values from backend on mount (T015).
  useEffect(() => {
    let cancelled = false;
    getSettings({ scope: 'cleanup' })
      .then((data) => {
        if (cancelled || editedRef.current) return;
        applyValues(data.values as Record<string, unknown>);
      })
      .catch(() => {
        // Backend unavailable — stay with in-code defaults.
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Policy has its own store and its own in-flight guard: a slow policy fetch
  // must not clobber a policy edit, independent of the settings-scope fetch.
  const policyEditedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    cleanupPolicyGet()
      .then((policy) => {
        if (cancelled || policyEditedRef.current) return;
        setActions(entriesToActions(policy.entries));
        setAutoOnCompletion(policy.autoOnCompletion);
      })
      .catch(() => {
        // Backend unavailable — stay with the all-Keep default.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const persistPolicy = (
    nextActions: Record<string, CleanupAction>,
    nextAuto: boolean,
  ) => {
    policyEditedRef.current = true;
    void cleanupPolicyUpdate({
      entries: actionsToEntries(nextActions),
      autoOnCompletion: nextAuto,
    }).catch(() => {
      // Auto-save failure surfaces on the next load; the pane keeps the
      // user's selection rather than silently snapping back.
    });
  };

  const handlePolicyChange = (dataType: string, action: string) => {
    const next = { ...actions, [dataType]: action as CleanupAction };
    setActions(next);
    persistPolicy(next, autoOnCompletion);
  };

  const handleRestorePolicy = async () => {
    policyEditedRef.current = true;
    const restored = await cleanupPolicyUpdate({
      entries: actionsToEntries(DEFAULT_ACTIONS),
      autoOnCompletion: false,
    });
    setActions(entriesToActions(restored.entries));
    setAutoOnCompletion(restored.autoOnCompletion);
  };

  // Protected (high-value) categories currently set to a destructive action —
  // surfaced as an impact warning so the change is deliberate.
  const warnedTypes = POLICY_DATA_TYPES.filter(
    (type) => type.isProtected && actions[type.id] !== 'keep',
  ).map((type) => type.label());

  return (
    <>
      {/* Source protection — spec 018 owned, persisted via settings backend */}
      <SettingsSection
        title={m.settings_cleanup_protection_title()}
        action={
          <RestoreDefaultsBtn
            scope="cleanup"
            keys={CLEANUP_KEYS}
            onRestored={applyValues}
          />
        }
      >
        <SettingsRow
          label={m.settings_cleanup_block_delete_label()}
          info={m.settings_cleanup_route_info()}
        >
          <Toggle
            checked={blockPermanentDelete}
            onChange={(v) => {
              editedRef.current = true;
              setBlockPermanentDelete(v);
              save('cleanup', { blockPermanentDelete: v });
            }}
          />
        </SettingsRow>

        <SettingsRow
          label={m.settings_cleanup_default_protection_label()}
          info={m.settings_cleanup_protection_info()}
        >
          <select
            className={`${selectBase} pv-cleanup__protection-select`}
            value={defaultProtection}
            onChange={(e) => {
              editedRef.current = true;
              const v = e.target.value as DefaultProtection;
              setDefaultProtection(v);
              save('cleanup', { defaultProtection: v });
            }}
          >
            <option value="protected">
              {m.settings_cleanup_protection_protected()}
            </option>
            <option value="unprotected">
              {m.settings_cleanup_protection_unprotected()}
            </option>
          </select>
        </SettingsRow>
      </SettingsSection>

      {/* Cleanup policy — the actions `cleanup.scan` reads (issue #804) */}
      <SettingsSection
        title={m.settings_cleanup_policy_title()}
        action={
          <RestoreDefaultsBtn
            onRestore={handleRestorePolicy}
            scopeLabel={m.settings_cleanup_policy_restore_scope()}
          />
        }
      >
        {warnedTypes.length > 0 && (
          <Banner variant="danger" className="pv-cleanup__warning">
            {m.settings_cleanup_pertype_desc({ types: warnedTypes.join(', ') })}
          </Banner>
        )}
        {POLICY_DATA_TYPES.map((type) => (
          <SettingsRow
            key={type.id}
            label={
              <span className="pv-cleanup__type-cell">
                {type.label()}
                {type.isProtected && (
                  <Pill variant="neutral">
                    {m.settings_cleanup_protection_protected()}
                  </Pill>
                )}
              </span>
            }
            info={m.settings_cleanup_policy_info()}
          >
            <SegControl
              options={[
                { value: 'keep', label: m.settings_cleanup_action_keep() },
                {
                  value: 'archive',
                  label: m.settings_cleanup_action_archive(),
                },
                { value: 'delete', label: m.settings_cleanup_action_delete() },
              ]}
              value={actions[type.id] ?? 'keep'}
              onChange={(v) => handlePolicyChange(type.id, v)}
              danger
              dangerValue="delete"
              aria-label={m.settings_cleanup_action_aria({
                type: type.label(),
              })}
            />
          </SettingsRow>
        ))}

        <SettingsRow
          label={m.settings_cleanup_policy_auto_label()}
          info={m.settings_cleanup_policy_auto_info()}
        >
          <Toggle
            checked={autoOnCompletion}
            aria-label={m.settings_cleanup_policy_auto_label()}
            onChange={(v) => {
              setAutoOnCompletion(v);
              persistPolicy(actions, v);
            }}
          />
        </SettingsRow>
      </SettingsSection>
    </>
  );
}
