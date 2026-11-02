# P163 (Daybreak): audit #155, group W3: the managed host (A3) and the policy gates

Follow the **common rules** in `docs/packets/P161-daybreak-audit155-deploy.md`.
Never install, start or stop the real service on this machine; every test here
runs the host's logic against fakes.

Worktree `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\audit-155-w3`,
branch `claude/audit-155-w3`. Files: `apps/secs2r-host/**`, `crates/secs-sys/src/host.rs`
(only where an item needs it), `crates/secs-policy/tests/gates.rs`. Read ADR 0002,
0011, 0018, 0020, `docs/UNSAFE_BOUNDARIES.md`, and `apps/secs2r-host/src/{main.rs,rules.rs}`
end to end before changing anything.

## Design (fixed): one module decides, the platform does (A3)

Today `rules.rs` keeps pure rules (`Core`, `ProcessRules`) and `main.rs`
(`handle_request`, the session event branch, restarts) orders owner changes,
state saving, the watcher, the EqAPO repair helper and child stop/start by
hand. Move that ordering behind one module:

- New `apps/secs2r-host/src/transitions.rs` (not Windows-only): given the
  managed state, the running child's session (if any) and one input (a
  `SetManaged` update with the requester's session and SID, `StopManaging`,
  a session event with its session id, a config-change notice, a restart tick),
  it returns the new managed state and an ordered list of effects (save state,
  replace watcher, repair EqAPO for session s, stop child, launch for session s,
  log, set a target's error status, schedule a retry).
- A trait for the platform side (save state, watcher, repair helper, stop,
  launch, log) with the Windows implementation in `main.rs` and a fake in tests.
  One driver function runs the effects in order through the trait and turns a
  failed EqAPO repair into that target's error status plus a bounded retry
  (3 attempts, backoff from the existing restart delays) instead of ending the
  service loop.
- `handle_request` and the event loop call the module and the driver; they no
  longer order effects themselves. Keep `Core` and `ProcessRules` pure (the
  architecture review's Fine as is: do not merge them).

## Items (in this order)

| Item | Finding | Cat | Seam and what is fixed |
|---|---|---|---|
| A-2 | TD-111 | A | Transitions module + fake platform: user A registers CamillaDSP, then user B (another SID) sends an EqAPO-only `SetManaged`. The new state has no CamillaDSP target and the fake platform never launches A's executable. Same SID keeps its targets. This item introduces the module, trait and driver. |
| A-17 | TD-113 | A | Same seam: the fake repair helper fails (missing file, timeout). The service keeps running, the CamillaDSP child is not stopped, the EqAPO target reports the error, and the retry happens at most 3 times. |
| A-18 | TD-114 | A | Same seam: a logoff event for a session other than the running child's leaves the child and the restart schedule alone; a logoff of the child's own session stops it as today. |
| A-32 | TD-135 | A | The update-source check in `rules.rs`: any existing component of the source path that is a symlink or junction is refused (check `symlink_metadata` of each ancestor). `#[cfg(windows)]` test with a real directory symlink in a temp folder; if creating it fails for lack of privilege, print why and return. Write at the site that the check-then-open race remains and why it is acceptable (cite `audit #155 TD-135`). |
| A-33 | TD-138 | A | `Core` logs: the in-memory log keeps at most 1,000 entries (oldest dropped), and a `Log { lines }` response is trimmed from the oldest side until it serializes under the pipe limit with a margin. Test both bounds at `Core`. |
| C-3 | TD-136 | C | `secs-policy` gates take first-party crates from the workspace members in the root `Cargo.toml` (so `bindings/python` is checked), and a check fails if a member is not covered. Gate: `cargo test -p secs-policy`. |
| C-16 | Q10 | C | At the install-folder check in `crates/secs-sys/src/host.rs` (near lines 570 to 654) write why the check relies on the Program Files ACL that the installer inherits and does not walk DACLs (cite `audit #155 Q10`). Comment only. |

Crates to gate: `secs2r-host` (and its Windows clippy), `secs-sys` if touched
(Windows clippy), `secs-policy`.
