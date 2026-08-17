# Feature Specification: Selectable Application Language

**Feature Branch**: `061-selectable-app-language`

**Created**: 2026-07-20

**Status**: Implemented

**Input**: User description: "Selectable application language with en-GB and pt-BR"

## Context

Spec 046 built the message catalog and the lint gate that keeps user-facing
strings out of the code. PR #1258 finished the job: 1856 keys, zero unused,
zero grandfathered lint violations. Every string in the app is *translatable*.

None of them are *translated*. The app ships one locale and pins it at build
time, so a user who does not read English has no recourse. This feature makes
the language a choice the user makes and the app remembers.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Choose a language before anything else (Priority: P1)

A first-time user opens the app and does not read English. Before they are
asked about observing sites, library roots, or processing tools — all of which
are explained in prose — they pick the language they read. Every subsequent
step of setup, and the whole app afterwards, is in that language.

**Why this priority**: it is the only story that stands alone. Setup asks
consequential, irreversible-feeling questions about the user's file library;
asking them in a language the user cannot read is where a non-English user
abandons the product. A language step that works is a viable MVP even if the
settings control ships later.

**Independent Test**: launch with no prior configuration, select
Português (Brasil) on the first screen, and confirm every following wizard step
renders in Portuguese.

**Acceptance Scenarios**:

1. **Given** a first run with no saved preferences, **When** the wizard opens,
   **Then** the first step presented is the language chooser, before the
   observing-site, library-root, and processing-tool steps.
2. **Given** the language chooser, **When** the user selects
   Português (Brasil), **Then** the wizard's own interface changes to
   Portuguese immediately, without a reload and without losing wizard progress.
3. **Given** a language was chosen during setup, **When** setup completes and
   the main app opens, **Then** the app is in that language.
4. **Given** the language chooser, **When** the user navigates by keyboard
   only, **Then** every language option is reachable and selectable, and the
   focused option shows a visible focus ring.
5. **Given** the user has advanced past the language step, **When** they use
   the wizard's Back navigation, **Then** they return to the language step and
   can change the choice — a language selected by mistake is recoverable
   without completing setup first.

---

### User Story 2 - Change language later (Priority: P2)

A user who completed setup in one language wants a different one — they chose
in haste, or someone else uses the machine.

**Why this priority**: valuable but not survival-critical; a user who got the
first choice right never needs it. It depends on the same persistence
machinery as P1, so it is cheap once P1 exists.

**Independent Test**: change the language in Settings, confirm the app
re-renders, restart the app, and confirm the choice survived.

**Acceptance Scenarios**:

1. **Given** the Settings → Appearance pane, **When** the user selects a
   different language, **Then** the interface applies it live with no reload,
   consistent with how the theme and density controls already behave.
2. **Given** a language chosen in Settings, **When** the app is fully closed
   and relaunched, **Then** the chosen language is still in effect.
3. **Given** the language control, **When** it is rendered, **Then** each
   option shows both its flag and its own native name, so a user who cannot
   read the current interface language can still identify their own.

---

### User Story 3 - Read Portuguese that reads like Portuguese (Priority: P3)

A Brazilian user reads the interface and finds it grammatical — plurals agree,
words are not stitched together in English order, and terms that share one
English word are correctly distinguished.

**Why this priority**: the mechanism must exist from the start, but quality is
iterative and improves with native review after launch.

**Independent Test**: exercise screens containing counts, and screens where one
English word serves two grammatical roles, and confirm each renders correctly
in Portuguese.

**Acceptance Scenarios**:

1. **Given** a screen showing a count of items, **When** the count is 1 versus
   many, **Then** Portuguese uses the correct singular and plural form.
2. **Given** two places where English shows the same word for different
   meanings, **When** viewed in Portuguese, **Then** each renders its own
   correct term.
3. **Given** a locale needs a grammatical distinction English does not make,
   **When** that locale supplies it, **Then** no change is required to the
   base locale or to any calling code.

### Edge Cases

- A locale file is missing a key that the base locale has → the interface MUST
  fall back to the base locale for that string rather than showing a raw key or
  an empty region.
- The saved language names a locale removed from the shipped set → the app MUST fall
  back to the base locale rather than failing to start.
- The user cancels or skips the first-run wizard entirely → the app MUST still
  have a defined language.
- A language is changed while a long-running operation is in progress → the
  operation MUST NOT be interrupted, cancelled, or have its progress reset.
- The language list grows beyond the visible panel height → the panel MUST
  scroll, and keyboard navigation MUST reach options below the fold.
- A translated string is materially longer than its English source → layout
  MUST accommodate it without clipping or overlap.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The application MUST support more than one interface language,
  and MUST ship with English (UK) and Português (Brasil).
- **FR-002**: The interface language MUST be a user preference, not a
  build-time constant. This supersedes spec 046 FR-004, which pinned the
  application to a single language and deferred the switcher.
- **FR-003**: The user's language choice MUST persist across a full application
  restart.
- **FR-004**: A language change MUST apply immediately to the running
  interface, without a reload and without losing unsaved context.
- **FR-005**: The first-run setup wizard MUST present language selection as its
  first step, ahead of every other step.
- **FR-006**: The Settings interface MUST offer language selection alongside
  the existing appearance preferences.
- **FR-007**: Every language option MUST be labelled with its own native name
  (`English (UK)`, `Português (Brasil)`) accompanied by a flag. The native name
  MUST be present; a flag alone is insufficient, because flags denote countries
  rather than languages.
- **FR-008**: The language chooser MUST be fully keyboard operable and MUST
  expose the current selection to assistive technology.
- **FR-009**: When a locale lacks a translation for a key, the application MUST
  fall back to the base locale for that string, and MUST NOT display a raw
  message key.
- **FR-010**: Each locale MUST be able to declare grammatical variations —
  plural forms, gendered or case-inflected terms — that the base locale does
  not make, without requiring changes to the base locale or to calling code.
- **FR-011**: Language-dependent behaviour MUST NOT be driven by matching on
  rendered English text. Distinctions MUST be resolved at the message-catalog
  level. (Rationale: text matching re-introduces English-keyed logic, the exact
  class of defect spec 046's lint gate exists to prevent, and it is invisible
  to that gate.)
- **FR-012**: Keys that share an English value but differ in meaning MUST
  remain separate keys and MUST NOT be consolidated on the basis that their
  English text matches.
- **FR-013**: Each shipped locale MUST declare a review status. A translation
  that has not been reviewed by a fluent speaker MUST be marked as
  machine-generated or otherwise unreviewed, so review status is a known
  quantity rather than an assumption.
- **FR-014**: Each shipped locale MUST contain exactly the base locale's
  message-key set. The CI locale-drift check MUST fail when a locale is missing
  a base key or contains a key absent from the base locale.

### Key Entities

- **Locale**: an interface language the application can present. Identified by
  a standard language tag, and carrying the native display name and flag shown
  in the chooser.
- **Language preference**: the user's chosen locale, durable across restarts,
  sitting alongside the existing appearance preferences.
- **Message catalog**: the per-locale set of user-facing strings. One catalog
  is the base and defines the complete key set; every shipped catalog matches
  that set, while runtime fallback protects the interface from malformed input.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user who reads no English can complete first-run setup
  end-to-end in Portuguese.
- **SC-002**: The language chosen at any point is still in effect after a full
  application restart, verified on Windows and Linux.
- **SC-003**: Changing the language re-renders the interface with no reload and
  no loss of in-progress work.
- **SC-004**: No screen displays a raw message key or an untranslated-looking
  gap in either shipped locale.
- **SC-005**: Every language option is reachable and selectable using only the
  keyboard, and is announced correctly by a screen reader.
- **SC-006**: Count-bearing strings render the grammatically correct form in
  both locales at counts of 0, 1, and many.
- **SC-007**: The application starts in a usable language even when the stored
  preference names a locale removed from the shipped set.

## Assumptions

- Portuguese (Brazil) is the second locale because it was explicitly requested;
  no other locale is in scope for this feature.
- Initial Portuguese translations will be machine-generated and are expected to
  need native review. Shipping them is preferable to shipping nothing, provided
  their review status is explicit (FR-013).
- The existing appearance preferences (theme, density, font size) establish the
  pattern for live-apply and persistence; language follows that pattern rather
  than inventing a new one.
- The message catalog is complete and enforced as of PR #1258 (1856 keys, zero
  unused, zero grandfathered lint violations), so this feature adds locales
  rather than extracting strings.
- The 98 English values shared across 223 keys are deliberate context splits,
  not duplication to be cleaned up. Portuguese is expected to diverge on some
  of them — `Archive` as a verb versus as a stored state being the known case.
- Right-to-left languages are out of scope; neither shipped locale requires
  bidirectional layout support.
