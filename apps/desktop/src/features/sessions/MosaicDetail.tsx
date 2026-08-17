// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * MosaicDetail — mosaic revision summary with panels, edges, and object evidence.
 *
 * Spec 062 US2: shows which panel groups form a mosaic, the adjacency edge
 * evidence (overlap, residual rotation, parity), and the captured-object
 * coverage results.
 *
 * FR-094: stale edges surface text + icon.
 * FR-040: canonical object identities deduplicated; per-panel containment
 *         available through the object evidence list.
 */

import { Skeleton, EmptyState, KV, Pill, Banner } from '@/ui';
import { m } from '@/lib/i18n';
import {
  useMosaic,
  useMosaicEdges,
  useMosaicObjectEvidence,
} from './useGroupsStore';
import { EvidenceSeverityPill } from './EvidenceSeverityPill';
import type { MosaicEdge, MosaicObjectEvidenceItem } from './groupsTypes';

// ── Edge row ───────────────────────────────────────────────────────────────────

function EdgeRow({ edge }: { edge: MosaicEdge }) {
  return (
    <tr
      className={`pv-mosaic-edge${edge.stale ? ' pv-mosaic-edge--stale' : ''}`}
      aria-label={m.mosaic_edge_row_aria({
        left: edge.leftPanelRevisionId,
        right: edge.rightPanelRevisionId,
      })}
    >
      <td className="pv-mosaic-edge__panels">
        <code>{edge.leftPanelRevisionId.slice(0, 8)}</code>
        {' ↔ '}
        <code>{edge.rightPanelRevisionId.slice(0, 8)}</code>
      </td>
      <td className="pv-mosaic-edge__overlap">
        {edge.overlapPercent.toFixed(1)}%
      </td>
      <td className="pv-mosaic-edge__rotation">
        {edge.residualSkyRotationDeg.toFixed(2)}°
      </td>
      <td className="pv-mosaic-edge__parity">
        <EvidenceSeverityPill
          severity={edge.parityMatch ? 'ok' : 'red'}
          detail={edge.parityMatch ? m.evidence_match() : m.evidence_mismatch()}
        />
      </td>
      <td className="pv-mosaic-edge__status">
        {edge.stale ? (
          <EvidenceSeverityPill severity="red" detail={m.mosaic_edge_stale()} />
        ) : (
          <EvidenceSeverityPill severity="ok" detail={m.mosaic_edge_valid()} />
        )}
      </td>
    </tr>
  );
}

// ── Object evidence row ────────────────────────────────────────────────────────

function ObjectEvidenceRow({ item }: { item: MosaicObjectEvidenceItem }) {
  const coverage = item.coverageState;
  return (
    <tr className="pv-mosaic-obj">
      <td className="pv-mosaic-obj__id">
        <code>{item.canonicalObjectId}</code>
      </td>
      <td className="pv-mosaic-obj__coverage">
        <EvidenceSeverityPill
          severity={coverage === 'full' ? 'ok' : 'yellow'}
          detail={
            coverage === 'full' ? m.mosaic_obj_full() : m.mosaic_obj_partial()
          }
        />
      </td>
      <td className="pv-mosaic-obj__panels">
        {m.mosaic_obj_panel_count({ count: item.panelContainmentRefs.length })}
      </td>
    </tr>
  );
}

// ── Main component ─────────────────────────────────────────────────────────────

export interface MosaicDetailProps {
  mosaicId: string;
}

export function MosaicDetail({ mosaicId }: MosaicDetailProps) {
  const { data: mosaicData, isLoading, isError } = useMosaic(mosaicId);

  const acceptedRevisionId = mosaicData?.acceptedHead.revisionId;

  const { data: edgesData, isLoading: edgesLoading } = useMosaicEdges(
    mosaicId,
    acceptedRevisionId,
  );
  const { data: objectsData, isLoading: objectsLoading } =
    useMosaicObjectEvidence(mosaicId, acceptedRevisionId);

  if (isLoading) {
    return (
      <div
        role="status"
        aria-live="polite"
        aria-label={m.mosaic_loading()}
        data-testid="mosaic-detail"
      >
        <Skeleton variant="block" />
      </div>
    );
  }

  if (isError || !mosaicData) {
    return (
      <EmptyState
        title={m.mosaic_error_title()}
        description={m.mosaic_error_desc()}
      />
    );
  }

  const { acceptedHead } = mosaicData;
  const edges = edgesData?.items ?? [];
  const objects = objectsData?.items ?? [];
  const staleEdgeCount = edges.filter((e) => e.edge.stale).length;

  return (
    <article
      className="pv-mosaic-detail"
      aria-label={m.mosaic_detail_aria({ id: mosaicId })}
      data-testid={`mosaic-detail-${mosaicId}`}
    >
      {/* Summary */}
      <section
        aria-label={m.mosaic_summary_heading()}
        className="pv-mosaic-summary"
      >
        <h3 className="pv-section__heading">{m.mosaic_summary_heading()}</h3>
        <dl className="pv-kv-list">
          <KV
            label={m.mosaic_panel_count_label()}
            value={acceptedHead.panelCount}
          />
          <KV
            label={m.mosaic_edge_count_label()}
            value={acceptedHead.edgeCount}
          />
          <KV
            label={m.mosaic_revision_label()}
            value={acceptedHead.revisionNumber}
          />
          <KV
            label={m.mosaic_accepted_at_label()}
            value={acceptedHead.acceptedAt}
          />
          {acceptedHead.intendedTargetId && (
            <KV
              label={m.mosaic_target_label()}
              value={<code>{acceptedHead.intendedTargetId}</code>}
            />
          )}
          {acceptedHead.retired && (
            <KV
              label={m.mosaic_status_label()}
              value={
                <Pill variant="warn">{m.sessions_group_retired_label()}</Pill>
              }
            />
          )}
        </dl>
      </section>

      {/* Stale edges warning */}
      {staleEdgeCount > 0 && (
        <Banner variant="warn" aria-live="polite">
          {m.mosaic_stale_edges_warning({ count: staleEdgeCount })}
        </Banner>
      )}

      {/* Adjacency edges */}
      <section
        aria-label={m.mosaic_edges_heading()}
        className="pv-mosaic-edges"
      >
        <h4 className="pv-section__subheading">{m.mosaic_edges_heading()}</h4>
        {edgesLoading ? (
          <Skeleton variant="block" />
        ) : edges.length === 0 ? (
          <EmptyState
            title={m.mosaic_edges_empty_title()}
            description={m.mosaic_edges_empty_desc()}
          />
        ) : (
          <table
            className="pv-mosaic-edges-table"
            aria-label={m.mosaic_edges_table_aria({ count: edges.length })}
          >
            <thead>
              <tr>
                <th scope="col">{m.mosaic_col_panels()}</th>
                <th scope="col">{m.mosaic_col_overlap()}</th>
                <th scope="col">{m.mosaic_col_rotation()}</th>
                <th scope="col">{m.mosaic_col_parity()}</th>
                <th scope="col">{m.mosaic_col_status()}</th>
              </tr>
            </thead>
            <tbody>
              {edges.map((row) => (
                <EdgeRow key={row.edge.edgeId} edge={row.edge} />
              ))}
            </tbody>
          </table>
        )}
      </section>

      {/* Object evidence */}
      <section
        aria-label={m.mosaic_objects_heading()}
        className="pv-mosaic-objects"
      >
        <h4 className="pv-section__subheading">{m.mosaic_objects_heading()}</h4>
        {objectsLoading ? (
          <Skeleton variant="block" />
        ) : objects.length === 0 ? (
          <EmptyState
            title={m.mosaic_objects_empty_title()}
            description={m.mosaic_objects_empty_desc()}
          />
        ) : (
          <table
            className="pv-mosaic-objects-table"
            aria-label={m.mosaic_objects_table_aria({ count: objects.length })}
          >
            <thead>
              <tr>
                <th scope="col">{m.mosaic_col_object()}</th>
                <th scope="col">{m.mosaic_col_coverage()}</th>
                <th scope="col">{m.mosaic_col_panels_count()}</th>
              </tr>
            </thead>
            <tbody>
              {objects.map((item) => (
                <ObjectEvidenceRow key={item.canonicalObjectId} item={item} />
              ))}
            </tbody>
          </table>
        )}
      </section>
    </article>
  );
}
