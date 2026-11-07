//! Command execution over the application service.

use crate::args::*;
use crate::runner::Runner;
use secs_api::Secs;
use secs_types::ipc::*;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;

pub(crate) fn run(action: Action, runner: &mut Runner) {
    match action {
        Action::ComputeInfo => {
            runner.call(Request::ComputeStatus);
        }
        Action::Bootstrap => {
            runner.call(Request::Bootstrap);
        }
        Action::Check => check(runner),
        Action::Project { action } => match action {
            ProjectAction::New {
                path,
                preset,
                subs,
                sample,
            } => {
                runner.call(Request::ProjectNew(ProjectNewParams {
                    path: path.to_string_lossy().into_owned(),
                    preset,
                    sub_count: subs,
                    sample,
                    tutorial: false,
                }));
            }
            ProjectAction::Info { path } => {
                runner.open(&path);
                runner.call(Request::ProjectInfo);
            }
            ProjectAction::Set {
                path,
                mimo,
                mimo_supports,
                mimo_early,
                mimo_seat_guard,
                low_protect,
                lf_align,
                mode_decay,
                hf_phase,
                dip_limit,
                input_hp,
                peq_derivatives,
                mimo_iir_derivatives,
                rc_bands,
            } => {
                use secs_types::settings_edit::SettingEdit;
                runner.open(&path);
                let mut edits = Vec::new();
                if let Some(mimo) = mimo {
                    edits.push(SettingEdit::MimoOn {
                        on: matches!(mimo, Switch::On),
                    });
                }
                if let Some(kind) = mimo_supports {
                    edits.push(SettingEdit::MimoFir {
                        on: matches!(kind, SupportKind::Fir),
                    });
                }
                if let Some(ms) = mimo_early {
                    edits.push(SettingEdit::MimoEarly { ms });
                }
                if let Some(value) = mimo_seat_guard {
                    edits.push(SettingEdit::MimoSeatGuard {
                        on: matches!(value, Switch::On),
                    });
                }
                if peq_derivatives.is_some() || mimo_iir_derivatives.is_some() {
                    edits.push(SettingEdit::AnalyticDerivatives {
                        peq: peq_derivatives
                            .map(|choice| matches!(choice, PeqDerivativeChoice::Analytic)),
                        mimo_iir: mimo_iir_derivatives
                            .map(|choice| matches!(choice, MimoIirDerivativeChoice::Analytic)),
                    });
                }
                if let Some(value) = low_protect {
                    edits.push(SettingEdit::LowProtect {
                        on: matches!(value, Switch::On),
                    });
                }
                if let Some(value) = lf_align {
                    edits.push(SettingEdit::LfAlign {
                        on: matches!(value, Switch::On),
                    });
                }
                if let Some(value) = mode_decay {
                    edits.push(SettingEdit::ModeDecay {
                        on: matches!(value, Switch::On),
                    });
                }
                if let Some(value) = hf_phase {
                    edits.push(SettingEdit::HfPhase {
                        on: matches!(value, Switch::On),
                    });
                }
                if let Some(value) = dip_limit {
                    edits.push(SettingEdit::DipLimit {
                        on: matches!(value, Switch::On),
                    });
                }
                if let Some(InputHpArg(hz)) = input_hp {
                    edits.push(SettingEdit::InputHp { hz });
                }
                if let Some(RcBandsArg(bands)) = rc_bands {
                    edits.push(SettingEdit::RcBands { bands });
                }
                let changed = !edits.is_empty();
                for edit in edits {
                    runner.call(Request::CalibrationSettingsEdit(
                        CalibrationSettingsEditParams { edit },
                    ));
                }
                if changed {
                    runner.save();
                }
                runner.call(Request::CalibrationSettingsGet);
            }
        },
        Action::Sample { project, positions } => {
            runner.open(&project);
            runner.call(Request::SampleLoad(SampleLoadParams { positions }));
            runner.save();
        }
        Action::Measure {
            project,
            position,
            method,
        } => {
            runner.open(&project);
            if let Some(method) = method {
                runner.call(Request::MeasureConfigure(MeasureConfigureParams {
                    method: method.name().into(),
                    ..Default::default()
                }));
            }
            runner.call(Request::MeasurePosition(MeasurePositionParams { position }));
            runner.save();
        }
        Action::ImportMdat {
            project,
            file,
            position,
        } => {
            runner.open(&project);
            let path = file.to_string_lossy().into_owned();
            let scan = runner.call(Request::MdatScan(MdatScanParams { path: path.clone() }));
            let assignments = match scan {
                Some(Response::MdatScan(result)) => untagged_rows_use_position(result),
                _ => Vec::new(),
            };
            runner.call(Request::MdatImport(MdatImportParams {
                path,
                assignments,
                position,
            }));
            runner.save();
        }
        Action::Calibrate { project, mimo } => {
            runner.open(&project);
            runner.calibrate(None, mimo);
            runner.call(Request::Results);
        }
        Action::RecommendXo {
            project,
            background,
        } => {
            runner.open(&project);
            runner.call(Request::RecommendXo(Some(
                secs_types::ipc::RecommendXoParams { background },
            )));
        }
        Action::Mimo { project } => {
            runner.open(&project);
            runner.calibrate(None, false);
            runner.call(Request::Mimo(MimoParams::default()));
            runner.call(Request::Results);
        }
        Action::Export {
            project,
            kind,
            out,
            mdat_scope,
            extra_rates,
            design_name,
            camilla_capture,
            camilla_playback,
            camilla_capture_device,
            camilla_playback_device,
            camilla_capture_format,
            camilla_playback_format,
            camilla_capture_exclusive,
            camilla_playback_exclusive,
            camilla_capture_loopback,
        } => {
            runner.open(&project);
            let camilla_devices = (kind == ExportChoice::CamillaDsp).then(|| CamillaDevices {
                capture: match camilla_capture {
                    CamillaBackendChoice::File => CamillaCaptureDevice::File,
                    CamillaBackendChoice::Wasapi => CamillaCaptureDevice::Wasapi {
                        device: camilla_capture_device,
                        format: camilla_capture_format.into(),
                        exclusive: camilla_capture_exclusive,
                        loopback: camilla_capture_loopback,
                    },
                    CamillaBackendChoice::Asio => CamillaCaptureDevice::Asio {
                        device: camilla_capture_device.unwrap_or_default(),
                    },
                },
                playback: match camilla_playback {
                    CamillaBackendChoice::File => CamillaPlaybackDevice::File,
                    CamillaBackendChoice::Wasapi => CamillaPlaybackDevice::Wasapi {
                        device: camilla_playback_device,
                        format: camilla_playback_format.into(),
                        exclusive: camilla_playback_exclusive,
                    },
                    CamillaBackendChoice::Asio => CamillaPlaybackDevice::Asio {
                        device: camilla_playback_device.unwrap_or_default(),
                    },
                },
            });
            runner.export(
                kind.into(),
                &out,
                mdat_scope.map(Into::into),
                design_name,
                camilla_devices,
                extra_rates,
            );
        }
        Action::Apply { action } => match action {
            ApplyAction::Camilla { project, address } => {
                runner.open(&project);
                let mut settings = ApplySettings::default();
                settings.camilla.level = ApplyLevel::Apply;
                if let Some(address) = address {
                    settings.camilla.address = address;
                }
                runner.call(Request::ApplySettingsSet(settings));
                runner.call(Request::ApplyCamilla(ApplyCamillaParams::default()));
            }
            ApplyAction::Eqapo {
                project: None,
                revert: true,
            } => {
                runner.call(Request::RevertEqapo);
            }
            ApplyAction::Eqapo {
                project: Some(project),
                revert: false,
            } => {
                runner.open(&project);
                let mut settings = ApplySettings::default();
                settings.eqapo.level = ApplyLevel::Apply;
                runner.call(Request::ApplySettingsSet(settings));
                runner.call(Request::ApplyEqapo);
            }
            ApplyAction::Eqapo { .. } => {
                unreachable!("clap requires exactly one of project and --revert")
            }
            ApplyAction::Status => {
                runner.call(Request::ApplyStatus);
            }
        },
        Action::Host { action } => match action {
            HostAction::Status => {
                runner.call(Request::ApplyStatus);
            }
            HostAction::Install => {
                runner.call(Request::HostInstall);
            }
            HostAction::Uninstall => {
                runner.call(Request::HostUninstall);
            }
            HostAction::StopManaging => {
                runner.call(Request::HostStopManaging);
            }
        },
        Action::Run {
            out,
            project,
            fill,
            mimo,
        } => full_run(runner, &out, project.as_deref(), fill, mimo),
    }
}

/// Rows whose title names no position keep `--position` (TD-36): the scan's
/// suggested position for them is only the current position guess.
fn untagged_rows_use_position(scan: MdatScanResult) -> Vec<MdatAssignment> {
    scan.suggestions
        .into_iter()
        .map(|mut assignment| {
            let untagged = assignment.index.is_some_and(|index| {
                scan.measurements
                    .iter()
                    .any(|row| row.index == index && !row.position_tagged)
            });
            if untagged {
                assignment.position = None;
            }
            assignment
        })
        .collect()
}

fn check(runner: &mut Runner) {
    let info = match runner.engine.call(Request::Bootstrap) {
        Ok(Response::Bootstrap(info)) => info,
        Ok(_) => unreachable!("bootstrap has one response variant"),
        Err(error) => {
            runner.fail(error);
            return;
        }
    };
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    let serial =
        secs_compute::Context::serial().map((0_i64..32).collect(), |value| value * value + 3);
    let parallel_context = secs_compute::Context::new(
        available,
        secs_compute::MemoryBudget::new(u64::MAX),
        Arc::new(secs_compute::NoProgress),
        secs_compute::CancelToken::new(),
    );
    let parallel = parallel_context.map((0_i64..32).collect(), |value| value * value + 3);
    let parallel_status = match (serial, parallel) {
        (Ok(serial), Ok(parallel)) if serial == parallel => "ok".to_owned(),
        (Ok(_), Ok(_)) => {
            runner.ok = false;
            "wrong".to_owned()
        }
        (left, right) => {
            runner.ok = false;
            format!("error: {left:?} / {right:?}")
        }
    };
    print_json(&serde_json::json!({"reply": {"ok": runner.ok, "data": {
        "version": info.version,
        "upstream": info.upstream,
        "platform": info.platform,
        "available_parallelism": available,
        "rayon_threads": rayon::current_num_threads(),
        "parallel": parallel_status,
        "status": if runner.ok { "ok" } else { "error" }
    }}}));
}

fn full_run(runner: &mut Runner, out: &Path, project: Option<&Path>, fill: FillChoice, mimo: bool) {
    if let Some(project) = project {
        runner.open(project);
    } else {
        let path = out.join("project/sample.secs");
        runner.call(Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "7.1.4".into(),
            sub_count: 3,
            sample: true,
            tutorial: false,
        }));
        match fill {
            FillChoice::Sample => {
                runner.call(Request::SampleLoad(SampleLoadParams::default()));
            }
            FillChoice::Mock => {
                runner.configure_mock();
                runner.call(Request::MeasurePosition(MeasurePositionParams {
                    position: 1,
                }));
            }
        }
        runner.save();
    }
    // The project's own calibration settings apply; the CLI changes none.
    runner.calibrate(None, false);
    if mimo {
        runner.call(Request::Mimo(MimoParams { settings: None }));
    }
    let results = match runner.call(Request::Results) {
        Some(Response::Results(results)) => Some(results),
        _ => None,
    };
    let has_subs = results
        .as_ref()
        .and_then(|results| results.calibration.as_ref())
        .is_some_and(|calibration| !calibration.sub_roles.is_empty());
    let has_mimo = results.as_ref().is_some_and(|results| {
        results.mimo_export == secs_types::ipc::MimoExportDecision::Included
    });
    for (kind, folder) in [
        (ExportKind::CamillaDsp, "camilla-dsp"),
        (ExportKind::StagedMdat, "staged-mdat"),
        (ExportKind::BeforeAfterMdat, "before-after-mdat"),
        (ExportKind::FilterText, "filter-text"),
        (ExportKind::EqualizerApo, "equalizer-apo"),
        (ExportKind::Convolution, "convolution"),
    ] {
        let stale_cache = results
            .as_ref()
            .and_then(|results| results.calibration.as_ref())
            .is_some_and(|calibration| calibration.cached && calibration.stale);
        if let Some(block) = secs_types::ipc::export_block(kind, has_subs, has_mimo, stale_cache) {
            print_json(&serde_json::json!({"reply": {
                "ok": true,
                "data": {
                    "skipped": export_kind_name(kind),
                    "reason": block.name(),
                },
            }}));
            continue;
        }
        runner.export(
            kind,
            &out.join("export").join(folder),
            None,
            None,
            None,
            Vec::new(),
        );
    }
}

fn export_kind_name(kind: ExportKind) -> &'static str {
    match kind {
        ExportKind::CamillaDsp => "camilla-dsp",
        ExportKind::StagedMdat => "staged-mdat",
        ExportKind::BeforeAfterMdat => "before-after-mdat",
        ExportKind::FilterText => "filter-text",
        ExportKind::EqualizerApo => "equalizer-apo",
        ExportKind::Convolution => "convolution",
    }
}

pub(crate) fn print_json(value: &impl Serialize) {
    match serde_json::to_string(value) {
        Ok(line) => println!("{line}"),
        Err(error) => eprintln!("secs2r-cli: could not serialize output: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::runner::{deps, set_clock};
    use secs_api::EngineBuilder;

    static TEMP: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn apply_eqapo_opens_the_project_and_uses_the_injected_install() {
        let root = std::env::temp_dir().join(format!(
            "secs2r-cli-apply-{}-{}",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let config = root.join("eqapo");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("config.txt"), b"Preamp: -3 dB\n").unwrap();
        let install = EqApoInstall {
            config_path: config.to_string_lossy().into_owned(),
            install_path: "EqualizerAPO-XT/current".into(),
            variant: EqApoVariant::Xt,
        };
        let mut injected = deps(true, None);
        injected.app_data_path = root.join("app-data");
        injected.eqapo_install = Arc::new(move || Some(install.clone()));
        let mut runner = Runner {
            engine: EngineBuilder::from_deps(injected).build(),
            ok: true,
        };
        let project = root.join("room.secs");
        runner.call(Request::ProjectNew(ProjectNewParams {
            path: project.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: true,
            tutorial: false,
        }));
        runner.call(Request::SampleLoad(SampleLoadParams::default()));
        runner.calibrate(None, false);
        assert!(runner.ok, "oracle sample project preparation failed");

        run(
            Action::Apply {
                action: ApplyAction::Eqapo {
                    project: Some(project),
                    revert: false,
                },
            },
            &mut runner,
        );
        assert!(runner.ok);
        let entry = fs::read_to_string(config.join("SECS2R/SECS.txt")).unwrap();
        let generation = entry
            .strip_prefix("Include: ")
            .and_then(|value| value.strip_suffix("\\config.txt\r\n"))
            .expect("SECS.txt must contain one generation include");
        let generation = config.join("SECS2R").join(generation);
        assert!(generation.join("config.txt").is_file());
        assert!(generation.join("secs-design.json").is_file());
        assert!(
            fs::read_to_string(config.join("config.txt"))
                .unwrap()
                .contains("Include: SECS2R\\SECS.txt")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cli_export_records_utc_under_a_nonzero_clock_offset_td152() {
        use secs_service::LocalTime;

        let root = std::env::temp_dir().join(format!(
            "secs2r-cli-clock-{}-{}",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let mut injected = deps(true, None);
        set_clock(
            &mut injected,
            Arc::new(|| LocalTime {
                year: 2024,
                month: 1,
                day: 2,
                hour: 12,
                minute: 4,
                second: 5,
            }),
            Arc::new(|| 32_400),
            Arc::new(|| 1_704_164_645_000),
        );
        injected.app_data_path = root.join("app-data");
        let mut runner = Runner {
            engine: EngineBuilder::from_deps(injected).build(),
            ok: true,
        };
        runner.call(Request::ProjectNew(ProjectNewParams {
            path: root.join("room.secs").to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: true,
            tutorial: false,
        }));
        runner.call(Request::SampleLoad(SampleLoadParams::default()));
        runner.calibrate(None, false);
        let output = root.join("export");
        runner.export(
            ExportKind::EqualizerApo,
            &output,
            None,
            None,
            None,
            Vec::new(),
        );
        assert!(runner.ok);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("secs-design.json")).unwrap()).unwrap();
        let created = manifest["created_utc"].clone();
        let last_export = runner.engine.service().status().last_export.unwrap().time;
        fs::remove_dir_all(root).unwrap();
        assert_eq!(created, "2024-01-02T03:04:05.000Z");
        assert_eq!(last_export, "2024-01-02T03:04:05Z");
    }
}
