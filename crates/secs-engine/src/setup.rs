//! Normalisation of the design settings before any measurement is read.
//!
//! Upstream: the body of `CorrectionEngine.__init__` (`correction_engine.py:418–489`):
//! manual sub entries, fixed sub filters, bundle layouts and their shared
//! filters, the FIR-length cap of the lift and the latency budget, the RC
//! range, the PEQ band count. Packet P05.
