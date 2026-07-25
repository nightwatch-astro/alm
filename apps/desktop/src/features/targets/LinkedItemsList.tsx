// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * LinkedItemsList — renders a list of linked sessions OR projects with
 * navigation. Parameterized to eliminate the twin map bodies in
 * TargetDetail.
 */

import { useNavigate } from '@tanstack/react-router';
import { StatusTag } from '@/components';
import { Skeleton } from '@/ui';
import { m } from '@/lib/i18n';
import { projectStateLabel, projectStateVariant } from '@/lib/lifecycle';

// ── Linked sessions ──────────────────────────────────────────────────────────

export interface LinkedSession {
  id: string;
  createdAt: string;
  filter: string;
  frameCount: number;
}

export interface LinkedSessionsListProps {
  sessions: LinkedSession[];
  loading: boolean;
}

export function LinkedSessionsList({
  sessions,
  loading,
}: LinkedSessionsListProps) {
  const navigate = useNavigate();

  if (loading) {
    return <Skeleton count={3} width="80%" label={m.common_loading()} />;
  }

  if (sessions.length === 0) {
    return (
      <span className="pv-planner__link-empty">
        {m.targets_detail_no_sessions()}
      </span>
    );
  }

  return (
    <ul className="pv-planner__link-list">
      {sessions.map((s) => {
        const dateStr = new Date(s.createdAt).toLocaleDateString(undefined, {
          year: 'numeric',
          month: 'short',
          day: 'numeric',
        });
        return (
          <li key={s.id} className="pv-planner__link-item">
            <button
              className="pv-planner__link-btn"
              onClick={() =>
                void navigate({
                  to: '/sessions',
                  search: { selected: s.id },
                })
              }
            >
              <span className="pv-planner__link-date">{dateStr}</span>
              {s.filter !== '' && (
                <span className="pv-planner__link-meta">{s.filter}</span>
              )}
              <span className="pv-planner__link-meta">
                {m.targets_detail_session_frames({ count: s.frameCount })}
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

// ── Linked projects ──────────────────────────────────────────────────────────

export interface LinkedProject {
  id: string;
  name: string;
  lifecycle: string;
}

export interface LinkedProjectsListProps {
  projects: LinkedProject[];
  loading: boolean;
}

export function LinkedProjectsList({
  projects,
  loading,
}: LinkedProjectsListProps) {
  const navigate = useNavigate();

  if (loading) {
    return <Skeleton count={3} width="80%" label={m.common_loading()} />;
  }

  if (projects.length === 0) {
    return (
      <span className="pv-planner__link-empty">
        {m.targets_detail_no_projects_linked()}
      </span>
    );
  }

  return (
    <ul className="pv-planner__link-list">
      {projects.map((p) => (
        <li key={p.id} className="pv-planner__link-item">
          <button
            className="pv-planner__link-btn"
            onClick={() =>
              void navigate({
                to: '/projects',
                search: { selected: p.id },
              })
            }
          >
            <span className="pv-planner__link-name">{p.name}</span>
            {/* #739 US3-AC2: lifecycle carries the shared tone (and
                localized label) every other project surface uses. */}
            <StatusTag variant={projectStateVariant(p.lifecycle)}>
              {projectStateLabel(p.lifecycle)}
            </StatusTag>
          </button>
        </li>
      ))}
    </ul>
  );
}
