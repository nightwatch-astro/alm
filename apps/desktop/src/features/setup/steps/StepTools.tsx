// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useEffect, useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { RotateCw } from 'lucide-react';
import { Btn } from '@/ui/Btn';
import { Pill } from '@/ui/Pill';
import { Toggle } from '@/ui/Toggle';
import { m } from '@/lib/i18n';
import { useFilePicker } from '@/shared/native/picker';
import { unwrap } from '@/api/ipc';

export interface ToolConfig {
  enabled: boolean;
  path: string | null;
}

export interface ToolsState {
  pixinsight: ToolConfig;
  siril: ToolConfig;
}

export const DEFAULT_TOOLS_STATE: ToolsState = {
  pixinsight: { enabled: false, path: null },
  siril: { enabled: false, path: null },
};

export interface StepToolsProps {
  tools: ToolsState;
  onToolsChange: (tools: ToolsState) => void;
}

interface ToolDef {
  key: keyof ToolsState;
  /** Render-time thunks so the strings re-read the active locale (spec 046 #8). */
  name: () => string;
  description: () => string;
}

const TOOL_DEFS: ToolDef[] = [
  {
    key: 'pixinsight',
    name: () => m.setup_tools_pixinsight_name(),
    description: () => m.setup_tools_pixinsight_desc(),
  },
  {
    key: 'siril',
    name: () => m.setup_tools_siril_name(),
    description: () => m.setup_tools_siril_desc(),
  },
];

/** Extensions recognized as executables across Windows/macOS/Linux (#511). */
const EXECUTABLE_EXTENSIONS = new Set(['exe', 'app', 'bin', 'sh', 'appimage']);

/**
 * Best-effort, no-exec heuristic for "is this path an executable" (#511
 * decision: detect/require an executable only, never spawn to verify
 * identity). A path with no extension is treated as plausibly valid --
 * that's the common case for Linux binaries (e.g. `siril`), which native
 * pickers can't filter by executable bit. Anything with a recognized
 * non-executable extension (e.g. the reported `.zip`) is rejected.
 */
function looksExecutable(path: string): boolean {
  const base = path.split(/[\\/]/).pop() ?? path;
  const dot = base.lastIndexOf('.');
  if (dot <= 0) return true; // no extension, or a dotfile -- Linux-typical
  const ext = base.slice(dot + 1).toLowerCase();
  return EXECUTABLE_EXTENSIONS.has(ext);
}

/**
 * Step 2 -- Processing Tools.
 * Auto-detects installed tools (`tools.discover`, application-based per OS) on mount,
 * then lets the user toggle/override the executable path.
 */
export function StepTools({ tools, onToolsChange }: StepToolsProps) {
  // Auto-detect installed tools once on mount (TanStack Query dedups a
  // StrictMode double-invoke via the shared query cache) and fill in any
  // unset paths. Detection is best-effort (spec 011): a failed/disabled
  // discovery just leaves `tools` unchanged, never surfaced as an error.
  const discoveryQuery = useQuery({
    queryKey: ['setup', 'toolsDiscover'] as const,
    queryFn: async () => {
      const { commands } = await import('@/bindings/index');
      return unwrap(await commands.toolsDiscover({ toolId: null }));
    },
    enabled: import.meta.env.VITE_USE_MOCKS !== 'true',
    retry: false,
  });

  useEffect(() => {
    const found = discoveryQuery.data;
    if (!found) return;
    const foundPaths = new Map(
      found.entries.filter((e) => e.available).map((e) => [e.toolId, e.path]),
    );
    let changed = false;
    const next: ToolsState = { ...tools };
    for (const def of TOOL_DEFS) {
      const path = foundPaths.get(def.key);
      if (path && !next[def.key].path) {
        next[def.key] = { enabled: true, path };
        changed = true;
      }
    }
    if (changed) onToolsChange(next);
    // Fires once when discovery data lands; merging only fills empty paths so
    // a StrictMode double-invoke re-running this with the same data is safe.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [discoveryQuery.data]);

  const handleToggle = (key: keyof ToolsState, checked: boolean) => {
    // Only flip `enabled`; keep the (detected or manually-set) path so disabling →
    // re-enabling doesn't lose detection and flip the tool back to "Not detected".
    onToolsChange({
      ...tools,
      [key]: { ...tools[key], enabled: checked },
    });
  };

  const handlePathChange = (key: keyof ToolsState, path: string | null) => {
    onToolsChange({
      ...tools,
      [key]: { ...tools[key], path },
    });
  };

  // Re-run auto-detection for a single tool (the "Redetect" button).
  const redetectMutation = useMutation({
    mutationFn: async (key: keyof ToolsState) => {
      const { commands } = await import('@/bindings/index');
      return unwrap(await commands.toolsDiscover({ toolId: key }));
    },
  });

  // Returns true if a binary was found (and the path was filled in), false
  // otherwise — ToolCard uses the result to show its "not found" message.
  const handleRedetect = async (key: keyof ToolsState): Promise<boolean> => {
    try {
      const data = await redetectMutation.mutateAsync(key);
      const entry = data.entries.find((e) => e.toolId === key && e.available);
      if (entry?.path) {
        onToolsChange({ ...tools, [key]: { enabled: true, path: entry.path } });
        return true;
      }
      return false;
    } catch {
      return false;
    }
  };

  return (
    <div className="pv-step-tools">
      <p className="pv-step-tools__intro">{m.setup_tools_intro()}</p>

      <div className="pv-step-tools__list">
        {TOOL_DEFS.map((def) => {
          const config = tools[def.key];
          return (
            <ToolCard
              key={def.key}
              def={def}
              config={config}
              onToggle={(checked) => handleToggle(def.key, checked)}
              onPathChange={(path) => handlePathChange(def.key, path)}
              onRedetect={() => handleRedetect(def.key)}
            />
          );
        })}
      </div>

      <p className="pv-step-tools__note">{m.setup_tools_skip_note()}</p>
    </div>
  );
}

/** A single tool card: name + detected status + description + enable toggle + path picker. */
// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: tool card render with per-tool detection and config branches
function ToolCard({
  def,
  config,
  onToggle,
  onPathChange,
  onRedetect,
}: {
  def: ToolDef;
  config: ToolConfig;
  onToggle: (checked: boolean) => void;
  onPathChange: (path: string | null) => void;
  onRedetect: () => Promise<boolean>;
}) {
  // One status pill per tool (#510): "Detected" only when a path is set AND
  // passes the executable heuristic; a set-but-invalid path (#511) reads as
  // "Invalid" rather than a false-positive "Detected"/"OK".
  const pathValid = config.path !== null && looksExecutable(config.path);
  const [redetecting, setRedetecting] = useState(false);
  const [notFound, setNotFound] = useState(false);
  const noInstallationFound = notFound;
  const statusLabel = redetecting
    ? m.setup_tools_detecting({ name: def.name() })
    : noInstallationFound
      ? m.setup_tools_no_installation()
      : config.path === null
        ? m.setup_tools_not_detected()
        : pathValid
          ? m.setup_tools_detected()
          : m.setup_tools_invalid();
  const statusVariant = redetecting
    ? 'warn'
    : noInstallationFound
      ? 'neutral'
      : config.path === null
        ? 'neutral'
        : pathValid
          ? 'ok'
          : 'danger';

  const handleRedetect = async () => {
    setRedetecting(true);
    setNotFound(false);
    const found = await onRedetect();
    setRedetecting(false);
    if (!found) setNotFound(true);
  };

  const handlePathChange = (path: string | null) => {
    setNotFound(false);
    onPathChange(path);
  };

  return (
    <div className="pv-step-tools__card" data-testid={`tool-card-${def.key}`}>
      {/* Header row: name + status pill + description + enable toggle */}
      <div className="pv-step-tools__header">
        <div className="pv-step-tools__tool-info">
          <div
            className="pv-step-tools__name-row"
            role="status"
            aria-live="polite"
            aria-atomic="true"
          >
            <span className="pv-step-tools__tool-name">{def.name()}</span>
            <Pill variant={statusVariant}>{statusLabel}</Pill>
          </div>
          <span className="pv-step-tools__tool-desc">{def.description()}</span>
        </div>
        <div className="pv-step-tools__controls">
          <div className="pv-step-tools__actions">
            <Btn
              variant="ghost"
              size="sm"
              onClick={handleRedetect}
              disabled={redetecting}
              aria-busy={redetecting}
              aria-label={m.setup_tools_redetect_binary_aria({
                name: def.name(),
              })}
              title={m.setup_tools_redetect()}
              className="pv-step-tools__redetect-btn"
            >
              <RotateCw
                size={14}
                aria-hidden="true"
                className={
                  redetecting
                    ? 'pv-step-tools__redetect-icon--spinning'
                    : undefined
                }
              />
            </Btn>
            <Toggle
              checked={config.enabled}
              onChange={onToggle}
              aria-label={m.settings_tools_enable_aria({ name: def.name() })}
            />
          </div>
        </div>
      </div>

      {/* Executable path picker, only when enabled */}
      {config.enabled && (
        <div className="pv-step-tools__path-block">
          <div className="pv-step-tools__path-row">
            <ToolPathPicker
              toolName={def.name()}
              path={config.path}
              onPathChange={handlePathChange}
            />
          </div>
          {config.path !== null && !pathValid && (
            <span className="pv-field-error">
              {m.setup_tools_invalid_executable()}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function ToolPathPicker({
  toolName,
  path,
  onPathChange,
}: {
  toolName: string;
  path: string | null;
  onPathChange: (path: string | null) => void;
}) {
  const { pick, loading } = useFilePicker();

  const handleChoose = async () => {
    // The processing tool's executable is a file (e.g. PixInsight.exe /
    // pixinsight / Siril), not a directory — pick the binary, not a folder.
    // No "All files" filter (#511): the native dialog enforces the
    // executable-only filter on Windows/macOS. Linux binaries are usually
    // extension-less and native dialogs there can't filter by executable
    // bit, so `looksExecutable` is the real safety net for that platform.
    const result = await pick([
      {
        name: m.setup_tools_executable_label(),
        extensions: ['exe', 'app', 'bin'],
      },
    ]);
    if (result.path) {
      onPathChange(result.path);
    }
  };

  return (
    <>
      <span className="pv-step-tools__path-label">
        {m.setup_tools_executable_label()}
      </span>
      <span
        className={
          'pv-mono pv-step-tools__path-value' +
          (path ? ' pv-step-tools__path-value--set' : '')
        }
        title={path ?? undefined}
      >
        {path ?? m.setup_tools_no_path()}
      </span>
      <Btn
        size="sm"
        onClick={handleChoose}
        disabled={loading}
        aria-label={m.setup_tools_select_binary_aria({ name: toolName })}
      >
        {loading ? m.setup_choosing() : m.setup_tools_select_binary()}
      </Btn>
    </>
  );
}
