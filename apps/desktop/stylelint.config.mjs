// Stylelint is adopted for ONE invariant: every `var(--pv-*)` in CSS resolves
// to a token that actually exists. No style preset — the repo is biome-first and
// this is not a second opinion on formatting.
//
// A CSS parser rather than a grep: 3,000+ `var()` references live in CSS, and
// the rule has to understand same-file scoping (component-local custom
// properties are legitimately declared next to their use), fallback syntax
// (`var(--x, 8px)`), and comments.
//
// A reference carrying a resolvable fallback is accepted by the rule even when
// the property is undefined. Component parameters therefore get an @property
// registration (tokens/properties.json) instead of an inline fallback, which is
// what makes them visible here.
export default {
  // TOP-LEVEL option, not a secondary option on the rule — globs here are
  // absolutized relative to this config file, which is what makes it work from
  // any cwd. Nesting it under the rule silently does nothing: the rule then
  // knows only same-file properties and reports every token as unknown.
  //
  // These are the GENERATED token artifacts, so the valid set is whatever the
  // design-token pipeline emitted; there is no second list to drift.
  referenceFiles: ['src/styles/tokens.css', '../../packages/tokens/tokens-docs.css'],
  rules: {
    'no-unknown-custom-properties': true,
  },
  ignoreFiles: ['dist/**', 'node_modules/**', '.ds-css/**'],
};
