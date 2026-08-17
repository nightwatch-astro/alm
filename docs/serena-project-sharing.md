# Serena project sharing

Status: Proposed
Tracking: `astro-plan-txk6`
Implementation owner: `mcp-serena` package
Research: [Serena project-sharing patterns](research/serena-project-sharing-ecosystem.md)

## Objective

Reuse one persistent Serena MCP server for compatible client sessions targeting
the same physical checkout on the same code host. Treat each concurrent Git
worktree as a separate Serena project.

The Serena process, repository, dependencies, and language servers must share a
host and filesystem. MCP clients may run elsewhere and connect through a tunnel.

## Runtime topology

```text
Codex session ──┐
                ├── direct HTTP ── primary-checkout Serena ── repository + LSPs
Claude session ─┘

worktree agent ── optional owned stdio Serena ── exact worktree + LSPs
```

Compatible clients connect directly to the long-lived primary-checkout HTTP
server. A bounded worktree agent does not receive Serena by default. If its task
requires semantic code navigation, it starts a separate stdio Serena with
`--project-from-cwd`; that process belongs to the agent and exits with it.

## Instance identity

The launcher derives an instance key from:

- code-host identity;
- canonical Serena project root, resolved from the nearest `.serena/project.yml`
  or `.git` boundary and normalized for the host;
- Serena language backend;
- one shared CLI context and tool configuration;
- compatibility version for the launcher state contract.

Codex and Claude must use the same shared Serena context. A client-specific
context would make the instances incompatible and require separate processes.

The key is a SHA-256 digest. Runtime state lives outside the repository under:

```text
${XDG_CACHE_HOME:-~/.cache}/serena-mcp/instances/<digest>/
```

`state.json` records the canonical root, PID, endpoint, Serena version, context,
backend, process start identity, and launcher contract version. It contains no
credentials.

Each Git worktree is a distinct project root. The launcher must not collapse a
worktree onto its main checkout because its files and language-server state can
diverge.

## Worktree policy

- Version `.serena/project.yml` and intentionally shared memories so Git places
  them in every worktree.
- Never copy or share `.serena/cache`; language-server indexes are path-bound and
  must be built per worktree.
- Reuse a Serena process only among clients whose canonical physical worktree
  root is identical.
- Do not route a worktree client to the parent's server, even when both worktrees
  initially point to the same commit. Either tree may diverge after startup, and
  symbolic edits target the server's active root.
- Treat `activate_project` as a sequential administrative operation, not a
  concurrent routing mechanism. The active project is server-global.
- Gate worktree Serena behind a `semantic-code-navigation` capability. Most
  bounded implementation lanes should use native file and search tools instead.

## Start-or-reuse protocol

1. Resolve the canonical repository root. Reject a directory that is not a
   repository unless the caller supplies an explicit Serena project path.
2. Derive the instance key and acquire its cross-process lock.
3. Read `state.json` and validate every reuse condition:
   - the PID is alive and its start identity still matches;
   - its command is a Serena HTTP server;
   - an MCP initialize request succeeds;
   - Serena reports the expected active project root;
   - context, backend, Serena version, and contract version are compatible.
4. Reuse the endpoint only when all conditions pass.
5. Quarantine invalid state, select an unused loopback port, start Serena with an
   explicit project root, and wait for a successful MCP handshake.
6. Write state atomically and release the lock.
7. Return the Streamable HTTP endpoint for direct client connection.

The lock covers validation, process creation, readiness, and state publication.
Concurrent first clients therefore converge on one process instead of starting
duplicate language servers.

## Lifecycle

The initial implementation keeps Serena alive until explicit shutdown, host
restart, or failed health validation. It does not infer liveness from one client
disconnecting.

Commands expose lifecycle operations without requiring an operating-system
service manager:

```text
serena-shared status [project]
serena-shared stop [project]
serena-shared prune
```

`prune` removes dead state and may stop a healthy instance only when an explicit
idle-expiration policy is configured. Automatic lease accounting is deferred
until client-crash recovery can be proven reliable.

## Worktree resource containment

Run every capability-gated worktree Serena in a dedicated cgroup that also
contains its language-server descendants. Serena does not expose a general
memory ceiling, and limiting only the Python parent would miss the language
server that often consumes most of the resources.

The initial Linux/WSL profile is:

```text
MemoryHigh=1G
MemoryMax=1536M
MemorySwapMax=256M
CPUQuota=75%
TasksMax=64
CPUWeight=10
IOWeight=10
```

`MemoryHigh` is the normal pressure boundary; `MemoryMax` is the last-resort
backstop that may terminate the Serena lane. `CPUQuota=75%` permits three
quarters of one core in aggregate. The weights keep Serena and its indexer behind
interactive work during contention. A lane whose language server cannot operate
within this profile may request a larger profile: `2G` high, `3G` maximum, one
and a half cores, and 96 tasks.

All worktree scopes belong to `serena-worktrees.slice`, which limits their
combined footprint:

```text
MemoryHigh=6G
MemoryMax=8G
MemorySwapMax=2G
CPUQuota=300%
TasksMax=512
```

The fleet limit matters more than multiplying the per-lane ceiling by the number
of worktrees. Additional lanes remain discoverable but do not receive Serena
unless they request the semantic capability. Under aggregate pressure, idle
Serena lanes are stopped before active lanes are allowed to compete at the hard
boundary.

On a host with an active systemd user manager, the launcher can create a
transient scope with `systemd-run --user --scope --quiet` and resource-control
properties. Scope mode is synchronous and preserves the MCP stdio relationship.

The wrapper must still forward termination and stop the complete scope when the
owning agent exits. On WSL, installation must first verify that the user manager
and its D-Bus socket are available; enabling user lingering is an explicit
one-time host setup, not something each agent should attempt.

If cgroup delegation is unavailable, fail closed or launch Serena without hard
limits only under an explicit compatibility setting. `nice`, `ionice`,
`taskset`, and `prlimit` are useful supplementary controls, but they do not
provide an aggregate memory and process limit for the whole Serena/LSP tree.
Running Serena in a rootless Podman container can provide equivalent cgroup
limits, but adds image, bind-mount, dependency, and path-identity complexity and
is not the default.

## Remote operation

A remote Serena deployment is valid when the remote checkout is the authoritative
working tree. The remote host owns the repository, dependencies, language servers,
Serena cache, and process.

The server binds to loopback. Remote clients reach it through SSH forwarding,
Tailscale, or an authenticated reverse proxy. An unauthenticated editing-capable
MCP endpoint must not listen on a public interface.

A remote Serena process must not analyze a periodically synchronized copy of a
local working tree. Network filesystem mounts are outside the supported design
because file watching, path identity, and language-server latency become
unreliable.

## Failure behavior

- A lock timeout returns an actionable startup error; it does not bypass the lock.
- A dead PID, reused PID, failed handshake, or project mismatch invalidates state.
- A port collision selects another port while the instance lock remains held.
- A Serena upgrade creates a new compatible generation or requires explicit
  shutdown; the launcher never kills an unverified process.
- An unavailable shared instance may fall back to an isolated stdio Serena only
  when the client configuration explicitly permits that fallback.
- An isolated worktree Serena is terminated with its owning agent process group;
  cleanup also handles abrupt parent death and worktree removal.

## Acceptance tests

- Two simultaneous clients for one repository receive the same Serena PID and
  endpoint.
- Codex and Claude can call symbolic tools through that shared process.
- Disconnecting either client leaves the other client operational.
- Two repositories receive distinct Serena PIDs and endpoints.
- A symlinked path and its canonical path resolve to one instance.
- A stale state file and a reused PID cannot attach a client to the wrong process.
- A project-root mismatch is rejected before any tool call is forwarded.
- A port collision recovers without spawning duplicate Serena processes.
- Restarting a client reuses a warm language server.
- A bounded worktree lane without the semantic capability launches no Serena.
- A semantic worktree lane starts Serena at its own root, cannot modify the
  parent through symbolic tools, and terminates all Serena/LSP descendants when
  the lane exits.
- A semantic worktree lane and all descendants stay inside one cgroup; sustained
  memory pressure is throttled at the soft boundary and cannot exceed the hard
  boundary.
- All worktree cgroups remain inside one fleet slice whose aggregate limits are
  enforced independently of the number of active worktrees.
- Exhausting the worktree profile fails only that lane and reports resource
  exhaustion rather than silently falling back to an unbounded process.
- Two concurrent worktrees never share an active-project state, cache, or
  language-server process.
- An SSH-forwarded client can use a Serena process located beside the authoritative
  remote checkout.

## Delivery slices

1. Add a Linux/WSL start-or-reuse launcher and state contract to `mcp-serena` for
   direct Streamable HTTP use on a long-lived primary checkout.
2. Add a `semantic-code-navigation` capability that gives a worktree agent an
   owned, cgroup-contained stdio Serena launched with `--project-from-cwd`.
3. Add concurrency, stale-state, exact-root isolation, parent-untouched, and
   process-teardown integration tests, including soft and hard resource limits.
4. Replace direct per-session Serena startup in generated primary-session Codex
   and Claude MCP configuration with the discovered HTTP endpoint.
5. Add explicit status, stop, and prune commands for persistent instances.
6. Evaluate a native Windows launcher only if Serena runs against native Windows
   working trees rather than WSL-hosted repositories.

## Rejected alternatives

- One Serena process for unrelated projects: active project and language-server
  state can cross session boundaries.
- One stdio Serena process per client: safe but repeats language-server cold starts.
- Routing worktree agents to the parent checkout's Serena: initially appears to
  reuse a warm index, but reads and edits continue to target the parent root after
  either tree diverges.
- Concurrent `activate_project` calls on one shared server: the active project is
  process-global, so clients can redirect one another.
- Copying a parent worktree's Serena cache: cache paths are absolute and the
  resulting index remains bound to the original checkout.
- A remote Serena process over a synchronized checkout: edits and indexes can
  diverge from the client's working tree.
- A public HTTP listener: Serena exposes code-reading and code-editing capabilities.
- Mandatory systemd, launchd, or Windows service registration: unnecessary for
  project-scoped discovery and reuse.
