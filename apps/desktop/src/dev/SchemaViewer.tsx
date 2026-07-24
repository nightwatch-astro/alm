// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SchemaViewer — pretty-prints a JSON Schema file and allows copy to clipboard
 * (spec 021 US3).
 *
 * Schema content is fetched server-side via `dev.schema.get` so no client-side
 * filesystem plugin is needed. When the file is absent, shows a `schema.missing`
 * error state.
 */

import { useState, useEffect } from 'react';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import {
  overlay as schemaOverlay,
  panel as schemaPanel,
  header as schemaHeader,
  name as schemaName,
  ver as schemaVersion,
  actions as schemaActions,
  body as schemaBody,
  missing as schemaMissing,
  missingPath as schemaMissingPath,
  missingCode as schemaMissingCode,
  loading as schemaLoading,
  pre as schemaPre,
} from './schema-viewer.css';

interface SchemaViewerProps {
  /** Absolute path to the JSON Schema file. */
  schemaPath: string;
  /** Contract version pinned on the call (may differ from registry current). */
  contractVersion: string;
  /** Contract name for display. */
  contractName: string;
  onClose: () => void;
}

export function SchemaViewer({
  schemaPath,
  contractVersion,
  contractName,
  onClose,
}: SchemaViewerProps) {
  const [content, setContent] = useState<string | null>(null);
  const [missing, setMissing] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setContent(null);
    setMissing(false);

    commands
      .devSchemaGet({ schemaPath })
      .then(unwrap)
      .then((resp) => {
        if (cancelled) return;
        if (resp.found && resp.content != null) {
          setContent(resp.content);
        } else {
          setMissing(true);
        }
      })
      .catch(() => {
        if (!cancelled) setMissing(true);
      });

    return () => {
      cancelled = true;
    };
  }, [schemaPath]);

  const handleCopy = async () => {
    if (!content) return;
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard unavailable in non-HTTPS context (e.g. tests).
    }
  };

  return (
    <div
      role="dialog"
      aria-label={`Schema viewer: ${contractName} v${contractVersion}`}
      className={schemaOverlay}
    >
      <div className={schemaPanel}>
        <div className={schemaHeader}>
          <div>
            <span className={schemaName}>{contractName}</span>
            <span className={schemaVersion}>v{contractVersion}</span>
          </div>
          <div className={schemaActions}>
            <button
              type="button"
              className="pv-btn pv-btn--sm"
              onClick={handleCopy}
              disabled={!content}
              aria-label="Copy schema to clipboard"
            >
              {copied ? 'Copied!' : 'Copy'}
            </button>
            <button
              type="button"
              className="pv-btn pv-btn--sm"
              onClick={onClose}
              aria-label="Close schema viewer"
            >
              Close
            </button>
          </div>
        </div>

        <div className={schemaBody}>
          {missing ? (
            <div
              role="alert"
              data-testid="schema-missing"
              className={schemaMissing}
            >
              <strong>schema.missing</strong>
              <p className={schemaMissingPath}>
                Schema file not found at:{' '}
                <code className={schemaMissingCode}>
                  {schemaPath || '(no path)'}
                </code>
              </p>
            </div>
          ) : content === null ? (
            <div className={schemaLoading}>Loading schema…</div>
          ) : (
            <pre data-testid="schema-content" className={schemaPre}>
              {content}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}
