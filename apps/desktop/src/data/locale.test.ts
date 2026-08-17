// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * locale.test.ts — spec 061 T012.
 *
 * Covers the parts of the locale runtime that don't need the settings-DB
 * mock (see locale.persistence.test.ts for DB-wins reconciliation):
 * strategy precedence order, and the compile-time fallback to the base
 * locale for a key missing from pt-BR.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

function setNavigatorLanguages(languages: string[]): void {
  Object.defineProperty(navigator, 'languages', {
    value: languages,
    configurable: true,
  });
}

describe('strategy precedence — custom-almSettings > preferredLanguage > baseLocale', () => {
  const originalLanguages = navigator.languages;

  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
  });

  afterEach(() => {
    setNavigatorLanguages([...originalLanguages]);
  });

  it('a saved choice in the mirror wins over the OS/webview language', async () => {
    setNavigatorLanguages(['pt-BR']);
    localStorage.setItem('pv.locale', 'en-GB');

    const { registerLocaleStrategy, getCurrentLocale } = await import(
      './locale'
    );
    registerLocaleStrategy();

    expect(getCurrentLocale()).toBe('en-GB');
  });

  it('falls back to the OS/webview language when nothing is saved', async () => {
    setNavigatorLanguages(['pt-BR']);

    const { registerLocaleStrategy, getCurrentLocale } = await import(
      './locale'
    );
    registerLocaleStrategy();

    expect(getCurrentLocale()).toBe('pt-BR');
  });

  it('falls back to the base locale when neither a saved choice nor a shipped OS language is available', async () => {
    setNavigatorLanguages(['fr-FR']);

    const { registerLocaleStrategy, getCurrentLocale, BASE_LOCALE } =
      await import('./locale');
    registerLocaleStrategy();

    expect(getCurrentLocale()).toBe(BASE_LOCALE);
    expect(BASE_LOCALE).toBe('en-GB');
  });

  it('ignores a malformed mirror value and falls through the chain', async () => {
    setNavigatorLanguages(['pt-BR']);
    localStorage.setItem('pv.locale', 'not-a-real-locale');

    const { registerLocaleStrategy, getCurrentLocale } = await import(
      './locale'
    );
    registerLocaleStrategy();

    expect(getCurrentLocale()).toBe('pt-BR');
  });
});

describe('missing-translation fallback (research D5)', () => {
  // The original version of this test asserted that `common_close` fell back
  // to English because it was absent from the pt-BR stub. That premise died
  // the moment the catalogue reached full coverage — it was a fixture that
  // had to stay artificially incomplete to keep passing, which is a trap, not
  // a guarantee. Whether a missing key falls back at all is Paraglide's
  // compile-time behaviour, not ours.
  //
  // What we actually need to hold is the user-visible half of D5: a reader on
  // pt-BR never sees a raw key or an empty string. That is checkable across
  // the whole catalogue rather than one hand-picked key, it stays true at any
  // coverage level, and if coverage regresses it still catches a bad fallback
  // (a missing key either renders English — acceptable — or renders raw —
  // caught here). Coverage drift itself is reported by
  // `scripts/check-i18n-locale-drift.mjs`.
  // Timeout bumped to 120 s: iterates all 2000+ catalogue entries; cold
  // import of the full paraglide bundle (one .js per catalogue key) can
  // exceed 15 s on the CI reference machine when the module cache is cold.
  it('no pt-BR message renders as a raw key or an empty string', {
    timeout: 120_000,
  }, async () => {
    const { m } = await import('@/lib/i18n');

    const offenders: string[] = [];
    let checked = 0;
    for (const [key, message] of Object.entries(m)) {
      if (typeof message !== 'function') continue;
      checked += 1;
      const value = (message as (p: object, o: { locale: string }) => string)(
        {},
        { locale: 'pt-BR' },
      );
      if (value === '' || value === key) offenders.push(key);
    }

    expect(offenders).toEqual([]);
    // Guard against the assertion above going vacuous: if `m` ever stops
    // exposing enumerable message functions (a barrel rewrite, a Proxy), the
    // loop would silently check nothing and still pass. The catalogue is 1856
    // keys; a floor well under that stays honest without being brittle as
    // keys are added.
    expect(checked).toBeGreaterThan(1000);
  });

  it('a key present in pt-BR resolves to the translated string', {
    timeout: 60_000,
  }, async () => {
    const { m } = await import('@/lib/i18n');

    expect(m.common_save({}, { locale: 'pt-BR' })).toBe('Salvar');
    expect(m.common_save({}, { locale: 'en-GB' })).toBe('Save');
  });
});
