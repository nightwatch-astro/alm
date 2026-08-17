# Research report: Serena project-sharing patterns

Date: 2026-07-22
Tracking: `astro-plan-txk6`
Design: [Serena project sharing](../serena-project-sharing.md)

## Executive summary

The duplicate-Serena problem is common. Serena's issue tracker contains reports
of duplicate language servers, concurrent writes to `.serena`, orphaned stdio
processes, and tens of gigabytes of aggregate memory use.

Serena already provides the shared-server primitive for clients operating on the
same physical checkout. For Git worktrees, however, Serena's documentation and
maintainer guidance recommend a separate server started from each worktree with
`--project-from-cwd`. The nearest `.git` pointer then wins over an ancestor
`.serena/project.yml`, keeping symbolic reads and edits inside that worktree.

Community reports agree on the isolation boundary: version Serena configuration
and memories so they appear in every worktree, but do not share or copy the
language-server cache. Reuse is safe only among clients targeting the exact same
worktree. A bounded worktree agent should not start Serena unless its task needs
semantic navigation; when it does, the Serena process should be rooted to and
owned by that worktree session.

## Serena's supported model

The official model has three constraints:

- Serena is stateful and exposes one active project per server.
- Multiple clients may share a server when they work on that same project.
- Different projects require different Serena instances.

Streamable HTTP gives the operator control over process lifetime. A client
disconnect does not own or terminate the server. Explicit `--project` startup
also removes project switching from the tool surface in single-project contexts.

`--project-from-cwd` resolves the nearest `.serena/project.yml` or `.git`
boundary. That resolution rule is the right basis for launcher identity because
it distinguishes nested projects and Git worktrees.

## Git worktree guidance

Serena's dedicated worktree documentation recommends two concrete practices:

1. Commit `.serena/project.yml` and any intentionally shared memories so every
   worktree receives them through Git.
2. Start the agent inside the worktree and launch Serena with
   `--project-from-cwd` so the worktree's `.git` pointer selects the exact root.

Maintainer responses in issues 1496, 805, and 1455 add the operational boundary:

- one Serena process per concurrently active worktree;
- no cache copying between worktrees because cache entries contain absolute
  paths and each language server must index its own tree;
- `activate_project` may retarget a server for sequential use, but is unsafe for
  concurrent worktrees because the active project is server-global;
- reusing a parent checkout's server from a worktree is a client integration bug,
  not a supported Serena optimization.

The observed failure mode is destructive rather than merely stale navigation.
A Claude Code user reported that Serena operations from four worktree agents all
landed in the main checkout. A local two-client experiment reproduced both parts:
the worktree client read the parent's divergent symbol and its symbolic edit
modified the parent file. Activating the worktree in the second session also
changed the first session's active project.

| Asset | Sharing rule |
|---|---|
| `.serena/project.yml` | Version and share through Git |
| Deliberately shared Serena memories | Version and share through Git |
| `.serena/cache` and language-server index | Rebuild per worktree; never copy or share |
| Serena MCP process and active-project state | One per concurrently active worktree |
| Warm process reuse | Only for clients on the exact same canonical worktree root |

Public automation follows this shape. A small bootstrap script in
`trading-advisor-3000` gives each worktree a unique local project identity while
excluding its cache. Workspace Harbor uses an exact-root ownership model: one
service may be joined by multiple clients at the same root, while different
worktrees receive separate services. These are useful implementation examples,
not authorities; both are small projects, while the Serena documentation and
maintainer responses provide the normative guidance.

## Patterns in public implementations

| Pattern | Public example | Strength | Limitation |
|---|---|---|---|
| Fixed project and port | Penpot `scripts/start-serena` | Few moving parts; fits one repository and one development environment | Manual startup and port ownership; their script binds to all interfaces |
| User-level daemon | Codsworth `serena-daemon.sh` | Start, stop, status, restart, launchd, and systemd user-service support | One fixed endpoint; more lifecycle machinery than project-scoped reuse needs |
| Project lock plus stdio bridge | einja `ensure-serena.sh` and `serena-mcp-bridge.sh` | Same-project reuse without changing client transport support | Stores runtime state in the repository and collapses worktrees onto one checkout |
| Container workspace server | Claude Code Team `serena-start.sh` | Keeps Serena beside mounted code and exposes HTTP to other containers | One server can switch among projects, which conflicts with concurrent multi-project isolation |
| Template-based MCP aggregation | Suggested in Serena issue 1278 | Could derive launch arguments from a client's project path | Depends on the aggregator proving cross-client process reuse and complete tool pagination |

### Fixed project scripts

Penpot starts Serena on a repository-specific port with an explicit project and
integrates the command into its development environment. This fits a single
known checkout. It does not solve automatic discovery across arbitrary projects,
but it demonstrates that keeping Serena outside each MCP client's lifetime is a
normal deployment choice.

### User-level daemons

Codsworth provides a 397-line shell daemon with PID files, port inspection,
launchd, and systemd user-service installation. The script detects the process by
port as well as PID because the process started through `uvx` is not necessarily
the final server process. This is evidence that PID-only validation is
insufficient.

A machine-wide service manager solves boot and restart policy. It does not solve
the mapping from an arbitrary client workspace to the correct per-project
instance. Adding systemd or launchd before that mapping would move the unresolved
problem into service configuration.

### Project-aware launchers

The einja scripts implement the same control flow as the proposed launcher:

1. Resolve a project identity.
2. Acquire a per-project startup lock.
3. Reuse recorded state only when its process still matches the project.
4. Select a port and start Serena in Streamable HTTP mode.
5. Publish the endpoint after it begins listening.
6. Run `mcp-remote` so an stdio-configured client can use that endpoint.

This is the strongest proof that the design does not require an MCP aggregator or
an operating-system service. It also exposes hardening opportunities:

- keep state under the XDG cache directory instead of a tracked checkout;
- treat each worktree as its own Serena project;
- validate PID start identity and exact launch arguments;
- perform an MCP initialize handshake instead of accepting an open TCP port;
- bind only to loopback;
- use an installed Serena version instead of a moving Git branch;
- disable daemon-side browser launch and dashboard behavior;
- quarantine invalid state instead of deleting or killing an unverified process.

### Containers and remote hosts

Serena maintainers support remote or container execution when the repository is
mounted on the same host as Serena. A remote MCP endpoint does not remove that
locality requirement. The language servers and Serena file tools must see the
authoritative working tree.

A container that holds several repositories can expose one Serena endpoint, but
concurrent clients can change its active project. Separate per-project processes
remain the safe topology. A remote deployment is coherent when the shell, Git,
Serena, and language servers all operate on the remote checkout. Mixing local
shell edits with a remotely synchronized Serena copy creates split-brain state.

## Client transport

Codex and Claude support Streamable HTTP directly, so a bridge such as
`mcp-remote` is unnecessary for the long-lived primary-checkout server. An
ephemeral, capability-gated worktree server can use stdio and inherit the agent's
lifetime. This keeps the common worktree path simple and makes parent-death
cleanup possible without inventing lease accounting for a shared daemon.

## Failure evidence

Serena issue 1235 reports duplicate `tsserver` processes and concurrent writes
when several MCP clients open the same directory. Maintainer guidance points to
one persistent HTTP instance for that project.

Issue 1683 reports 167 orphaned Serena-related processes consuming about 46 GB
of resident memory after short-lived clients repeatedly started stdio servers.
The triggering startup scan was fixed, but the incident demonstrates the cost of
coupling a full Serena process to every client session.

Issue 1718 reports stale language-server state after external file edits in a
long-lived server. Serena fixed the reported notification path. A pinned Serena
version and a freshness test remain part of the acceptance suite because sharing
increases the lifetime over which stale-state defects can appear.

OpenAI Codex issue 12491 reports a complementary lifecycle failure: per-worktree
tasks left MCP subprocesses behind, eventually producing more than a thousand
zombie Codex processes and about 37 GB of resident memory use. Per-worktree stdio
is therefore acceptable only with strict child-process ownership, process-group
termination, worktree teardown cleanup, and preferably a parent-heartbeat or
parent-death check.

Serena issue 944 reports a single HTTP Serena process reaching roughly 30 GB,
with the reporter suspecting Serena or its language-server integration rather
than duplicate instances. Serena's configuration exposes tool timeouts and
language-server-specific arguments, but no general memory ceiling. Process-tree
containment must therefore live outside Serena. On Linux and WSL, cgroup v2 is
the smallest reliable primitive: `MemoryHigh`, `MemoryMax`, `CPUQuota`, and
`TasksMax` apply to Serena and all language-server descendants as one unit.

## Recommendation

Use two deliberately different modes:

- For a long-lived primary checkout, reuse one direct Streamable HTTP Serena
  process among compatible clients on that exact canonical root.
- For a worktree lane, do not attach to the parent's process. Leave Serena off by
  default and start a lane-owned stdio server with `--project-from-cwd` only when
  the task requests a semantic-code-navigation capability.

The primary launcher still needs an XDG state directory, startup lock, exact-root
validation, MCP initialize readiness check, and explicit status, stop, and prune
operations. The worktree path instead needs reliable teardown and proof that the
parent checkout remains untouched. This accepts a cold start only for the smaller
set of worktree tasks that benefit from semantic tooling.

Do not put 1MCP back on the critical path. Its template model describes the
desired routing, but it must first prove process reuse across fresh clients and
complete tool pagination in Codex. The launcher-and-bridge design has fewer
shared failure modes and direct public precedent.

## Acceptance additions from the research

- Two worktrees of one repository receive different Serena PIDs and endpoints.
- An ordinary bounded worktree lane launches no Serena process.
- A capability-gated worktree lane resolves its own root with
  `--project-from-cwd`, and symbolic edits do not change the parent checkout.
- Terminating a worktree lane terminates its Serena process and language-server
  descendants.
- Serena and all language-server descendants remain in one cgroup with verified
  memory, CPU, swap, and task limits.
- An external file edit is visible through symbolic tools without restarting the
  shared server.
- The launcher rejects a live PID whose command line names another project.
- Readiness requires a successful MCP initialize exchange, not only a listening
  socket.
- Both Codex and Claude reconnect directly to the same warmed primary-checkout
  endpoint.
- A pinned Serena upgrade invalidates or migrates incompatible runtime state.

## Sources

- [Serena workflow: multiple agents](https://oraios.github.io/serena/02-usage/040_workflow.html)
- [Serena running modes and project resolution](https://oraios.github.io/serena/02-usage/020_running.html)
- [Serena client configuration](https://oraios.github.io/serena/02-usage/030_clients.html)
- [Serena additional usage: Git worktrees](https://oraios.github.io/serena/02-usage/999_additional-usage.html)
- [Serena issue 1496: worktree teammates reuse the primary checkout](https://github.com/oraios/serena/issues/1496)
- [Serena issue 805: using Serena with Git worktrees](https://github.com/oraios/serena/issues/805)
- [Serena issue 1455: copied cache retains the original worktree path](https://github.com/oraios/serena/issues/1455)
- [Serena issue 1235: duplicate instances and concurrent writes](https://github.com/oraios/serena/issues/1235)
- [Serena issue 1116: shared instance guidance](https://github.com/oraios/serena/issues/1116)
- [Serena issue 1278: project-aware HTTP orchestration request](https://github.com/oraios/serena/issues/1278)
- [Serena issue 1683: orphaned process resource incident](https://github.com/oraios/serena/issues/1683)
- [Serena issue 1718: long-lived language-server freshness](https://github.com/oraios/serena/issues/1718)
- [Serena issue 944: one instance consuming about 30 GB](https://github.com/oraios/serena/issues/944)
- [Serena issue 358: remote Docker locality](https://github.com/oraios/serena/issues/358)
- [Serena issue 648: WSL HTTP-server workaround](https://github.com/oraios/serena/issues/648)
- [Penpot Serena startup script](https://github.com/penpot/penpot/blob/396d799c715287ca9ebe2f3f75d46de8eb505b04/scripts/start-serena)
- [Codsworth Serena daemon](https://github.com/AlphaBravoCompany/codsworth-marketplace/blob/3ff25a512c36499cd92543bd3d9b1297570f22c3/plugins/foundry/scripts/serena-daemon.sh)
- [einja Serena launcher](https://github.com/einja-inc/einja-management-template/blob/main/scripts/ensure-serena.sh)
- [einja Serena stdio bridge](https://github.com/einja-inc/einja-management-template/blob/main/scripts/serena-mcp-bridge.sh)
- [Claude Code Team Serena container launcher](https://github.com/nscott/claude-code-team/blob/main/serena-mcp/serena-start.sh)
- [Community report: Serena edits from worktrees landed in main](https://www.reddit.com/r/ClaudeAI/comments/1n4f4iy/watch_out_when_you_are_using_serena_mcp_with/)
- [OpenAI Codex issue 12491: worktree MCP subprocess leak](https://github.com/openai/codex/issues/12491)
- [systemd resource-control settings](https://man7.org/linux/man-pages/man5/systemd.resource-control.5.html)
- [Trading Advisor worktree bootstrap](https://github.com/deusexrenovatio-arch/trading-advisor-3000/blob/e211838b0a07d1d719cbaa07aa904706bd25d0b5/scripts/serena_worktree_bootstrap.py)
- [Workspace Harbor exact-root orchestration](https://github.com/douglasmonsky/serena-workspace-orchestrator)
