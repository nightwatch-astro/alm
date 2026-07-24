// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * /dev/contracts — Developer Contract Diagnostics page (spec 021).
 *
 * Reachable only via Cmd+K "Developer / Contracts" when devMode is on.
 * When devMode is off, renders a disabled stub (FR-010, US4 acceptance 2).
 *
 * This file is tree-shaken in production builds because:
 * - The route is only registered in dev-tools builds (router.tsx).
 * - The Cmd+K palette entry is only added when devMode is on (CommandPalette.tsx).
 */

import { useState, useEffect, useCallback } from 'react';
import { useMountedRef } from '@/hooks/useMountedRef';
import { commands } from '@/bindings/index';
import { unwrap, invoke } from '@/api/ipc';
import type { ContractMeta, ContractCall } from '@/bindings/index';
import { PageShell } from '@/components';
import { ContractList } from './ContractList';
import { CallList } from './CallList';
import { SchemaViewer } from './SchemaViewer';
import { pickDirectory } from '@/shared/native/picker';
import { getCallSnapshot, subscribeRecorder } from './recorder';
import {
  pageStubBody,
  pageStubHeading,
  pageStubText,
  pageLoading,
  pageBody,
  pageHeader,
  pageTitle,
  pageActions,
  pageError,
  pageExportResult,
  pageSectionHeading,
} from './dev.css';

// ── Disabled stub ─────────────────────────────────────────────────────────────

function DevModeDisabledStub() {
  return (
    <div
      className={`pv-dev-stub pv-page__scroll ${pageStubBody}`}
      data-testid="dev-disabled-stub"
    >
      <h2 className={pageStubHeading}>Developer mode disabled</h2>
      <p className={pageStubText}>
        Enable <strong>devMode</strong> in Settings › Advanced, then restart the
        app to access developer diagnostics.
      </p>
    </div>
  );
}

// ── Main page ─────────────────────────────────────────────────────────────────

export function ContractsPage() {
  const [devMode, setDevMode] = useState<boolean | null>(null);
  const [contracts, setContracts] = useState<ContractMeta[]>([]);
  const [calls, setCalls] = useState<ContractCall[]>(() => getCallSnapshot());
  const [selectedContract, setSelectedContract] = useState<ContractMeta | null>(
    null,
  );
  const [schemaViewerOpen, setSchemaViewerOpen] = useState(false);
  const [schemaPath, setSchemaPath] = useState('');
  const [schemaVersion, setSchemaVersion] = useState('');
  const [exporting, setExporting] = useState(false);
  const [exportResult, setExportResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Check devMode on mount.
  useEffect(() => {
    let cancelled = false;
    commands
      .settingsGet('advanced')
      .then(unwrap)
      .then((data) => {
        if (cancelled) return;
        const vals = data.values as Record<string, unknown>;
        setDevMode(vals?.devMode === true);
      })
      .catch(() => {
        if (!cancelled) setDevMode(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // `loadContracts` is also bound to the manual refresh control, so the guard
  // lives in the callback rather than in a per-effect `cancelled` flag.
  const mountedRef = useMountedRef();

  // Load contracts when devMode is confirmed on.
  const loadContracts = useCallback(() => {
    commands
      .devContractsList({ requestId: null })
      .then(unwrap)
      .then((resp) => {
        if (mountedRef.current) setContracts(resp.contracts);
      })
      .catch((e: unknown) => {
        if (mountedRef.current) setError(String(e));
      });
  }, []);

  // Recent calls are read live from the JS-side recording proxy buffer
  // (`recorder.ts`), which is populated in real time as the wrapped
  // dispatcher records each call (spec 021 follow-up #736). This is the
  // same buffer the recorder already exercises in its own tests; there is
  // no backend round trip in the render path, so the list updates instantly
  // (no manual refresh required).
  const refreshCalls = useCallback(() => {
    setCalls(getCallSnapshot());
  }, []);

  useEffect(() => {
    if (devMode !== true) return;
    loadContracts();
    // `calls` is seeded from getCallSnapshot() at mount (useState initializer
    // above); this only subscribes for subsequent live updates, so there is
    // no synchronous setState call in the effect body itself.
    return subscribeRecorder(refreshCalls);
  }, [devMode, loadContracts, refreshCalls]);

  const handleReplay = useCallback(async (call: ContractCall) => {
    // Replay is only reachable from a `replaySafe` call row (CallList
    // disables the button otherwise). Re-dispatches through the raw Tauri
    // invoke name (dotted contract name -> snake_case fn name) with the
    // exact (already-redacted) recorded request. The recording proxy is
    // still installed, so this produces a brand-new `ContractCall` entry
    // rather than mutating the original (spec 021 plan.md "Replay").
    const cmd = call.contract.replace(/\./g, '_');
    try {
      await invoke(cmd, call.request as Record<string, unknown> | undefined);
    } catch {
      // The outcome (including errors) is captured as a new ContractCall by
      // the recording proxy itself; nothing further to surface here.
    }
  }, []);

  const handleViewSchema = useCallback((contract: ContractMeta) => {
    setSelectedContract(contract);
    setSchemaPath(contract.schemaPath);
    setSchemaVersion(contract.version);
    setSchemaViewerOpen(true);
  }, []);

  const handleViewSchemaForCall = useCallback(
    (call: ContractCall) => {
      const contract = contracts.find((c) => c.name === call.contract);
      if (contract) {
        setSelectedContract(contract);
        setSchemaPath(contract.schemaPath);
        setSchemaVersion(call.contractVersion);
        setSchemaViewerOpen(true);
      }
    },
    [contracts],
  );

  const handleExport = useCallback(async () => {
    // T075 / FR-030: pick an absolute directory so the output path is never
    // relative (which would trigger path.write.denied from the Rust side).
    const dirResult = await pickDirectory(undefined, 'export');
    if (dirResult.cancelled || !dirResult.path) return;

    const sep = dirResult.path.includes('\\') ? '\\' : '/';
    const outputPath = `${dirResult.path}${sep}${Date.now()}-dev-export.json`;

    setExporting(true);
    setExportResult(null);
    try {
      const resp = unwrap(
        await commands.devExport({
          requestId: null,
          outputPath,
          includeContracts: true,
          includeCalls: true,
        }),
      );
      setExportResult(
        `Exported ${resp.contractCount} contracts and ${resp.callCount} calls to ${resp.writtenPath}`,
      );
    } catch (e: unknown) {
      setExportResult(`Export failed: ${String(e)}`);
    } finally {
      setExporting(false);
    }
  }, []);

  // Still loading devMode state.
  if (devMode === null) {
    return (
      <PageShell>
        <div className={`pv-page__scroll ${pageLoading}`}>Loading…</div>
      </PageShell>
    );
  }

  // DevMode is off — show stub, do not subscribe to call stream.
  if (!devMode) {
    return (
      <PageShell>
        <DevModeDisabledStub />
      </PageShell>
    );
  }

  return (
    <PageShell>
      <div className={`pv-dev-contracts pv-page__scroll ${pageBody}`}>
        <div className={pageHeader}>
          <h1 className={pageTitle}>Developer Contract Diagnostics</h1>
          <div className={pageActions}>
            <button
              type="button"
              className="pv-btn pv-btn--sm"
              onClick={refreshCalls}
              aria-label="Refresh calls"
            >
              Refresh calls
            </button>
            <button
              type="button"
              className="pv-btn pv-btn--sm"
              onClick={handleExport}
              disabled={exporting}
              aria-label="Export diagnostics"
            >
              {exporting ? 'Exporting…' : 'Export'}
            </button>
          </div>
        </div>

        {error && (
          <div role="alert" className={pageError}>
            Error: {error}
          </div>
        )}

        {exportResult && (
          <div role="status" className={pageExportResult}>
            {exportResult}
          </div>
        )}

        <section>
          <h2 className={pageSectionHeading}>Contracts ({contracts.length})</h2>
          <ContractList contracts={contracts} onViewSchema={handleViewSchema} />
        </section>

        <section>
          <h2 className={pageSectionHeading}>Recent Calls ({calls.length})</h2>
          <CallList
            calls={calls}
            contracts={contracts}
            onViewSchema={handleViewSchemaForCall}
            onReplay={handleReplay}
          />
        </section>

        {schemaViewerOpen && (
          <SchemaViewer
            schemaPath={schemaPath}
            contractVersion={schemaVersion}
            contractName={selectedContract?.name ?? ''}
            onClose={() => setSchemaViewerOpen(false)}
          />
        )}
      </div>
    </PageShell>
  );
}
