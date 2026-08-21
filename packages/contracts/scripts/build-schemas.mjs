import { existsSync, mkdirSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { spawnSync } from "node:child_process";

const schemasDir = new URL("../schemas", import.meta.url).pathname;
const specsDir = new URL("../../../specs", import.meta.url).pathname;
const generatedDir = new URL("../src/generated", import.meta.url).pathname;
const packageDir = new URL("..", import.meta.url).pathname;

// `json2ts` is a dependency binary, not a global. `spawnSync` without a shell
// does not consult the package-manager-injected PATH, so resolve the bin
// explicitly: pnpm puts it in this package's node_modules, npm/hoisted layouts
// in the workspace root. Without this every invocation failed with ENOENT,
// which the old script reported as "json2ts failed on <schema>" — indis-
// tinguishable from a genuine schema error.
function resolveJson2Ts() {
  const candidates = [
    join(packageDir, "node_modules/.bin/json2ts"),
    join(packageDir, "../../node_modules/.bin/json2ts"),
    join(packageDir, "../../node_modules/.pnpm/node_modules/.bin/json2ts"),
  ];
  const found = candidates.find((path) => existsSync(path));
  if (!found) {
    console.error(
      "json2ts binary not found. Install dependencies first (`pnpm install`).\nLooked in:\n" +
        candidates.map((c) => `  ${c}`).join("\n"),
    );
    process.exit(1);
  }
  return found;
}

function findSchemas(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      return findSchemas(path);
    }
    return entry.isFile() && entry.name.endsWith(".schema.json") ? [path] : [];
  });
}

// SpecKit-managed contracts live under `specs/<NNN>-<slug>/contracts/*.json`.
// Only the contracts listed below are wired into the TS generation pipeline —
// other specs reference remote `$ref`s that json2ts cannot resolve offline.
// Add to this allowlist as each spec's contracts settle.
const SPEC_CONTRACT_ALLOWLIST = [
  "002-data-lifecycle-state-model/contracts/lifecycle.transition.json",
  "002-data-lifecycle-state-model/contracts/provenance.read.json",
  "003-first-run-source-setup/contracts/roots.register.json",
  "003-first-run-source-setup/contracts/roots.register.batch.json",
  "003-first-run-source-setup/contracts/firstrun.complete.json",
  "003-first-run-source-setup/contracts/firstrun.restart.json",
  "003-first-run-source-setup/contracts/audit.first_run.completed.json",
  "004-native-filesystem-controls/contracts/native.directory.pick.json",
  "004-native-filesystem-controls/contracts/native.file.pick.json",
  "004-native-filesystem-controls/contracts/native.reveal.json",
  "022-mantine-prototype-design-system/contracts/theme.get.json",
  "022-mantine-prototype-design-system/contracts/theme.set.json",
  // Spec 013 — Target Lookup From FITS OBJECT
  "013-target-lookup-from-fits-object/contracts/target.lookup.json",
  "013-target-lookup-from-fits-object/contracts/target.resolve.json",
  // Spec 006's inventory.list and inventory.session.review contracts are
  // deliberately absent: `../schemas/` holds the maintained copies of both, and
  // those are the ones `tests/contract/contract_jsonschema_roundtrip.rs`
  // validates against.
  // Spec 012 — Processing Artifact Observation
  "012-processing-artifact-observation/contracts/artifact.list.json",
  "012-processing-artifact-observation/contracts/artifact.classify.json",
  "012-processing-artifact-observation/contracts/workflow.run_completed.json",
  // Spec 019 — Bottom Log Viewer
  "019-bottom-log-viewer/contracts/log.stream.json",
  "019-bottom-log-viewer/contracts/log.export.json",
];

// A missing allowlist entry means the allowlist is stale, not that this run
// should emit fewer declaration files. The old version checked only that the
// parent directory existed and silently dropped the entry, so a renamed spec
// folder produced a smaller `src/generated` that the drift gate then accepted.
function findSpecContracts() {
  const paths = SPEC_CONTRACT_ALLOWLIST.map((rel) => join(specsDir, rel));
  const missing = paths.filter((path) => !existsSync(path));
  if (missing.length > 0) {
    console.error(
      `${missing.length} allowlisted spec contract(s) do not exist. Update ` +
        "SPEC_CONTRACT_ALLOWLIST in this script:\n" +
        missing.map((m) => `  ${m}`).join("\n"),
    );
    process.exit(1);
  }
  return paths;
}

const json2ts = resolveJson2Ts();
const schemas = [...findSchemas(schemasDir), ...findSpecContracts()];

if (schemas.length === 0) {
  mkdirSync(generatedDir, { recursive: true });
  writeFileSync(join(generatedDir, "contracts.d.ts"), "export {};\n", "utf8");
  console.log("No contract schemas found yet; wrote placeholder declarations.");
  process.exit(0);
}

// Generate into a staging directory and swap it in only once every schema has
// succeeded. The previous script deleted `src/generated` up front, so a failed
// run left the tree empty — worse than before it started, and it destroyed the
// only copy of types that could no longer be regenerated.
const stagingDir = `${generatedDir}.staging`;
rmSync(stagingDir, { recursive: true, force: true });
mkdirSync(stagingDir, { recursive: true });

// Two schemas that reduce to the same stem write the same `.d.ts`, and the
// second silently wins. That is how the paginated `inventory.list` schema came
// to be overwritten by a spec copy that had never gained the pagination fields.
const stems = new Map();
for (const schema of schemas) {
  const stem = basename(schema, ".schema.json").replace(/\.json$/, "");
  const existing = stems.get(stem);
  if (existing) {
    rmSync(stagingDir, { recursive: true, force: true });
    console.error(
      `Two schemas both generate ${stem}.d.ts, so one would overwrite the ` +
        `other:\n  ${existing}\n  ${schema}\nDrop one, or rename its file.`,
    );
    process.exit(1);
  }
  stems.set(stem, schema);
}

const failures = [];
for (const [stem, schema] of stems) {
  const output = join(stagingDir, `${stem}.d.ts`);
  const result = spawnSync(json2ts, ["-i", schema, "-o", output, "--unreachableDefinitions"], {
    stdio: "inherit",
  });

  if (result.error || result.status !== 0) {
    failures.push(`${schema}${result.error ? ` (${result.error.message})` : ""}`);
  }
}

if (failures.length > 0) {
  rmSync(stagingDir, { recursive: true, force: true });
  console.error(
    `\njson2ts failed on ${failures.length} of ${schemas.length} schema(s); ` +
      `${generatedDir} left unchanged:\n` +
      failures.map((f) => `  ${f}`).join("\n"),
  );
  process.exit(1);
}

rmSync(generatedDir, { recursive: true, force: true });
renameSync(stagingDir, generatedDir);
console.log(`Generated ${schemas.length} contract declaration file(s).`);
