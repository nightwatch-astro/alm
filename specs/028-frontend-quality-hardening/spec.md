# Feature Specification: Frontend Quality Hardening

> **Obsolete as a work queue (2026-08-17)**: this spec's last substantive item
> named `ProjectDetailPane`, a component that does not exist. The shipped
> equivalent is
> `apps/desktop/src/features/projects/ProjectBottomDetail.tsx`. UI design is
> owned by [Spec 032 — Design V4](../032-design-v4-implementation/spec.md).
> Reference only; task state lives in beads.

**Feature Branch**: `028-frontend-quality-hardening`
**Created**: 2026-05-25
**Status**: Obsolete
**Input**: Surviving quality tasks from superseded specs 020 (Router & URL State) and 022 (Design System), plus new coverage needs from spec 027 (Desktop Frontend Implementation).

**Supersedes**: Remaining open tasks from spec 020 and spec 022.

## Scope

This spec consolidates frontend quality, hardening, and CI tasks that were
deferred or left incomplete when specs 020 and 022 were superseded by the
spec 027 full frontend rewrite.

### URL State & Router Contracts (from spec 020)

- Typed `RouteContract` per route with `validateSearch` / `parseSearch` helpers
- `useSearch` / `useNavigate` refactors replacing ad-hoc URL manipulation
- Deep-link resolution for entity refs (`?selected=session:uuid`)
- Back-button and browser-history state correctness
- Vitest coverage for route redirects, search parsing, unknown-route fallthrough

### Design System Quality (from spec 022)

- Token completeness audit: every color, spacing, radius, shadow, and motion
  duration in `components.css` must reference a `tokens.css` variable
- CI grep guard: fail build on raw hex colors, raw px (outside token blocks),
  or raw ms values in component styles
- TypeScript token type generation (`tokens.d.ts`) for autocomplete (deferred
  from 022 to post-v1, re-evaluate priority)
- Primitive prop audit: every `ui/` primitive accepts `className` and spreads
  remaining props onto root element
- `DESIGN.md` sync: update root `/DESIGN.md` with current token taxonomy,
  primitive vocabulary, page composition rules, density levels

### Test Coverage (new from spec 027)

- Vitest unit tests for UI primitives and utility modules
- Component integration tests for critical flows (setup wizard, review queue
  decision cycle, project wizard)

### CI & Automation

- Pre-commit or CI check for unused exports (`knip`)
- Circular import detection (`madge`)
- Bundle size baseline and regression guard

