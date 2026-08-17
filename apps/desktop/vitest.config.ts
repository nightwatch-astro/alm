// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { vanillaExtractPlugin } from "@vanilla-extract/vite-plugin";
import { resolve } from "path";

export default defineConfig({
  plugins: [vanillaExtractPlugin(), react()],
  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
    },
  },
  define: {
    // Tests do NOT enable dev-tools; the release gate checks this is "false".
    // VITE_USE_MOCKS is intentionally not set here — tests that need mocks
    // use vi.mock('@tauri-apps/api/core') directly (see recorder.test.ts,
    // source-views.test.ts). WizardPage tests use vi.mock for store/commands.
    "import.meta.env.VITE_DEV_TOOLS": JSON.stringify("false"),
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./vitest.setup.ts"],
    // Compiles the generated Paraglide catalogue before any suite loads.
    // package.json's `pretest` hook only runs for `pnpm test`; invoking
    // vitest directly (single file, watch mode, IDE runner) skipped it, and
    // every suite then failed at import on `@/paraglide/messages`. A
    // globalSetup runs once per vitest process whatever the entrypoint.
    // See vitest.globalSetup.ts.
    globalSetup: ["./vitest.globalSetup.ts"],
    css: false,
    // Vitest's 5s default is too tight for jsdom + React render + waitFor
    // chains once the runner is CPU-contended — and CI runners are contended
    // by definition (2-4 cores running all 186 suites in parallel).
    //
    // Reproduced locally: the full suite is green at load ~1, but saturating
    // all 12 cores makes UNRELATED suites fail with a bare
    // "Error: Test timed out in 5000ms". GuidedOverlay and devSurface.release
    // are the reliable casualties — exactly the suites that went red on
    // macos-latest / windows-latest without any relevant code change.
    //
    // A timeout ceiling only costs wall-clock when a test is ACTUALLY stuck,
    // so raising it does not slow the passing path. It stops a slow machine
    // from being misreported as a broken test.
    testTimeout: 15_000,
    hookTimeout: 15_000,
    // Exclude Playwright e2e specs and any node_modules.
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    exclude: ["node_modules/**", "dist/**", "../../tests/e2e/**"],
    passWithNoTests: true,
  },
});
