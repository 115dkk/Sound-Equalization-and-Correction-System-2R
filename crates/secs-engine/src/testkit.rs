//! Engine-only helpers used by the unpublished oracle test kit.

pub use crate::fit::{RecordedFit, replay_fitter};

/// Final support metrics over every measured position, using production guards.
#[derive(Debug)]
pub struct MimoAcousticMetrics {
    pub input: String,
    pub late_energy_db: f64,
    pub worst_band_db: f64,
    /// The existing 0.15 s/dB modal-tail guard, not a fitted T60 slope.
    pub modal_tail_guard: f64,
    pub paths: usize,
}

pub fn mimo_acoustic_metrics(
    design: &crate::Design,
    measurements: &crate::MeasurementSet,
    config: &crate::EngineConfig,
    run: &crate::mimo::MimoRun,
    ctx: &secs_compute::Context,
) -> Result<Vec<MimoAcousticMetrics>, crate::EngineError> {
    use crate::Tuning;
    use crate::mimo::{self, design as md, iir};
    use secs_types::chain::MimoFilter;

    let prepared = mimo::prepared::Prepared::new(design, measurements, config, ctx)?;
    let req = run.key.request();
    let mut metrics = Vec::new();
    for (input, ideal_supports) in &run.ideal.supports {
        let pb = iir::build_problem(
            design,
            &prepared.spectra,
            &prepared.align_delay_ms,
            &run.ideal,
            input,
            ideal_supports,
            &req,
            &Tuning::default(),
            ctx,
        )?;
        let actual_paths: Vec<_> = run
            .paths
            .paths
            .iter()
            .filter(|path| path.input == *input)
            .collect();
        let (ys, band) = if run.fir {
            let raw: Vec<Vec<Vec<f64>>> = design
                .spectra
                .positions
                .iter()
                .map(|&position| {
                    pb.roles
                        .iter()
                        .map(|role| {
                            let h = mimo::support_path(
                                &run.ideal,
                                role,
                                &prepared.spectra,
                                position,
                                &prepared.align_delay_ms,
                            )?;
                            Ok(secs_dsp::lowrate::to_low(
                                &h,
                                crate::grid::design_freqs(),
                                pb.n,
                            )?)
                        })
                        .collect::<Result<_, crate::EngineError>>()
                })
                .collect::<Result<_, _>>()?;
            let filters: Vec<_> = ideal_supports
                .iter()
                .map(|(_, support)| support.clone())
                .collect();
            let ys = pb
                .xs
                .iter()
                .zip(&raw)
                .map(|(x, paths)| {
                    let mut add = vec![0.0; pb.n];
                    for (g, support) in paths.iter().zip(&filters) {
                        for (sum, value) in add.iter_mut().zip(md::convolve(g, support)) {
                            *sum += value;
                        }
                    }
                    x.iter()
                        .zip(add)
                        .map(|(value, addition)| value + addition)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let band = if filters.is_empty() {
                -99.0
            } else {
                md::worst_band_db(&pb.xs, &raw, &filters, req.fsiso)
            };
            (ys, band)
        } else {
            let supports = actual_paths
                .iter()
                .filter_map(|path| {
                    if let MimoFilter::Iir {
                        gain_db,
                        delay_ms,
                        polarity,
                        peq,
                        allpass,
                    } = &path.filter
                    {
                        Some((
                            pb.roles
                                .iter()
                                .position(|role| *role == path.support)
                                .expect("actual support belongs to ideal problem"),
                            iir::IirSupport {
                                gain_db: *gain_db,
                                delay_ms: *delay_ms,
                                polarity: *polarity,
                                peq: peq.clone(),
                                allpass: allpass.clone(),
                            },
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let band = if supports.is_empty() {
                -99.0
            } else {
                iir::worst_band_db(&pb, &supports)
            };
            (iir::applied(&pb, &supports), band)
        };
        let energy = |signals: &[Vec<f64>]| {
            signals
                .iter()
                .zip(&pb.wl)
                .map(|(signal, weights)| {
                    crate::decay::energy(
                        &signal
                            .iter()
                            .zip(weights)
                            .map(|(value, weight)| value * weight)
                            .collect::<Vec<_>>(),
                    )
                })
                .sum::<f64>()
        };
        let late_energy_db = 10.0 * ((energy(&ys) + 1e-30) / (energy(&pb.xs) + 1e-30)).log10();
        metrics.push(MimoAcousticMetrics {
            input: input.clone(),
            late_energy_db,
            worst_band_db: band,
            modal_tail_guard: mimo::worst_t60_change(&pb.xs, &ys, 6)?,
            paths: actual_paths.len(),
        });
    }
    Ok(metrics)
}
