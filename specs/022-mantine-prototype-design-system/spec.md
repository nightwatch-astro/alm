# Feature Specification: Desktop Prototype Design System

> **UI Revised**: The UI design in this spec was revised by
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md), which is
> superseded as of 2026-06-11. UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns.

**Feature Branch**: `022-mantine-prototype-design-system`
**Created**: 2026-05-09
**Last Revised**: 2026-05-20
**Status**: Superseded by spec 027 (Desktop Frontend Implementation)
**Input**: Original user description: "Specify that the prototype is Mantine-first,
uses standard Mantine components and TanStack Table/Router, writes DESIGN.md, and
avoids custom CSS or raw primitives unless necessary." Revised after the
design-system evaluation pass replaced Mantine with Base UI + a CSS-variable
token system.

## Supersession Notice

The original Mantine-first direction is **superseded**. The prototype's
design pass evaluated Mantine against alternatives and chose
`@base-ui-components/react` (Base UI) plus a small, explicit set of
companion libraries (`cmdk`, `react-resizable-panels`). The branch name
and feature directory are kept for git history continuity, but the title
and content reflect the actual implementation.

**Why the direction changed**:

1. **Token system fit**. The desktop already had a calibrated
   CSS-variable token system (`apps/desktop/src/styles/tokens.css`)
   covering color, typography, spacing, density, shadow, radius, and
   timing across light/dark/system modes. Mantine wants to own its own
   theme object and would have created two parallel sources of truth.
2. **No Tailwind buy-in**. Several Mantine-adjacent design system kits
   (shadcn/ui, Park UI) require Tailwind. The team explicitly does not
   want a utility-class layer; component CSS files keyed to design
   tokens are easier to audit against the constitution's "reviewable"
   posture.
3. **Headless behavior over styled components**. Base UI provides the
   accessibility plumbing (focus traps, ARIA wiring, keyboard handling,
   collision-aware popovers) without imposing visuals. The prototype
   pages own visual decisions through `alm-*`-prefixed CSS classes that
   read tokens.
4. **MUI accessibility chops**. Base UI is maintained by the MUI team;
   its primitives carry the same level of WCAG attention that Mantine
   offers, without the styling assumptions.
5. **Density and desktop affordances**. The product is a dense,
   keyboard-driven desktop app. A custom command palette (`cmdk`), a
   custom docked drawer (`react-resizable-panels`), and a custom data
   table (`@tanstack/react-table` rendered through `alm-table-*` CSS)
   give the prototype the desktop feel Mantine's defaults soften.

The original FRs are not deleted; they are restated below in their
revised form so future contributors do not re-litigate the choice.

## Foundational Principle: Prefer Standard Libraries

Before writing a custom UI primitive or interaction pattern, search for an
established, actively-maintained library that solves the problem. The bar is:
accessibility-aware, widely adopted, license-compatible, narrow scope (does
one thing well). Custom code carries an accessibility and maintenance tax that
should not be paid twice. This principle applies to all UI plans in this
project — per-feature `plan.md` documents MUST either reference the library
chosen or justify why a custom implementation was necessary.

Reference libraries currently in use (or chosen for a near-term feature):

| Concern | Library | Used by |
|---|---|---|
| Headless primitives | Base UI (`@base-ui-components/react`) | this spec |
| Anchor positioning | Floating UI (transitively via Base UI + Shepherd) | this spec, spec 010 |
| Command palette | `cmdk` | this spec |
| Layout / docked drawer | `react-resizable-panels` | this spec |
| Icons | `lucide-react` | this spec |
| Table primitive | `@tanstack/react-table` | this spec |
| Routing + URL state | `@tanstack/react-router` | this spec, spec 020 |
| Guided product tour | [Shepherd](https://github.com/shipshapecode/shepherd) | spec 010 |

New UI features SHOULD start with a library scan; if a primitive does not
exist, prefer composing two narrow libraries over building a general one.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Single Design Token Source (Priority: P1)

As a contributor styling a prototype page, I want every color,
spacing, type, density, shadow, radius, and timing value to resolve to
a CSS custom property defined in one tokens file so the look stays
coherent and a future theme switch only touches that file.

**Why this priority**: Without a single token source the design drifts
the moment two contributors style adjacent components. The token
system is the foundation every other story depends on.

**Independent Test**: Open `apps/desktop/src/styles/tokens.css` and a
random component CSS file. Confirm component rules reference `var(--…)`
tokens for color, spacing, radius, shadow, and motion, not hardcoded
hex/px/ms values.

**Acceptance Scenarios**:

1. **Given** a component CSS rule, **When** it sets a color or surface,
   **Then** it uses a `--bg`/`--surface-*`/`--text-*`/`--border-*`
   variable rather than a literal value.
2. **Given** the dark theme is active, **When** `data-theme="dark"` is
   set on `:root`, **Then** all tokens swap and no component needs
   theme-aware overrides beyond what tokens express.
3. **Given** a new color, spacing, or shadow is needed, **When** a
   contributor adds it, **Then** they add a token first and reference
   it from components, not the other way around.

---

### User Story 2 - Headless Primitive Library (Priority: P2)

As a contributor composing a page, I want a small set of accessible
primitives (`Button`, `IconButton`, `TextInput`, `Select`, `Switch`,
`Menu`, `Dialog`, `Tooltip`, `Accordion`, `Badge`, `Stepper`,
`StateLabel`, `EmptyState`, `PageHeader`, `Filters`, `DataTable`,
`DockedDrawer`, `DrawerShell`, `CommandPalette`, `LogPanel`,
`TokenPattern`) under `apps/desktop/src/ui/` so I can build pages
without re-deriving accessibility behavior.

**Why this priority**: The primitives are what contributors reach for
every day. They must exist, be uniform, and read tokens.

**Independent Test**: List `apps/desktop/src/ui/`. Confirm each
primitive imports Base UI or another headless library where applicable,
exposes a `className`-friendly surface, and has a matching block in
`apps/desktop/src/styles/components.css` keyed by `alm-*` selectors.

**Acceptance Scenarios**:

1. **Given** an interactive primitive (menu, dialog, tooltip,
   accordion, select), **When** the source is inspected, **Then** it
   wraps a Base UI headless primitive and applies `alm-*` CSS classes.
2. **Given** the command palette, **When** the source is inspected,
   **Then** it uses `cmdk` for fuzzy match plus a Base UI dialog for
   modal/focus behavior.
3. **Given** the docked drawer, **When** the source is inspected,
   **Then** it composes `react-resizable-panels` with `alm-drawer-*`
   visuals and respects the `--drawer-min-w`/`--drawer-max-w` tokens.
4. **Given** a primitive is added or changed, **When** review runs,
   **Then** the primitive does not bring in a styled-component runtime
   or a CSS-in-JS dependency.

---

### User Story 3 - Composable Component Vocabulary (Priority: P3)

As a contributor building a ledger page, I want feature pages to compose
the existing primitives (page header + filters + data table + drawer +
empty state) without inventing one-off layouts.

**Why this priority**: Pages share strong layout conventions; ad hoc
page shells were a major source of drift in earlier prototypes.

**Independent Test**: Inspect Inventory, Inbox, Projects, and Plans
pages. Confirm each uses `PageHeader`, `Filters`, `DataTable` (with
`@tanstack/react-table` state), and `DockedDrawer`/`DrawerShell` for
selection detail.

**Acceptance Scenarios**:

1. **Given** a ledger page, **When** it renders, **Then** it composes
   the primitives above instead of bespoke layout markup.
2. **Given** a ledger page selects a row, **When** the drawer opens,
   **Then** the drawer uses `DockedDrawer` and the page does not
   re-implement resize, persist, or close behavior.
3. **Given** a new ledger page is proposed, **When** review runs,
   **Then** it is rejected if it does not reuse the page shell
   primitives.

---

### User Story 4 - Theme Mode Switching (Priority: P4)

As a user, I want to choose system / light / dark mode and have the
app apply my choice immediately and across reloads.

**Why this priority**: Theme switching is a small surface but a
visible product affordance, and the contract for it is what locks the
token system in place.

**Independent Test**: Open Settings, switch between system, light, and
dark. Confirm the entire app re-themes without reload and that the
choice persists after a refresh.

**Acceptance Scenarios**:

1. **Given** the user picks "dark", **When** they reload, **Then** the
   app starts in dark mode using the stored preference.
2. **Given** the user picks "system", **When** the OS appearance
   changes, **Then** the app follows without user action.
3. **Given** a programmatic caller asks for the current theme, **When**
   it calls `theme.get`, **Then** it receives `{mode, resolved}` per
   the contract.
4. **Given** a programmatic caller wants to change the theme, **When**
   it calls `theme.set` with `system|light|dark`, **Then** the app
   applies it and echoes the new `{mode, resolved}`.

### Edge Cases

- A primitive needs an affordance Base UI does not provide
  (e.g. resizable docked panel) → compose a small headless companion
  library (`react-resizable-panels`) instead of forking Base UI.
- The host platform exposes a native control (Tauri title bar) →
  integrate at the shell boundary, not inside a primitive.
- The OS theme changes while the app is open and mode is "system" →
  resolved theme updates without code in feature pages.
- A future contributor wants to add Tailwind or CSS-in-JS → rejected;
  components consume tokens through plain CSS files keyed to
  `alm-*` classes.
- A component needs a value not in tokens → add the token first.

### Domain Questions To Resolve

- ~~Whether the token vocabulary should be exported as a generated
  TypeScript module for compile-time autocomplete in addition to CSS.~~
  **RESOLVED (R-022-TSDefer, GRILL 2026-05-22)**: TypeScript token
  autocomplete module deferred to v1.x. Tokens are enforced via review
  only in v1. See Out of Scope.
- ~~Whether the `alm-` prefix should be hardened with a CSS scope check
  or remain a convention.~~
  **RESOLVED (R-022-PrefixConvention, GRILL 2026-05-22)**: `alm-` prefix
  is a greppable convention only in v1. Reviewer enforces; no build-time
  check in v1. Build-time lint deferred to v1.x. See Out of Scope.
- ~~Whether DESIGN.md belongs at the repo root, in `docs/design/`, or in
  `apps/desktop/`.~~
  **RESOLVED (A1, GRILL 2026-05-22)**: DESIGN.md lives at the repo root
  (`/DESIGN.md`). The file already exists (created in the ui-revision-pass
  commit `314292a`). All spec references treat `/DESIGN.md` as the
  canonical design document.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The prototype MUST use `@base-ui-components/react` as the
  headless primitive layer for menu, dialog, tooltip, accordion,
  select, switch, and popover behavior.
- **FR-002**: The prototype MUST use `cmdk` for command palette fuzzy
  matching and keyboard navigation, composed with a Base UI dialog for
  focus and overlay behavior.
- **FR-003**: The prototype MUST use `react-resizable-panels` for the
  docked drawer/detail pane in ledger pages.
- **FR-004**: The prototype MUST use `@tanstack/react-table` for table
  state (rows, columns, sort, filter, selection) and render it through
  `DataTable` plus `alm-table-*` CSS classes.
- **FR-005**: The prototype MUST use `@tanstack/react-router` for
  routing, per spec 020.
- **FR-006**: All visual decisions MUST resolve to CSS custom
  properties declared in `apps/desktop/src/styles/tokens.css`. Hardcoded
  colors, spacing, radii, shadows, and motion durations are not
  permitted in component CSS. **Exception (D-022-1, GRILL 2026-05-22)**:
  `font-family` declarations MAY reference platform-native font-stack
  literals (e.g. `system-ui, -apple-system, sans-serif`) because font
  stacks are inherently platform strings, not design tokens. Token
  coverage for `font-size`, `font-weight`, and `line-height` still applies.
- **FR-007**: Component styles MUST live in `apps/desktop/src/styles/components.css`
  and use the `alm-` class prefix.
- **FR-008**: Reusable primitives MUST live under `apps/desktop/src/ui/`
  with one file per primitive and a barrel export in `index.ts`.
- **FR-009**: The prototype MUST NOT introduce Tailwind, CSS-in-JS
  runtimes, or styled-components.
- **FR-010**: The prototype MUST NOT introduce Mantine or shadcn/ui as
  runtime dependencies. Existing references in older docs MUST be
  treated as historical.
- **FR-011**: Theme mode handling MUST support `system`, `light`, and
  `dark`. The active mode MUST be reflected by `data-theme` on the
  document root (or its absence for `system`).
- **FR-012**: Theme switching MUST persist across reloads via
  `localStorage` under the key `alm.theme`.
- **FR-013**: The `theme.get` and `theme.set` contracts describe the
  UI-to-core boundary for theme reads and writes. **Softened
  (D-022-3, GRILL 2026-05-22)**: Theme contracts MAY be replaced by a
  backend-driven settings layer in a future revision; the v1
  `ThemeProvider` remains the canonical implementation. The contract
  shape is forward-compat only and does not block v1 implementation.
- **FR-014**: Primitive component APIs MUST accept `className` and
  spread remaining props onto the underlying root element to allow
  feature pages to extend behavior without forking primitives.
- **FR-015**: A `DESIGN.md` (or equivalent durable design doc) MUST
  document token taxonomy, primitive vocabulary, page composition
  rules, and the headless-library policy. Location is a domain
  question (see above).
- **FR-016**: UI copy MUST use functional product language and avoid
  AI-flavored labels (carried from the original spec).
- **FR-017** *(A2, GRILL 2026-05-22)*: The token system MUST support
  two density levels: `dense` and `comfortable`. The `--row-h` alias
  switches between `--row-h-dense` and `--row-h-comfortable`. A third
  `compact` level is deferred to v1.x.
- **FR-018** *(A3, GRILL 2026-05-22)*: A new reusable primitive MUST be
  added to `apps/desktop/src/ui/` when it is used in **3 or more**
  distinct feature contexts, OR when it encapsulates unique accessibility
  semantics (e.g. focus trapping, ARIA role composition) that would
  otherwise be duplicated. Single-use affordances MAY stay inline.
- **FR-019** *(A4, GRILL 2026-05-22)*: Adding a new design token to
  `tokens.css` MUST be accompanied by an update to `/DESIGN.md` and
  MUST pass adversarial review before merge. Reviewers verify that no
  existing token already covers the use case.

### Key Entities

- **Design Token**: A CSS custom property declared in `tokens.css` with
  a name, value, theme variants (light/dark/system), and a category
  (color, typography, spacing, density, shadow, radius, timing,
  z-index, shell-metric).
- **Primitive Component**: A `.tsx` file in `apps/desktop/src/ui/` that
  wraps a headless library and exposes a token-driven visual surface.
- **Component Style Block**: A rule set in `components.css` keyed by an
  `alm-*` class that resolves only to tokens.
- **Theme Mode**: One of `system`, `light`, `dark`.
- **Resolved Theme**: One of `light`, `dark` — the concrete mode in
  effect after resolving `system` against the OS preference.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A repository grep finds no Mantine, shadcn/ui, Tailwind,
  or styled-components imports in `apps/desktop/`.
- **SC-002**: A repository grep finds no hardcoded hex colors or `px`
  spacing values in `apps/desktop/src/styles/components.css` outside
  of token-emit lines and explicitly justified exceptions.
- **SC-003**: Every primitive under `apps/desktop/src/ui/` either wraps
  a Base UI / `cmdk` / `react-resizable-panels` element or is built on
  semantic HTML with documented rationale.
- **SC-004**: The desktop prototype's Inventory, Inbox, Projects, and
  Plans pages each compose `PageHeader`, `Filters`, `DataTable`, and
  `DockedDrawer`/`DrawerShell` rather than bespoke layout markup.
- **SC-005**: Theme switching is observable in Playwright MCP: toggling
  `system → light → dark` updates `:root[data-theme]` and persists on
  reload.

## Assumptions

- The CSS custom property approach is sufficient for the prototype and
  any planned production theme (no need for runtime theme objects).
- A small set of headless libraries is preferable to one large styled
  framework for a dense, keyboard-driven desktop app.
- TanStack Router and TanStack Table remain the navigation/table
  decisions (no change from the original spec).
- DESIGN.md, when it lands, will reference this spec, spec 015 (token
  pattern builder), and spec 020 (router/URL state) as anchors.

## Out of Scope

- A full themeable component library beyond what the desktop prototype
  needs today.
- Replacing Base UI internals or contributing back upstream.
- Pixel-perfect final production visual design.
- Mantine migration tooling (Mantine was never adopted at runtime).
- Tailwind, CSS-in-JS, or styled-components adoption.
- Multi-brand theming. Tokens describe one product brand; alternate
  brands are a future research topic.
- **TypeScript token autocomplete module** *(R-022-TSDefer, GRILL
  2026-05-22)*: Generating a `tokens.d.ts` / `tokens.ts` module for
  compile-time token name autocomplete is deferred to v1.x. Token
  correctness is enforced via code review only in v1.
- **Build-time `alm-` prefix lint** *(R-022-PrefixConvention, GRILL
  2026-05-22)*: A CI check enforcing the `alm-` class prefix is deferred
  to v1.x. The prefix is a greppable convention; reviewers enforce it
  manually in v1.
- **Compact density level** *(A2, GRILL 2026-05-22)*: A third density
  mode `compact` is deferred to v1.x. v1 ships `dense` and `comfortable`
  only.

## References

- Original Mantine direction: see git history of this file (this is a
  revision of the same `spec.md` rather than a separate retired file).
- Spec 015 (token pattern builder) for naming-pattern tokens.
- Spec 020 (router and URL state) for the routing decision.
- Implementation evidence: `apps/desktop/src/ui/`,
  `apps/desktop/src/styles/{tokens,reset,components}.css`,
  `apps/desktop/src/app/theme.tsx`.
