#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Resource rules for heavy computation.
//!
//! One place for what every expensive job must obey:
//!
//! - **Memory budget.** A job estimates its bytes and takes a weighted permit
//!   before it allocates; a single job larger than the budget is refused up
//!   front with the reason, instead of exhausting RAM halfway.
//! - **Caches.** Spectra, responses and stage results live in byte-capped LRU
//!   caches keyed by input fingerprints; a changed measurement evicts its keys.
//! - **Threads.** A rayon pool sized by cores and budget. Parallel work is
//!   `map` only; floating-point accumulation runs in its original order, so
//!   results do not depend on the thread count.
//! - **Cancellation and progress.** A cooperative cancel token checked at
//!   stage, channel, input and iteration boundaries, and a progress sink that
//!   reports stage, done/total and an ETA from the observed rate.
//!
//! The available-memory probe is a seam (`MemoryProbe`): the app injects the
//! OS adapter from `secs-sys`; tests inject a fixed value.
