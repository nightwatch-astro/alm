// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { PROJECT_LIFECYCLE, projectStateIndex } from '@/lib/lifecycle';
import { m } from '@/lib/i18n';
import {
  root,
  step as stepCls,
  connector as connectorCls,
  dotVariants,
  lineVariants,
  labelVariants,
} from './lifecycle.css';

export interface LifecycleProps {
  /** Stored project state (e.g. "processing", "setup_incomplete", "blocked"). */
  state: string;
}

/**
 * Vertical lifecycle flowchart for a project's state. Renders the centralized
 * PROJECT_LIFECYCLE steps with done/active styling, plus a trailing blocked
 * marker when the project is off-track. Lives in the detail rail (design v4).
 */
export function Lifecycle({ state }: LifecycleProps) {
  // `state` is an arbitrary string; `projectStateIndex` is now keyed by the
  // exhaustive `ProjectState` union, so coerce at the lookup and fall back to
  // -1 (off-track) for any unknown value.
  const currentIdx =
    projectStateIndex[state as keyof typeof projectStateIndex] ?? -1;
  const isBlocked = state === 'blocked';

  return (
    <div className={root} data-testid="lifecycle">
      {PROJECT_LIFECYCLE.map((label, i) => {
        const isDone = !isBlocked && i < currentIdx;
        const isCurrent = !isBlocked && i === currentIdx;

        const dotKey = isDone ? 'done' : isCurrent ? 'active' : 'default';
        const labelKey = isDone ? 'done' : isCurrent ? 'active' : 'default';

        return (
          <div key={label} className={stepCls}>
            <div className={connectorCls}>
              {i > 0 && (
                <div
                  className={
                    lineVariants[isDone || isCurrent ? 'done' : 'default']
                  }
                />
              )}
              <div className={dotVariants[dotKey]} />
              {i < PROJECT_LIFECYCLE.length - 1 && (
                <div className={lineVariants[isDone ? 'done' : 'default']} />
              )}
            </div>
            <span className={labelVariants[labelKey]}>{label}</span>
          </div>
        );
      })}
      {isBlocked && (
        <div className={stepCls}>
          <div className={connectorCls}>
            <div className={lineVariants.default} />
            <div className={dotVariants.blocked} />
          </div>
          <span className={labelVariants.danger}>
            {m.projects_stepper_blocked_chip()}
          </span>
        </div>
      )}
    </div>
  );
}
