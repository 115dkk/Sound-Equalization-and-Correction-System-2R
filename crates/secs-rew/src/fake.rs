use crate::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

/// Caller owns sample data, synthesis, resampling and any one-position sample
/// restriction. Called for each role/position; no source path is discovered here.
pub trait IrSource: Send + Sync {
    fn impulse_response(
        &self,
        role: &str,
        pos: u32,
        sample_rate: u32,
    ) -> Result<ImpulseResponse, RewError>;
    fn snr_db(&self, _role: &str, _pos: u32) -> Option<f64> {
        None
    }
}
#[derive(Debug, Clone)]
pub struct FakeOptions {
    pub delay: Duration,
    pub sample_rate: u32,
    pub running: bool,
    pub version: String,
    pub pro: bool,
    pub level_interval: Duration,
    pub measure_level_dbfs: f64,
    pub cal_file: String,
    pub output_devices: Vec<DeviceInfo>,
    pub input_devices: Vec<DeviceInfo>,
}
fn device(id: &str, name: &str, channels: usize, kind: &str) -> DeviceInfo {
    DeviceInfo {
        id: id.into(),
        name: name.into(),
        channels,
        kind: kind.into(),
        extra: json!({}),
    }
}
impl Default for FakeOptions {
    fn default() -> Self {
        Self {
            delay: Duration::from_millis(50),
            sample_rate: 48000,
            running: true,
            version: "5.40 beta 135 (demo)".into(),
            pro: true,
            level_interval: Duration::from_millis(20),
            measure_level_dbfs: -20.0,
            cal_file: String::new(),
            output_devices: vec![
                device("fake-out-0", "Demo ASIO 16ch", 16, "output"),
                device("fake-out-1", "Demo Speakers (2ch)", 2, "output"),
            ],
            input_devices: vec![
                device("fake-in-0", "Demo USB Mic", 1, "input"),
                device("fake-in-1", "Demo Interface In", 8, "input"),
            ],
        }
    }
}
#[derive(Debug, Default)]
struct Generator {
    running: bool,
    level_db: f64,
}
pub struct FakeRew<S: IrSource> {
    source: S,
    pub options: FakeOptions,
    by_uuid: BTreeMap<String, ImpulseResponse>,
    counter: u64,
    generator: Arc<Mutex<Generator>>,
    level_stop: CancelToken,
    level_thread: Option<JoinHandle<()>>,
    pub calls: Vec<(String, Value)>,
}
impl<S: IrSource> std::fmt::Debug for FakeRew<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeRew")
            .field("options", &self.options)
            .field("measurements", &self.by_uuid.len())
            .finish_non_exhaustive()
    }
}
impl<S: IrSource> FakeRew<S> {
    pub fn new(source: S) -> Self {
        Self::with_options(source, FakeOptions::default())
    }
    pub fn with_options(source: S, options: FakeOptions) -> Self {
        Self {
            source,
            options,
            by_uuid: BTreeMap::new(),
            counter: 0,
            generator: Arc::new(Mutex::new(Generator::default())),
            level_stop: CancelToken::default(),
            level_thread: None,
            calls: vec![],
        }
    }
    pub fn is_running(&self) -> bool {
        self.options.running
    }
    pub fn version(&self) -> Option<String> {
        self.options.running.then(|| self.options.version.clone())
    }
    pub fn is_pro(&self) -> bool {
        self.options.running && self.options.pro
    }
    pub fn set_running(&mut self, value: bool) {
        self.options.running = value;
    }
    pub fn measure_level_dbfs(&self) -> Option<f64> {
        self.options
            .running
            .then_some(self.options.measure_level_dbfs)
    }
    pub fn input_calibration_file(&self) -> Option<String> {
        self.options.running.then(|| self.options.cal_file.clone())
    }
    pub fn output_devices(&self) -> Vec<DeviceInfo> {
        self.options.output_devices.clone()
    }
    pub fn input_devices(&self) -> Vec<DeviceInfo> {
        self.options.input_devices.clone()
    }
    pub fn output_channel_names(&self) -> Vec<String> {
        (1..=self
            .options
            .output_devices
            .iter()
            .map(|d| d.channels)
            .max()
            .unwrap_or(2))
            .map(|n| format!("{n}: Demo Out {n}"))
            .collect()
    }
    pub fn input_channel_names(&self) -> Vec<String> {
        (1..=self
            .options
            .input_devices
            .iter()
            .map(|d| d.channels)
            .max()
            .unwrap_or(1))
            .map(|n| format!("{n}: Demo In {n}"))
            .collect()
    }
    pub fn generator_running(&self) -> bool {
        self.generator.lock().unwrap().running
    }
    pub fn level_running(&self) -> bool {
        self.level_thread.is_some()
    }
    pub fn start(&mut self, callback: LevelCallback) {
        self.stop();
        self.level_stop = CancelToken::default();
        let stop = self.level_stop.clone();
        let generator = self.generator.clone();
        let interval = self.options.level_interval.max(Duration::from_millis(1));
        self.calls.push(("level_start".into(), json!({})));
        self.level_thread = Some(thread::spawn(move || {
            let mut tick = 0_u64;
            while !stop.is_cancelled() {
                let level = {
                    let g = generator.lock().unwrap();
                    level_sample(g.running, g.level_db, tick)
                };
                emit(&Some(callback.clone()), level);
                tick += 1;
                if stop.wait(interval) {
                    break;
                }
            }
        }));
    }
    pub fn stop(&mut self) {
        self.level_stop.cancel();
        if let Some(t) = self.level_thread.take() {
            let _ = t.join();
        }
    }
    pub fn play_pink(&mut self, role: &str, output: u32, level_db: f64) {
        let mut g = self.generator.lock().unwrap();
        g.running = true;
        g.level_db = level_db;
        self.calls.push((
            "play_pink".into(),
            json!({"role":role,"output":output,"level_db":level_db}),
        ));
    }
    pub fn stop_generator(&mut self) {
        self.generator.lock().unwrap().running = false;
        self.calls.push(("stop_generator".into(), json!({})));
    }
    pub fn ir_for(
        &self,
        role: &str,
        pos: u32,
        sample_rate: Option<u32>,
    ) -> Result<ImpulseResponse, RewError> {
        self.source.impulse_response(
            role,
            pos,
            sample_rate
                .filter(|r| *r > 0)
                .unwrap_or(self.options.sample_rate),
        )
    }
    pub fn get_ir(&self, uuid: &str) -> Result<(Vec<f32>, u32), RewError> {
        let ir = self
            .by_uuid
            .get(uuid)
            .ok_or_else(|| RewError::schema("uuid", "unknown measurement"))?;
        Ok((ir.ir.clone(), ir.sample_rate))
    }
    pub fn impulse_response(&self, uuid: &str) -> Result<ImpulseResponse, RewError> {
        self.by_uuid
            .get(uuid)
            .cloned()
            .ok_or_else(|| RewError::schema("uuid", "unknown measurement"))
    }
    pub fn measurements(&self) -> Vec<Value> {
        self.by_uuid
            .iter()
            .map(|(u, ir)| json!({"uuid":u,"sampleRate":ir.sample_rate,"samples":ir.ir.len()}))
            .collect()
    }
    pub fn run_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: u32,
        on_progress: Option<ProgressCallback>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, RewError> {
        self.calls
            .push(("run_position".into(), json!({"pos":pos,"roles":plan.roles})));
        let mut results = vec![];
        for role in &plan.roles {
            if cancel.is_cancelled() {
                emit(
                    &on_progress,
                    progress(role, pos, "cancelled", 0.0, "측정을 취소했습니다"),
                );
                break;
            }
            emit(
                &on_progress,
                progress(
                    role,
                    pos,
                    "sweep",
                    0.0,
                    &format!("{role} 스윕 재생 중 (위치 {pos})"),
                ),
            );
            for step in 1..=5 {
                if cancel.wait(self.options.delay / 5) {
                    break;
                }
                emit(
                    &on_progress,
                    progress(
                        role,
                        pos,
                        "running",
                        90.0 * f64::from(step) / 5.0,
                        &format!("{role} 스윕 {step}/5"),
                    ),
                );
            }
            if cancel.is_cancelled() {
                emit(
                    &on_progress,
                    progress(role, pos, "cancelled", 0.0, "측정을 취소했습니다"),
                );
                break;
            }
            emit(
                &on_progress,
                progress(
                    role,
                    pos,
                    "process",
                    90.0,
                    &format!("{role} 임펄스 응답 계산"),
                ),
            );
            let ir = self.ir_for(role, pos, Some(plan.sample_rate))?;
            self.counter += 1;
            let uuid = format!("fake-{}-P{pos}-{:04}", role.replace(':', "_"), self.counter);
            self.by_uuid.insert(uuid.clone(), ir.clone());
            results.push(MeasurementResult {
                role: role.clone(),
                pos,
                uuid: Some(uuid),
                ir: ir.ir,
                sample_rate: ir.sample_rate,
                name: plan.name_for(role, pos),
                spl_db: Some(75.0 + 0.25 * f64::from(plan.output_for(role))),
                t0_s: ir.t0_s,
                spl_offset_db: None,
                snr_db: self.source.snr_db(role, pos),
            });
            emit(
                &on_progress,
                progress(role, pos, "done", 100.0, &format!("{role} 완료")),
            );
        }
        Ok(results)
    }
    pub fn close(&mut self) {
        self.stop();
        self.stop_generator();
    }
}
impl<S: IrSource> Drop for FakeRew<S> {
    fn drop(&mut self) {
        self.close();
    }
}
fn level_sample(running: bool, level_db: f64, tick: u64) -> Level {
    let dbfs = if running {
        level_db + 2.0 + 1.2 * (tick as f64 * 0.9).sin()
    } else {
        -62.0 + 1.5 * (tick as f64 * 0.7).sin()
    };
    Level {
        input_dbfs: dbfs,
        spl_db: running.then_some(75.0 + dbfs - level_db),
        clip: dbfs > -1.0,
    }
}
pub fn fake_plan(roles: &[&str], positions: &[u32]) -> MeasurementPlan {
    MeasurementPlan {
        roles: roles.iter().map(|s| (*s).into()).collect(),
        outputs: roles
            .iter()
            .enumerate()
            .map(|(i, r)| ((*r).into(), (i + 1) as u32))
            .collect(),
        positions: positions.to_vec(),
        output_device: "Demo ASIO 16ch".into(),
        input_device: "Demo USB Mic".into(),
        ..MeasurementPlan::default()
    }
}
