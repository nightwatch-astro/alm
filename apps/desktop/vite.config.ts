// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import { vanillaExtractPlugin } from "@vanilla-extract/vite-plugin";
import { paraglideVitePlugin } from "@inlang/paraglide-js";
import { resolve } from "path";
import { readFileSync } from "node:fs";

const appVersion = JSON.parse(
  readFileSync(resolve(__dirname, "package.json"), "utf8"),
).version as string;

export default defineConfig(({ mode, command }) => {
  // Precedence: real OS env var > .env file > default ("false" = real backend).
  // NOTE: a `define` on import.meta.env.VITE_USE_MOCKS is merged into the env
  // object and OVERRIDES .env, so it must reflect the resolved value.
  // Browser-only dev (no Tauri host) must opt into mocks: VITE_USE_MOCKS=true.
  const fileEnv = loadEnv(mode, resolve(__dirname), "");
  const useMocks =
    process.env.VITE_USE_MOCKS ?? fileEnv.VITE_USE_MOCKS ?? "false";
  // VITE_DEV_TOOLS: set to "true" only in dev-tools builds (mirrors the
  // Cargo `dev-tools` feature). Release builds omit the flag so the entire
  // dev surface is tree-shaken by the bundler. Default is "false".
  const devTools =
    process.env.VITE_DEV_TOOLS ?? fileEnv.VITE_DEV_TOOLS ?? "false";
  if (command === "serve") {
    // eslint-disable-next-line no-console
    console.log(`[vite] VITE_USE_MOCKS="${useMocks}" VITE_DEV_TOOLS="${devTools}" (mode=${mode})`);
  }

  return {
    plugins: [
      vanillaExtractPlugin(),
      // Compile the message catalog (messages/*.json → src/paraglide/) on dev
      // start + build, with HMR when a message changes. Strategy chain (spec
      // 061 research D1), evaluated in order: a user's saved choice
      // ("custom-almSettings", src/data/locale.ts) beats the OS/webview
      // language ("preferredLanguage"), which beats the compiled-in
      // "baseLocale" (en-GB) fallback. This supersedes the earlier hard-pinned
      // English of spec 046 FR-004 — that constraint is retired by spec 061.
      // The generated src/paraglide/ output is git-ignored.
      paraglideVitePlugin({
        project: "./project.inlang",
        outdir: "./src/paraglide",
        strategy: ["custom-almSettings", "preferredLanguage", "baseLocale"],
        // Emit .d.ts alongside the compiled .js so a bare `tsc --noEmit` (which
        // does not run Vite) always finds declarations for `@/paraglide/*` —
        // keeps every compile path (dev, build, vitest, typecheck) consistent.
        emitTsDeclarations: true,
      }),
      react(),
    ],
    clearScreen: false,
    server: {
      port: 5173,
      strictPort: true,
    },
    resolve: {
      alias: {
        "@": resolve(__dirname, "./src"),
      },
    },
    // Handoff 07: splash.html is a second Vite entry (own HTML + module
    // script, not React) alongside the main SPA shell — Tauri's `splash`
    // window loads it directly from the built `dist/` root.
    build: {
      rollupOptions: {
        input: {
          main: resolve(__dirname, "index.html"),
          splash: resolve(__dirname, "splash.html"),
        },
      },
    },
    define: {
      "import.meta.env.VITE_USE_MOCKS": JSON.stringify(useMocks),
      "import.meta.env.VITE_DEV_TOOLS": JSON.stringify(devTools),
      // Fix-lane round 5 (PR #477): cheap build-staleness marker, dumped by
      // the real-UI E2E harness (`window.__PV_E2E__.buildTime`) so a failing
      // Windows journey can show whether the served dist was actually built
      // from the commit under test, rather than a stale CI cache artifact.
      // Baked in at config-eval time (build wall-clock), not a git SHA — no
      // extra plumbing needed beyond this one `define`.
      "import.meta.env.VITE_BUILD_TIME": JSON.stringify(new Date().toISOString()),
      // Splash screen version label (handoff 07) — sourced from package.json
      // so it can't drift from the app's own version.
      "import.meta.env.VITE_APP_VERSION": JSON.stringify(appVersion),
    },
  };
});
