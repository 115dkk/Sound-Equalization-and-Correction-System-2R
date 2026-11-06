//! Command-line argument model.

use clap::{Parser, Subcommand, ValueEnum};
use secs_types::ipc::*;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "secs2r-cli", version, about = "SECS 2R without a screen")]
pub(crate) struct Cli {
    /// Omit job-event lines while retaining every reply.
    #[arg(long, global = true)]
    pub(crate) quiet: bool,
    /// Use a fixed clock and UUID counter beginning at one for reproducible output.
    #[arg(long, global = true)]
    pub(crate) deterministic_uuids: bool,
    /// Choose MIMO computation for this process without changing desktop preferences.
    #[arg(long, global = true, value_enum)]
    pub(crate) compute: Option<ComputeChoice>,
    #[command(subcommand)]
    pub(crate) action: Action,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Action {
    /// Print compute preference, selected device, and availability.
    ComputeInfo,
    /// Print the version, upstream base, and platform.
    Bootstrap,
    /// Check platform and deterministic parallel-map operation.
    Check,
    /// Create or inspect a project.
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Load embedded or deterministic synthetic demo measurements.
    Sample {
        project: PathBuf,
        #[arg(long)]
        positions: Option<u32>,
    },
    /// Measure one project position.
    Measure {
        project: PathBuf,
        #[arg(long)]
        position: i64,
        #[arg(long)]
        method: Option<MeasureMethod>,
    },
    /// Import suggested assignments from a REW mdat or a Dirac Live project.
    ImportMdat {
        project: PathBuf,
        file: PathBuf,
        #[arg(long, default_value_t = 1)]
        position: i64,
    },
    /// Compute a calibration.
    Calibrate {
        project: PathBuf,
        /// Compute MIMO after the correction in the same job.
        #[arg(long)]
        mimo: bool,
    },
    /// Recommend crossover frequencies.
    RecommendXo {
        project: PathBuf,
        /// Use reduced CPU resources for the recommendation.
        #[arg(long)]
        background: bool,
    },
    /// Calibrate first, then compute MIMO support paths.
    Mimo { project: PathBuf },
    /// Write one export bundle.
    Export {
        project: PathBuf,
        #[arg(long)]
        kind: ExportChoice,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, value_enum)]
        mdat_scope: Option<MdatScopeChoice>,
        /// Additional comma-separated convolution output rates.
        #[arg(long, value_delimiter = ',')]
        extra_rates: Vec<u32>,
        /// Stable design name; a matching kind and name is replaced.
        #[arg(long)]
        design_name: Option<String>,
        /// CamillaDSP capture backend.
        #[arg(long, value_enum, default_value_t = CamillaBackendChoice::File)]
        camilla_capture: CamillaBackendChoice,
        /// CamillaDSP playback backend.
        #[arg(long, value_enum, default_value_t = CamillaBackendChoice::File)]
        camilla_playback: CamillaBackendChoice,
        /// Capture endpoint or ASIO driver name.
        #[arg(long)]
        camilla_capture_device: Option<String>,
        /// Playback endpoint or ASIO driver name.
        #[arg(long)]
        camilla_playback_device: Option<String>,
        /// Wasapi capture sample format.
        #[arg(long, value_enum, default_value_t = CamillaFormatChoice::F32)]
        camilla_capture_format: CamillaFormatChoice,
        /// Wasapi playback sample format.
        #[arg(long, value_enum, default_value_t = CamillaFormatChoice::F32)]
        camilla_playback_format: CamillaFormatChoice,
        #[arg(long)]
        camilla_capture_exclusive: bool,
        #[arg(long)]
        camilla_playback_exclusive: bool,
        #[arg(long)]
        camilla_capture_loopback: bool,
    },
    /// Apply or inspect the current design through an external processor.
    Apply {
        #[command(subcommand)]
        action: ApplyAction,
    },
    /// Inspect or administer the Windows managed-host service.
    Host {
        #[command(subcommand)]
        action: HostAction,
    },
    /// Run creation, filling, calibration, and all exports.
    Run {
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long, default_value = "sample")]
        fill: FillChoice,
        #[arg(long)]
        mimo: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ApplyAction {
    /// Export the current design and reload CamillaDSP.
    Camilla {
        /// Project whose current design is applied.
        project: PathBuf,
        /// Override the CamillaDSP websocket address for this process.
        #[arg(long)]
        address: Option<String>,
    },
    /// Export the current design to Equalizer APO, or remove its include.
    Eqapo {
        /// Project whose current design is applied; omitted when reverting.
        #[arg(required_unless_present = "revert")]
        project: Option<PathBuf>,
        /// Remove the managed include instead of applying.
        #[arg(long, conflicts_with = "project")]
        revert: bool,
    },
    /// Print CamillaDSP and Equalizer APO application state.
    Status,
}

#[derive(Debug, Subcommand)]
pub(crate) enum HostAction {
    /// Print service registration and managed-target status.
    Status,
    /// Register the automatic LocalSystem service through UAC.
    Install,
    /// Stop and remove the service through UAC.
    Uninstall,
    /// Clear every target kept by the service.
    StopManaging,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProjectAction {
    /// Create a project file.
    New {
        path: PathBuf,
        #[arg(long, default_value = "2.0")]
        preset: String,
        #[arg(long, default_value_t = 0)]
        subs: u8,
        #[arg(long)]
        sample: bool,
    },
    /// Open a project and print its information.
    Info { path: PathBuf },
    /// Set the calibration settings an experiment changes, then save the project.
    Set {
        path: PathBuf,
        /// Compute MIMO support paths after calibrating.
        #[arg(long, value_enum)]
        mimo: Option<Switch>,
        /// Low-end protection, automatic: prevent boosts below measured rolloff and protect sub outputs.
        #[arg(long, value_enum)]
        low_protect: Option<Switch>,
        /// Align bass arrival across main channels.
        #[arg(long, value_enum)]
        lf_align: Option<Switch>,
        /// Shorten long room-mode ringing with corrective cuts.
        #[arg(long, value_enum)]
        mode_decay: Option<Switch>,
        /// Unwind the RC FIR's constant phase above the correction band.
        #[arg(long, value_enum)]
        hf_phase: Option<Switch>,
        /// Limit how much one main fills a narrow response dip.
        #[arg(long, value_enum)]
        dip_limit: Option<Switch>,
        /// Low-end protection, manual: one high-pass on every input (ADR 0031), in Hz, or off.
        #[arg(long, value_name = "Hz|off")]
        input_hp: Option<InputHpArg>,
        /// MIMO supports as FIR or as IIR (PEQ and all-pass) filters.
        #[arg(long, value_enum)]
        mimo_supports: Option<SupportKind>,
        /// Start MIMO support action this many milliseconds after onset.
        #[arg(long, value_name = "ms")]
        mimo_early: Option<f64>,
        /// Evaluate MIMO guards at each measured position.
        #[arg(long, value_enum)]
        mimo_seat_guard: Option<Switch>,
        /// PEQ derivative method (experimental analytic results differ from SECS 2).
        #[arg(long, value_enum)]
        peq_derivatives: Option<PeqDerivativeChoice>,
        /// MIMO IIR derivative method (experimental analytic results differ from SECS 2).
        #[arg(long, value_enum)]
        mimo_iir_derivatives: Option<MimoIirDerivativeChoice>,
        /// Extra RC bands as LO-HI pairs separated by commas, or none to clear them.
        #[arg(long)]
        rc_bands: Option<RcBandsArg>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct InputHpArg(pub(crate) Option<f64>);

impl std::str::FromStr for InputHpArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "off" {
            return Ok(Self(None));
        }
        value
            .parse::<f64>()
            .map(|hz| Self(Some(hz)))
            .map_err(|_| "input high-pass must be a frequency in Hz or off".into())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RcBandsArg(pub(crate) Vec<secs_types::calibration::RcBand>);

impl std::str::FromStr for RcBandsArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "none" {
            return Ok(Self(Vec::new()));
        }
        if value.is_empty() {
            return Err("RC bands must be LO-HI pairs or none".into());
        }
        let bands = value
            .split(',')
            .map(|band| {
                let (lo_hz, hi_hz) = band
                    .split_once('-')
                    .ok_or_else(|| format!("invalid RC band {band:?}; expected LO-HI"))?;
                if lo_hz.is_empty() || hi_hz.is_empty() || hi_hz.contains('-') {
                    return Err(format!("invalid RC band {band:?}; expected LO-HI"));
                }
                Ok(secs_types::calibration::RcBand {
                    lo_hz: lo_hz
                        .parse()
                        .map_err(|_| format!("invalid RC lower edge {lo_hz:?}"))?,
                    hi_hz: hi_hz
                        .parse()
                        .map_err(|_| format!("invalid RC upper edge {hi_hz:?}"))?,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self(bands))
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Switch {
    On,
    Off,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum SupportKind {
    Fir,
    Iir,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum PeqDerivativeChoice {
    TwoPoint,
    Analytic,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum MimoIirDerivativeChoice {
    Central,
    Analytic,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum ComputeChoice {
    Auto,
    Cpu,
    Gpu,
}
#[derive(Debug, Clone, Copy, ValueEnum, Default, PartialEq, Eq)]
pub(crate) enum CamillaBackendChoice {
    #[default]
    File,
    Wasapi,
    Asio,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub(crate) enum CamillaFormatChoice {
    S32,
    S24,
    S16,
    #[default]
    F32,
}

impl From<CamillaFormatChoice> for CamillaWasapiFormat {
    fn from(value: CamillaFormatChoice) -> Self {
        match value {
            CamillaFormatChoice::S32 => Self::S32,
            CamillaFormatChoice::S24 => Self::S24,
            CamillaFormatChoice::S16 => Self::S16,
            CamillaFormatChoice::F32 => Self::F32,
        }
    }
}

impl From<ComputeChoice> for secs_types::compute::ComputePreference {
    fn from(value: ComputeChoice) -> Self {
        match value {
            ComputeChoice::Auto => Self::Auto,
            ComputeChoice::Cpu => Self::Cpu,
            ComputeChoice::Gpu => Self::Gpu,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum MeasureMethod {
    Mock,
    Native,
    Rew,
}
impl MeasureMethod {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Native => "native",
            Self::Rew => "rew",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum FillChoice {
    Sample,
    Mock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ExportChoice {
    CamillaDsp,
    StagedMdat,
    BeforeAfterMdat,
    FilterText,
    EqualizerApo,
    Convolution,
}
impl From<ExportChoice> for ExportKind {
    fn from(value: ExportChoice) -> Self {
        match value {
            ExportChoice::CamillaDsp => Self::CamillaDsp,
            ExportChoice::StagedMdat => Self::StagedMdat,
            ExportChoice::BeforeAfterMdat => Self::BeforeAfterMdat,
            ExportChoice::FilterText => Self::FilterText,
            ExportChoice::EqualizerApo => Self::EqualizerApo,
            ExportChoice::Convolution => Self::Convolution,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum MdatScopeChoice {
    All,
    Measurements,
    Results,
}
impl From<MdatScopeChoice> for MdatScope {
    fn from(value: MdatScopeChoice) -> Self {
        match value {
            MdatScopeChoice::All => Self::All,
            MdatScopeChoice::Measurements => Self::Measurements,
            MdatScopeChoice::Results => Self::Results,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn apply_commands_parse() {
        assert!(matches!(
            Cli::try_parse_from([
                "secs2r-cli",
                "apply",
                "camilla",
                "room.secs",
                "--address",
                "localhost:1234"
            ])
            .unwrap()
            .action,
            Action::Apply {
                action: ApplyAction::Camilla {
                    project,
                    address: Some(_)
                }
            } if project == Path::new("room.secs")
        ));
        assert!(matches!(
            Cli::try_parse_from(["secs2r-cli", "apply", "eqapo", "room.secs"])
                .unwrap()
                .action,
            Action::Apply {
                action: ApplyAction::Eqapo {
                    project: Some(project),
                    revert: false
                }
            } if project == Path::new("room.secs")
        ));
        assert!(matches!(
            Cli::try_parse_from(["secs2r-cli", "apply", "eqapo", "--revert"])
                .unwrap()
                .action,
            Action::Apply {
                action: ApplyAction::Eqapo {
                    project: None,
                    revert: true
                }
            }
        ));
        assert!(matches!(
            Cli::try_parse_from(["secs2r-cli", "apply", "status"])
                .unwrap()
                .action,
            Action::Apply {
                action: ApplyAction::Status
            }
        ));
        assert!(Cli::try_parse_from(["secs2r-cli", "apply", "eqapo"]).is_err());
        for (command, expected) in [
            ("status", HostAction::Status),
            ("install", HostAction::Install),
            ("uninstall", HostAction::Uninstall),
            ("stop-managing", HostAction::StopManaging),
        ] {
            assert!(matches!(
                Cli::try_parse_from(["secs2r-cli", "host", command])
                    .unwrap()
                    .action,
                Action::Host { action } if std::mem::discriminant(&action) == std::mem::discriminant(&expected)
            ));
        }
    }

    #[test]
    fn recommend_xo_parses_background_flag_td157() {
        let cli = Cli::try_parse_from(["secs2r-cli", "recommend-xo", "room.secs", "--background"])
            .unwrap();
        assert!(matches!(
            cli.action,
            Action::RecommendXo {
                background: true,
                ..
            }
        ));
        let cli = Cli::try_parse_from(["secs2r-cli", "recommend-xo", "room.secs"]).unwrap();
        assert!(matches!(
            cli.action,
            Action::RecommendXo {
                background: false,
                ..
            }
        ));
    }

    #[test]
    fn calibrate_parses_mimo_flag() {
        let cli =
            Cli::try_parse_from(["secs2r-cli", "calibrate", "project.secs", "--mimo"]).unwrap();
        let Action::Calibrate { project, mimo } = cli.action else {
            panic!("calibrate command must parse")
        };
        assert_eq!(project, PathBuf::from("project.secs"));
        assert!(mimo);
    }

    #[test]
    fn export_parses_comma_separated_extra_rates() {
        let cli = Cli::try_parse_from([
            "secs2r-cli",
            "export",
            "project.secs",
            "--kind",
            "convolution",
            "--out",
            "exports",
            "--extra-rates",
            "44100,96000",
        ])
        .unwrap();
        let Action::Export { extra_rates, .. } = cli.action else {
            panic!("export command must parse")
        };
        assert_eq!(extra_rates, [44_100, 96_000]);
    }
}
