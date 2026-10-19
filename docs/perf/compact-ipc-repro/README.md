# Compact-result IPC audit reproduction

This helper uses an unmodified copy of the repository's actual `secs-types` crate. It never edits the supplied repository. Requirements: the repository's pinned Rust toolchain, Python 3.10+, and optionally Node 24+.

From the repository root, using an output directory outside the checkout:

```sh
python docs/perf/compact-ipc-repro/measure.py --repo . --revision d8e0bd6c5f470eb489f9f38d5e2d6cd59619b1be --phase baseline --out /tmp/secs-ipc-before --node
python docs/perf/compact-ipc-repro/measure.py --repo . --phase after --out /tmp/secs-ipc-after --node
```

On Windows replace `/tmp/...` with a scratch directory, and use `py -3` if needed. The optional `--offline` uses already-cached Rust packages. `--target-dir` can reuse an external target directory. Large JSON files and generated binaries stay in the output directory; do not commit them. The helper snapshots a Git revision or current working-tree types, narrows only the copied workspace membership, builds the standalone harness, and records Rust/type-source provenance.

## Fixed synthetic input

- 11 input/main roles: L, R, C, Ls, Rs, Lb, Rb, Tfl, Tfr, Tbl, Tbr.
- These 11 roles plus SW1/SW2 are support candidates; each input excludes itself: **132 FIR paths × 8,000 coefficients = 1,056,000 f64 values**.
- Calibration chain: the same 11 mains × 2,048 input-correction coefficients = 22,528 f64 values, plus fixed PEQ/sub metadata.
- Coefficients are dense, deterministic, decaying signed noise, not an acoustically validated correction: SplitMix64-style unsigned mixing with seed `0x5EC520261020`; signed upper 32 bits divided by `2^32` and `(256 + tap)`. MIMO stream = path index; calibration stream = 100 + main index. The exact wrapping constants and operations are in both Rust helpers.
- Both phases construct the full coefficient structures. The after harness keeps the full structures alive while serializing compact summaries and terminal markers. This experiment does **not** establish a reduction in total Rust application/cache memory.
- Metadata (roles, metrics, notices, compute provenance) is identical across phases except fields intentionally replaced by role/input counts. Do not mix these byte counts with a service test using different coefficient values or notices.

## Measurements and limits

`rust-metrics.json` reports exact UTF-8 JSON byte counts. Serialization uses one warm-up and seven samples of `serde_json::to_vec`; coefficient construction and disk writes are outside the timer. Durations are microbenchmarks, not IPC round-trip or UI latency.

The optional Node parse probe uses `JSON.parse`, one warm-up, 11 samples, and explicit GC outside timers. One parsed object's retained V8 heap excludes its already-resident source JSON string. Very small heap deltas/times are noisy.

The history probe is a **synthetic model** of baseline `state/session.ts`'s `host.jobs` retaining each `Finished.result`, plus a separately decoded current `ResultsSummary`. It repeats 40 completed MIMO jobs and keeps two active jobs. Baseline retains all 40 completed results. After retains 32 terminal metadata records plus both active jobs and one compact current summary. The model does not execute React or the actual reducer, and excludes IPC bridge copies, WebView rendering, native caches, and process RSS. Source JSON strings are already resident and excluded from retained heap deltas. Host background work was not excluded from parse timing runs.

`compact-ipc-audit.json` and `payload-comparison.csv` are the committed compact evidence. Raw multi-megabyte payloads remain external. Record new runtime/heap observations when reproducing on a different Node version or machine; only exact payload bytes are expected to be stable for this fixed fixture and type version.
