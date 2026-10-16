#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! SECS 2's user-visible REW `.mdat` exports.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use secs_camilla::pyfmt::{format_fixed, format_g};
use secs_engine::{
    Design, EngineConfig, EngineError, MeasurementSet, StageResponses, export_view,
    input::LayoutKind, stage_responses,
};
use secs_mdat::{MdatError, MeasurementInput, TEMPLATE};
use secs_types::{
    calibration::CalibrationSettings,
    chain::{MimoPaths, SubChannel},
};

const N_FFT: usize = 1 << 17;
const SR: u32 = 48_000;
const SUB_GAIN_WARN_DB: f64 = 12.0;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Inputs already computed by the correction engine.
#[derive(Debug)]
pub struct ExportRun<'a> {
    pub design: &'a Design,
    pub measurements: &'a MeasurementSet,
    pub config: &'a EngineConfig,
    pub settings: &'a CalibrationSettings,
}

/// MIMO support-filter realization used in exported labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MimoKind {
    Iir,
    Fir,
}

impl MimoKind {
    fn name(self) -> &'static str {
        match self {
            Self::Iir => "IIR",
            Self::Fir => "FIR",
        }
    }
}

/// MIMO paths and already-applied design-grid responses.
#[derive(Debug)]
pub struct MimoResponses {
    pub kind: MimoKind,
    pub fsiso: f64,
    pub cap_text: String,
    pub paths: MimoPaths,
    pub applied: Vec<(String, Vec<secs_engine::C>)>,
}

/// Typed future seam for M3 MIMO stage responses.
#[derive(Debug, Clone, Copy)]
pub enum MimoExport<'a> {
    None,
    Responses(&'a MimoResponses),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SubWarning {
    NarrowBand {
        role: String,
        hp_hz: f64,
        lp_hz: f64,
    },
    HighGain {
        role: String,
        gain_db: f64,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Mdat(#[from] MdatError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("operating-system randomness failed: {0}")]
    Random(String),
    #[error("missing export response for {0}")]
    MissingResponse(String),
}

fn chain_text(run: &ExportRun<'_>) -> String {
    let peq = run.config.peq_bands.unwrap_or(16);
    if run.config.taps > 0 {
        format!("PEQ {peq} + FIR {}", grouped(run.config.taps))
    } else {
        format!("PEQ {peq}")
    }
}

fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn sub_mode_text(mode: &str) -> String {
    if mode == "fixed" {
        "고정".into()
    } else if let Some(rest) = mode.strip_prefix("bundle:") {
        let mut parts = rest.split(':');
        let bundle = parts.next().unwrap_or_default();
        let place = match parts.next() {
            Some("dba-front") => " DBA 앞",
            Some("dba-rear") => " DBA 뒤",
            Some("multi") => " 멀티",
            _ => "",
        };
        format!("Group {bundle}{place}")
    } else {
        "자동".into()
    }
}

fn band_type_text(name: &str) -> String {
    let split = name.len().min(2);
    format!("{} {}", &name[..split], &name[split..])
}

fn sub_filter_text(sub: &SubChannel) -> String {
    let mut parts = Vec::new();
    if sub.hp_hz != 0.0 {
        parts.push(format!(
            "HP {} Hz {}",
            format_g(sub.hp_hz, false),
            band_type_text(sub.hp_type.name())
        ));
    }
    if sub.lp_hz != 0.0 {
        parts.push(format!(
            "LP {} Hz {}",
            format_g(sub.lp_hz, false),
            band_type_text(sub.lp_type.name())
        ));
    }
    parts.extend(sub.peq.iter().map(|&(hz, q, gain)| {
        format!(
            "PEQ {} Hz Q {} {} dB",
            format_fixed(hz, 0, false),
            format_fixed(q, 2, false),
            format_fixed(gain, 1, true)
        )
    }));
    parts.join(" ")
}

fn sub_value_text(sub: &SubChannel) -> String {
    let mut out = format!(
        "{} {} {} dB {} ms {}",
        sub.role,
        sub_mode_text(&sub.mode),
        format_fixed(sub.gain_db, 2, true),
        format_fixed(sub.delay_ms, 2, false),
        if sub.polarity > 0 { "정상" } else { "반전" }
    );
    for &(hz, q) in &sub.allpass {
        out.push_str(&format!(
            " AP {} Hz Q {}",
            format_fixed(hz, 0, false),
            format_fixed(q, 2, false)
        ));
    }
    let filters = sub_filter_text(sub);
    if !filters.is_empty() {
        out.push(' ');
        out.push_str(&filters);
    }
    out
}

pub fn layout_lines(run: &ExportRun<'_>) -> Vec<String> {
    let Some(subs) = &run.design.subs else {
        return Vec::new();
    };
    let mut layouts: Vec<_> = subs.layout_info.iter().collect();
    layouts.sort_by(|a, b| a.0.cmp(&b.0));
    layouts
        .into_iter()
        .map(|(name, info)| match info.kind {
            LayoutKind::Dba => format!(
                "Group {name} DBA: 앞 {}, 뒤 {} 반전 +{} ms(자동 {} 보정 {}), 같은 필터 PEQ {}",
                info.front.join("·"),
                info.rear.join("·"),
                format_fixed(info.rear_ms, 2, false),
                format_fixed(info.auto_ms.unwrap_or(0.0), 2, false),
                format_fixed(info.trim_ms, 2, true),
                info.peq.len()
            ),
            LayoutKind::Multi => format!(
                "Group {name} 멀티서브: {}, 같은 필터 PEQ {}",
                info.members.join("·"),
                info.peq.len()
            ),
        })
        .collect()
}

pub fn sub_values_text(run: &ExportRun<'_>) -> String {
    layout_lines(run)
        .into_iter()
        .chain(run.design.chain.subs.iter().map(sub_value_text))
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn rc_pieces(run: &ExportRun<'_>) -> Vec<String> {
    rc_pieces_from_values(
        run.design.rc_hi,
        run.design.rc_auto,
        run.design.schroeder_hz,
        run.settings.room_volume_m3,
        false,
    )
}

/// Builds the upstream RC note fragments from cached scalar values.
pub fn rc_pieces_from_values(
    rc_hi: f64,
    rc_auto: bool,
    schroeder_hz: Option<f64>,
    room_volume_m3: Option<f64>,
    legacy_fallback: bool,
) -> Vec<String> {
    let hz = format_fixed(rc_hi, 0, false);
    let head = if rc_auto {
        if let Some(schroeder) = schroeder_hz {
            format!(
                "RC 상한 {hz} Hz (슈뢰더 {} Hz)",
                format_fixed(schroeder, 0, false)
            )
        } else if legacy_fallback {
            format!("RC 상한 {hz} Hz (자동)")
        } else {
            format!("RC 상한 {hz} Hz (기본값)")
        }
    } else {
        format!("RC 상한 {hz} Hz (수동)")
    };
    if !rc_auto {
        return vec![head];
    }
    let Some(volume) = room_volume_m3.filter(|v| *v != 0.0) else {
        return vec![head, "자동: 룸 볼륨 없음".into()];
    };
    let mut out = vec![
        head,
        format!("자동: 룸 볼륨 {} m³", format_fixed(volume, 0, false)),
    ];
    if schroeder_hz.is_some_and(|v| v > rc_hi + 0.5) {
        out.push(format!("자동 상한 최대 {hz} Hz"));
    }
    out
}

pub fn sub_filter_warnings(settings: &CalibrationSettings, roles: &[String]) -> Vec<SubWarning> {
    roles
        .iter()
        .filter_map(|role| {
            let entry = settings
                .subs_manual
                .iter()
                .find(|(r, _)| r == role)
                .map(|(_, e)| e)?;
            (entry.hp_hz != 0.0 && entry.lp_hz != 0.0 && entry.lp_hz < 2.0 * entry.hp_hz).then(
                || SubWarning::NarrowBand {
                    role: role.clone(),
                    hp_hz: entry.hp_hz,
                    lp_hz: entry.lp_hz,
                },
            )
        })
        .collect()
}

pub fn sub_gain_warnings(design: &Design) -> Vec<SubWarning> {
    design
        .chain
        .subs
        .iter()
        .filter(|sub| sub.gain_db > SUB_GAIN_WARN_DB)
        .map(|sub| SubWarning::HighGain {
            role: sub.role.clone(),
            gain_db: sub.gain_db,
        })
        .collect()
}

fn ir(spectrum: &[secs_engine::C]) -> Vec<f64> {
    secs_dsp::fft::irfft(spectrum, N_FFT)
        .into_iter()
        .map(|sample| f64::from(sample as f32))
        .collect()
}

fn item(title: String, notes: String, spectrum: &[secs_engine::C], t0: f64) -> MeasurementInput {
    MeasurementInput {
        title,
        notes,
        sample_rate_hz: SR,
        samples: ir(spectrum),
        start_time_s: t0,
        spl_offset_db: None,
    }
}

fn stage_items_with_mimo(
    run: &ExportRun<'_>,
    mimo: MimoExport<'_>,
    label: Option<&str>,
) -> Result<Vec<MeasurementInput>, ExportError> {
    let view = export_view(run.design, run.measurements, run.config)?;
    let position = run.config.main_position;
    let subs: Vec<_> = run
        .config
        .subs
        .iter()
        .filter_map(|role| view.spectra.get(position, role))
        .collect();
    let label = label.map_or_else(|| run.settings.label(), str::to_owned);
    let shifted = view.base_s - run.design.chain.latency_ms / 1000.0;
    let mut items = Vec::new();
    for (index, main) in run.design.chain.mains.iter().enumerate() {
        let seq = index + 1;
        let raw = view
            .spectra
            .get(position, &main.role)
            .ok_or_else(|| ExportError::MissingResponse(main.role.clone()))?;
        let stages = stage_responses(run.design, &main.role, &view.spectra, position)
            .ok_or_else(|| ExportError::MissingResponse(main.role.clone()))?;
        let prefix = format!("SECS 보정 설계 · {label} · 청취 위치 {position} · 계산값 — ");
        items.push(item(
            format!("{seq:02}_{}_1 보정 전 메인", main.role),
            format!("{prefix}메인 스피커 원 응답(2 s 창, 필터 없음)"),
            raw,
            view.base_s,
        ));
        if !subs.is_empty() {
            let mut sum = vec![secs_engine::C::new(0.0, 0.0); raw.len()];
            for spectrum in &subs {
                for (out, value) in sum.iter_mut().zip(spectrum.iter()) {
                    *out += value;
                }
            }
            let tail = if view.has_sub_filters {
                "서브 원 응답의 단순 합(정렬 없음 — 서브 고정 출력 필터만)"
            } else {
                "서브 원 응답의 단순 합(정렬 없음·필터 없음)"
            };
            items.push(item(
                format!("{seq:02}_{}_1 보정 전 서브", main.role),
                format!("{prefix}{tail}"),
                &sum,
                view.base_s,
            ));
        }
        add_xo_items(
            run,
            &mut items,
            seq,
            main.role.as_str(),
            &prefix,
            &stages,
            view.base_s,
        );
        items.push(item(
            format!("{seq:02}_{}_3 합 후 보정", main.role),
            format!(
                "{prefix}2차: 탑+서브 합에 입력 보정({}) · 전체 지연 {} ms(시작 시각에서 뺌) · RC 상한 {} Hz",
                chain_text(run),
                format_fixed(run.design.chain.latency_ms, 1, false),
                format_fixed(run.design.rc_hi, 0, false)
            ),
            &stages.output,
            shifted,
        ));
        if let MimoExport::Responses(mimo) = mimo {
            let supports: Vec<_> = mimo
                .paths
                .paths
                .iter()
                .filter(|path| path.input == main.role)
                .collect();
            if !supports.is_empty() {
                let response = mimo
                    .applied
                    .iter()
                    .find(|(role, _)| role == &main.role)
                    .map(|(_, response)| response.as_slice())
                    .ok_or_else(|| ExportError::MissingResponse(main.role.clone()))?;
                let description = supports
                    .iter()
                    .map(|path| match &path.filter {
                        secs_types::chain::MimoFilter::Fir { taps_48k } => {
                            format!("{} FIR {}탭", path.support, grouped(taps_48k.len()))
                        }
                        secs_types::chain::MimoFilter::Iir {
                            gain_db,
                            delay_ms,
                            polarity,
                            peq,
                            allpass,
                        } => format!(
                            "{} {} dB {} ms {} PEQ {} AP {}",
                            path.support,
                            format_fixed(*gain_db, 1, false),
                            format_fixed(*delay_ms, 1, false),
                            if *polarity > 0 { "+" } else { "-" },
                            peq.len(),
                            allpass.len()
                        ),
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                items.push(item(
                    format!("{seq:02}_{}_4 MIMO 적용({})", main.role, mimo.kind.name()),
                    format!(
                        "{prefix}MIMO {} 서포트 {}개(Fsiso {} Hz{}) — {description}",
                        mimo.kind.name(),
                        supports.len(),
                        format_g(mimo.fsiso, false),
                        mimo.cap_text
                    ),
                    response,
                    shifted,
                ));
            }
        }
    }
    Ok(items)
}

fn add_xo_items(
    run: &ExportRun<'_>,
    items: &mut Vec<MeasurementInput>,
    seq: usize,
    role: &str,
    prefix: &str,
    stages: &StageResponses,
    base_s: f64,
) {
    let Some(xo) = run
        .design
        .chain
        .mains
        .iter()
        .find(|m| m.role == role)
        .and_then(|m| m.xo_hz)
        .filter(|v| *v != 0.0)
    else {
        return;
    };
    items.push(item(
        format!("{seq:02}_{role}_2 XO 타겟 메인"),
        format!(
            "{prefix}1차: 게인·딜레이·출력 PEQ(음향 XO 타겟)·전기 HP {} Hz",
            format_g(xo, false)
        ),
        &stages.top_xo,
        base_s,
    ));
    if let Some(sub) = &stages.sub_xo {
        items.push(item(
            format!("{seq:02}_{role}_2 XO 타겟 서브"),
            format!(
                "{prefix}1차: BA(게인·지연·극성)·서브 버스 PEQ·전기 LP — {}",
                sub_values_text(run)
            ),
            sub,
            base_s,
        ));
    }
}

pub fn stage_items(run: &ExportRun<'_>, label: Option<&str>) -> Vec<MeasurementInput> {
    stage_items_with_mimo(run, MimoExport::None, label).expect("validated engine export inputs")
}

pub fn stage_items_mimo(
    run: &ExportRun<'_>,
    mimo: &MimoResponses,
    label: Option<&str>,
) -> Result<Vec<MeasurementInput>, ExportError> {
    stage_items_with_mimo(run, MimoExport::Responses(mimo), label)
}

fn before_after_items_with_mimo(
    run: &ExportRun<'_>,
    mimo: MimoExport<'_>,
) -> Result<Vec<MeasurementInput>, ExportError> {
    let view = export_view(run.design, run.measurements, run.config)?;
    let position = run.config.main_position;
    let label = run.settings.label();
    let shifted = view.base_s - run.design.chain.latency_ms / 1000.0;
    let mut items = Vec::new();
    let mut seq = 0;
    for main in &run.design.chain.mains {
        seq += 1;
        if let Some(measured) = run.measurements.get(position, &main.role) {
            items.push(measured_item(seq, &main.role, position, measured, ""));
        }
        let final_response = stage_responses(run.design, &main.role, &view.spectra, position)
            .ok_or_else(|| ExportError::MissingResponse(main.role.clone()))?
            .output;
        let fir = if main.fir.len() > 1 {
            format!("FIR {}탭", grouped(main.fir.len()))
        } else {
            "FIR 없음".into()
        };
        let mut notes = format!(
            "SECS 보정 설계 · {label} · 청취 위치 {position} · 계산값 — 채널 보정(PEQ {}개 · {fir}) + 서브 합, 전체 지연 {} ms(시작 시각에서 뺌) · {}",
            main.input_peq.len(),
            format_fixed(run.design.chain.latency_ms, 1, false),
            rc_pieces(run).join(" · ")
        );
        if !run.design.chain.subs.is_empty() && main.xo_hz.is_some_and(|v| v != 0.0) {
            notes.push_str(" · 서브: ");
            notes.push_str(&sub_values_text(run));
        }
        items.push(item(
            format!("{seq:02}_{}_2 보정 후 {label}", main.role),
            notes,
            &final_response,
            shifted,
        ));
        if let MimoExport::Responses(mimo) = mimo {
            let support_count = mimo
                .paths
                .paths
                .iter()
                .filter(|path| path.input == main.role)
                .count();
            if support_count != 0 {
                let response = mimo
                    .applied
                    .iter()
                    .find(|(role, _)| role == &main.role)
                    .map(|(_, response)| response.as_slice())
                    .ok_or_else(|| ExportError::MissingResponse(main.role.clone()))?;
                items.push(item(
                    format!(
                        "{seq:02}_{}_3 보정 후 + MIMO({})",
                        main.role,
                        mimo.kind.name()
                    ),
                    format!(
                        "SECS 보정 설계 · {label} · 청취 위치 {position} · 계산값 — MIMO {} 서포트 {support_count}개 (Fsiso {} Hz{})",
                        mimo.kind.name(),
                        format_g(mimo.fsiso, false),
                        mimo.cap_text
                    ),
                    response,
                    shifted,
                ));
            }
        }
    }
    for sub in &run.design.chain.subs {
        if let Some(measured) = run.measurements.get(position, &sub.role) {
            seq += 1;
            items.push(measured_item(
                seq,
                &sub.role,
                position,
                measured,
                &format!(" · DSP 값: {}", sub_value_text(sub)),
            ));
        }
    }
    Ok(items)
}

fn measured_item(
    seq: usize,
    role: &str,
    position: u32,
    measured: &secs_engine::Measurement,
    note: &str,
) -> MeasurementInput {
    MeasurementInput {
        title: format!("{seq:02}_{role}_1 보정 전(원 측정)"),
        notes: format!("SECS · 청취 위치 {position} · 원 측정 그대로(필터 없음){note}"),
        sample_rate_hz: measured.sample_rate,
        samples: (0..measured.samples.len())
            .map(|index| f64::from(measured.samples.at(index) as f32))
            .collect(),
        start_time_s: measured.t0,
        spl_offset_db: None,
    }
}

pub fn before_after_items(run: &ExportRun<'_>) -> Vec<MeasurementInput> {
    before_after_items_with_mimo(run, MimoExport::None).expect("validated engine export inputs")
}

pub fn before_after_items_mimo(
    run: &ExportRun<'_>,
    mimo: &MimoResponses,
) -> Result<Vec<MeasurementInput>, ExportError> {
    before_after_items_with_mimo(run, MimoExport::Responses(mimo))
}

/// `spl_offset_db` is the project's median REW SPL offset
/// (`secs_project::project_spl_offset`).
pub fn export_mdat(
    run: &ExportRun<'_>,
    folder: &Path,
    spl_offset_db: Option<f64>,
) -> Result<PathBuf, ExportError> {
    export_mdat_with_mimo(run, folder, None, spl_offset_db)
}

pub fn export_mdat_with_mimo(
    run: &ExportRun<'_>,
    folder: &Path,
    mimo: Option<&MimoResponses>,
    spl_offset_db: Option<f64>,
) -> Result<PathBuf, ExportError> {
    export_mdat_with_mimo_using(run, folder, mimo, spl_offset_db, &mut || {
        random_uuid().map_err(|error| ExportError::Random(error.to_string()))
    })
}

/// Writes the staged mdat using an injected UUID source.
pub fn export_mdat_with_mimo_using(
    run: &ExportRun<'_>,
    folder: &Path,
    mimo: Option<&MimoResponses>,
    spl_offset_db: Option<f64>,
    uuid: &mut dyn FnMut() -> Result<u128, ExportError>,
) -> Result<PathBuf, ExportError> {
    let label = run.settings.label();
    let items = stage_items_with_mimo(
        run,
        mimo.map_or(MimoExport::None, MimoExport::Responses),
        Some(&label),
    )?;
    export_items_using(
        folder,
        &format!("SECS 보정 설계 {label}.mdat"),
        &items,
        spl_offset_db,
        uuid,
    )
}

pub fn export_before_after_mdat(
    run: &ExportRun<'_>,
    folder: &Path,
    spl_offset_db: Option<f64>,
) -> Result<PathBuf, ExportError> {
    export_before_after_mdat_with_mimo(run, folder, None, spl_offset_db)
}

pub fn export_before_after_mdat_with_mimo(
    run: &ExportRun<'_>,
    folder: &Path,
    mimo: Option<&MimoResponses>,
    spl_offset_db: Option<f64>,
) -> Result<PathBuf, ExportError> {
    export_before_after_mdat_with_mimo_using(run, folder, mimo, spl_offset_db, &mut || {
        random_uuid().map_err(|error| ExportError::Random(error.to_string()))
    })
}

/// Writes the before/after mdat using an injected UUID source.
pub fn export_before_after_mdat_with_mimo_using(
    run: &ExportRun<'_>,
    folder: &Path,
    mimo: Option<&MimoResponses>,
    spl_offset_db: Option<f64>,
    uuid: &mut dyn FnMut() -> Result<u128, ExportError>,
) -> Result<PathBuf, ExportError> {
    let label = run.settings.label();
    let items =
        before_after_items_with_mimo(run, mimo.map_or(MimoExport::None, MimoExport::Responses))?;
    export_items_using(
        folder,
        &format!("SECS 보정 전후 {label}.mdat"),
        &items,
        spl_offset_db,
        uuid,
    )
}

fn export_items_using(
    folder: &Path,
    filename: &str,
    items: &[MeasurementInput],
    spl_offset_db: Option<f64>,
    uuid: &mut dyn FnMut() -> Result<u128, ExportError>,
) -> Result<PathBuf, ExportError> {
    let uuids = (0..items.len())
        .map(|_| uuid())
        .collect::<Result<Vec<_>, _>>()?;
    let mut uuids = uuids.into_iter();
    let bytes = secs_mdat::build(items, TEMPLATE, spl_offset_db, &mut || {
        uuids
            .next()
            .expect("one pre-generated UUID per validated measurement")
    })?;
    fs::create_dir_all(folder)?;
    let destination = folder.join(filename);
    atomic_replace(&destination, &bytes)?;
    Ok(destination)
}

fn random_uuid() -> Result<u128, getrandom::Error> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)?;
    let mut value = u128::from_be_bytes(bytes);
    value &= !(0xf_u128 << 76);
    value |= 4_u128 << 76;
    value &= !(0x3_u128 << 62);
    value |= 0x2_u128 << 62;
    Ok(value)
}

fn atomic_replace(destination: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let stem = destination
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("export.mdat");
    let mut temporary = None;
    for _ in 0..100 {
        let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{stem}.{}.{}.tmp", std::process::id(), serial));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let Some((path, mut file)) = temporary else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create export temporary file",
        ));
    };
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&path, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc_pieces_from_values_preserves_live_and_legacy_wording() {
        assert_eq!(
            rc_pieces_from_values(300.0, true, None, None, false),
            ["RC 상한 300 Hz (기본값)", "자동: 룸 볼륨 없음"]
        );
        assert_eq!(
            rc_pieces_from_values(300.0, true, None, None, true),
            ["RC 상한 300 Hz (자동)", "자동: 룸 볼륨 없음"]
        );
        assert_eq!(
            rc_pieces_from_values(300.0, false, None, Some(80.0), true),
            ["RC 상한 300 Hz (수동)"]
        );
        assert_eq!(
            rc_pieces_from_values(300.0, true, Some(420.0), Some(80.0), false),
            [
                "RC 상한 300 Hz (슈뢰더 420 Hz)",
                "자동: 룸 볼륨 80 m³",
                "자동 상한 최대 300 Hz",
            ]
        );
    }

    #[test]
    fn atomic_replace_overwrites_and_cleans_temporary_file() {
        let root = std::env::temp_dir().join(format!(
            "secs-export-atomic-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let destination = root.join("SECS 보정 설계 표준 모드.mdat");
        fs::write(&destination, b"old").unwrap();

        atomic_replace(&destination, b"new").unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"new");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_replace_cleans_temporary_file_after_rename_failure() {
        let root = std::env::temp_dir().join(format!(
            "secs-export-atomic-failure-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let destination = root.join("occupied");
        fs::create_dir(&destination).unwrap();

        assert!(atomic_replace(&destination, b"new").is_err());

        assert!(destination.is_dir());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn random_uuid_has_v4_variant() {
        let uuid = random_uuid().unwrap();
        assert_eq!((uuid >> 76) & 0xf, 4);
        assert_eq!((uuid >> 62) & 0x3, 2);
    }

    #[test]
    fn warning_edges() {
        let mut settings = CalibrationSettings::default();
        let entry = secs_types::calibration::SubEntry {
            hp_hz: 30.0,
            lp_hz: 50.0,
            ..secs_types::calibration::SubEntry::default()
        };
        settings.subs_manual.push(("SW1".into(), entry));
        assert_eq!(
            sub_filter_warnings(&settings, &["SW1".into()]),
            vec![SubWarning::NarrowBand {
                role: "SW1".into(),
                hp_hz: 30.0,
                lp_hz: 50.0,
            }]
        );
    }
}
