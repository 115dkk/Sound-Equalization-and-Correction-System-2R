use crate::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub fn b64_be_f32(encoded: &str) -> Result<Vec<f32>, RewError> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|e| RewError::schema("impulse-response", e))?;
    if bytes.len() % 4 != 0 {
        return Err(RewError::schema("impulse-response", "float32 byte length"));
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_be_bytes(*b))
        .collect())
}
pub fn channel_name_for(names: &[String], channel: u32) -> Option<String> {
    let prefix = format!("{channel}:");
    names
        .iter()
        .find(|n| n.trim().starts_with(&prefix))
        .or_else(|| channel.checked_sub(1).and_then(|n| names.get(n as usize)))
        .cloned()
}
pub fn parse_progress(body: &Value) -> (Option<f64>, String) {
    let text = match body {
        Value::Null => String::new(),
        Value::String(s) => s.trim().to_owned(),
        _ => body.to_string(),
    };
    let (head, rest) = text.split_once('%').unwrap_or((&text, ""));
    match head.trim().parse::<f64>() {
        Ok(n) if n.is_finite() => (Some(n), rest.trim().into()),
        _ => (None, text),
    }
}
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| n.is_finite())
}
fn first(value: &Value, index: usize) -> Option<f64> {
    if let Some(v) = value.as_array() {
        v.get(index).or_else(|| v.first()).and_then(number)
    } else {
        number(value)
    }
}
pub fn level_payload(payload: &Value, index: usize) -> Level {
    let nested = payload
        .get("inputs")
        .or_else(|| payload.get("levels"))
        .and_then(Value::as_array)
        .and_then(|v| v.first())
        .filter(|v| v.is_object());
    let data = nested.unwrap_or(payload);
    let rms = first(
        data.get("rms")
            .or_else(|| data.get("rmsLevel"))
            .or_else(|| data.get("level"))
            .unwrap_or(&Value::Null),
        index,
    );
    let peak = first(
        data.get("peak")
            .or_else(|| data.get("peakLevel"))
            .unwrap_or(&Value::Null),
        index,
    );
    let mut spl = first(
        data.get("spl")
            .or_else(|| data.get("SPL"))
            .unwrap_or(&Value::Null),
        index,
    );
    if spl.is_none()
        && data["unit"]
            .as_str()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("spl")
    {
        spl = rms.or(peak);
    }
    Level {
        input_dbfs: rms.or(peak).unwrap_or(-120.0),
        peak_dbfs: peak.or(rms).unwrap_or(-120.0),
        spl_db: spl.filter(|n| *n > -179.0),
        clip: peak.is_some_and(|n| n > -1.0),
    }
}

#[derive(Debug)]
pub struct RewClient<T: Transport> {
    transport: T,
    pub timeout: Duration,
    pub measure_timeout: Duration,
    pub poll_interval: Duration,
    pub level_index: usize,
    driver: Option<String>,
    pro_hint: Option<bool>,
    down_until: Option<Instant>,
    cache: HashMap<String, (Instant, Value)>,
    callback: Option<CallbackServer>,
    level_url: Option<String>,
    progress_url: Option<String>,
    measuring: bool,
}
impl<T: Transport> RewClient<T> {
    /// Construction never contacts REW.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            timeout: READ_TIMEOUT,
            measure_timeout: MEASURE_TIMEOUT,
            poll_interval: Duration::from_millis(500),
            level_index: 0,
            driver: None,
            pro_hint: None,
            down_until: None,
            cache: HashMap::new(),
            callback: None,
            level_url: None,
            progress_url: None,
            measuring: false,
        }
    }
    pub fn transport(&self) -> &T {
        &self.transport
    }
    fn request(&mut self, request: Request, status_probe: bool) -> Result<Value, RewError> {
        let guarded = status_probe && !self.measuring;
        if guarded && self.down_until.is_some_and(|t| t > Instant::now()) {
            return Err(RewError::connection(
                &request.path,
                "recent connection failure",
            ));
        }
        let response = match self.transport.request(&request) {
            Ok(r) => r,
            Err(e) => {
                if guarded && matches!(e, RewError::Connection { .. }) {
                    self.down_until = Some(Instant::now() + DOWN_FAST_FAIL);
                }
                return Err(e);
            }
        };
        if guarded {
            self.down_until = None;
        }
        // R1: authorization/payment statuses indicate the Pro gate regardless
        // of path. Never infer licensing from human-readable text.
        if matches!(response.status, 401..=403) {
            return Err(RewError::ProRequired {
                status: response.status,
                path: request.path,
            });
        }
        if response.status >= 400 {
            return Err(RewError::Http {
                status: response.status,
                path: request.path,
                message: response.body.to_string().chars().take(200).collect(),
            });
        }
        Ok(response.body)
    }
    pub fn get(&mut self, path: &str) -> Result<Value, RewError> {
        self.get_timeout(path, self.timeout, false)
    }
    fn get_timeout(
        &mut self,
        path: &str,
        timeout: Duration,
        probe: bool,
    ) -> Result<Value, RewError> {
        self.request(Request::new("GET", path, None, timeout), probe)
    }
    pub fn post(&mut self, path: &str, body: Value) -> Result<Value, RewError> {
        self.post_timeout(path, body, self.timeout)
    }
    fn post_timeout(
        &mut self,
        path: &str,
        body: Value,
        timeout: Duration,
    ) -> Result<Value, RewError> {
        self.request(Request::new("POST", path, Some(body), timeout), false)
    }
    pub fn hold_offline(&mut self, duration: Duration) {
        self.down_until = Some(Instant::now() + duration);
    }
    pub fn probe(&mut self) -> bool {
        self.down_until = None;
        if self
            .get_timeout("/version", Duration::from_secs(1), true)
            .is_err()
        {
            self.down_until = Some(Instant::now() + PROBE_HOLD);
            false
        } else {
            true
        }
    }
    pub fn is_running(&mut self) -> bool {
        self.get_timeout("/version", self.timeout.min(Duration::from_secs(2)), true)
            .is_ok()
    }
    pub fn version(&mut self) -> Option<String> {
        let data = self
            .get_timeout("/version", self.timeout.min(Duration::from_secs(2)), true)
            .ok()?;
        if data.is_null() {
            None
        } else {
            Some(
                data.get("message")
                    .and_then(Value::as_str)
                    .or_else(|| data.as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| data.to_string()),
            )
        }
    }
    /// Legacy per-client hint. Service code keeps the authoritative observation.
    pub fn pro_state(&self) -> Option<bool> {
        self.pro_hint
    }
    pub fn is_pro(&self) -> bool {
        self.pro_hint.unwrap_or(true)
    }
    pub fn set_pro(&mut self, value: Option<bool>) {
        self.pro_hint = value;
    }
    fn read_driver(&mut self) -> Result<String, RewError> {
        if let Some((until, value)) = self.cache.get("driver")
            && *until > Instant::now()
        {
            return Ok(value.as_str().unwrap_or("Java").to_owned());
        }
        let data = self.get_timeout("/audio/driver", self.timeout, true)?;
        let driver = data
            .get("driver")
            .unwrap_or(&data)
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("Java")
            .to_owned();
        self.driver = Some(driver.clone());
        self.cache.insert(
            "driver".into(),
            (Instant::now() + LIST_CACHE, Value::String(driver.clone())),
        );
        Ok(driver)
    }
    pub fn driver(&mut self) -> String {
        self.read_driver()
            .unwrap_or_else(|_| self.driver.clone().unwrap_or_else(|| "Java".into()))
    }
    pub fn set_driver(&mut self, name: &str) -> Result<(), RewError> {
        self.post_timeout(
            "/audio/driver",
            json!({"driver":name}),
            self.timeout.max(Duration::from_secs(30)),
        )?;
        self.driver = None;
        self.cache.clear();
        Ok(())
    }
    fn asio(&mut self) -> bool {
        self.driver().trim().eq_ignore_ascii_case("asio")
    }
    fn audio_value(value: Value) -> Option<String> {
        let text = value.as_str().or_else(|| {
            ["device", "name", "input", "output", "channel", "value"]
                .into_iter()
                .find_map(|key| value.get(key).and_then(Value::as_str))
        })?;
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_owned())
    }
    fn current_audio_value(&mut self, path: &str) -> Option<String> {
        Self::audio_value(self.get_timeout(path, self.timeout, true).ok()?)
    }
    pub fn current_device(&mut self, kind: &str) -> Option<String> {
        let path = self.audio_path(&format!("{kind}_device"));
        self.current_audio_value(&path)
    }
    pub fn set_device(&mut self, kind: &str, name: &str) -> Result<(), RewError> {
        let path = self.audio_path(&format!("{kind}_device"));
        self.post_timeout(
            &path,
            json!({"device":name}),
            self.timeout.max(Duration::from_secs(30)),
        )?;
        self.cache.clear();
        Ok(())
    }
    pub fn current_audio(&mut self) -> CurrentAudio {
        if self.asio() {
            let device = self.current_device("output");
            CurrentAudio {
                input_device: device.clone(),
                output_device: device,
                input: self.current_audio_value("/audio/asio/input"),
            }
        } else {
            CurrentAudio {
                input_device: self.current_device("input"),
                output_device: self.current_device("output"),
                input: self.current_audio_value("/audio/java/input"),
            }
        }
    }
    fn audio_path(&mut self, kind: &str) -> String {
        let asio = self.asio();
        format!(
            "/audio/{}/{}",
            if asio { "asio" } else { "java" },
            match kind {
                "output_devices" | "input_devices" if asio => "devices",
                "output_devices" => "output-devices",
                "input_devices" => "input-devices",
                "output_choices" if asio => "outputs",
                "output_choices" => "output-channels",
                "input_choices" => "inputs",
                "output_set" if asio => "output",
                "output_set" => "output-channel",
                "output_device" | "input_device" if asio => "device",
                "output_device" => "output-device",
                "input_device" => "input-device",
                "ref_output_set" if asio => "ref-output",
                "ref_output_set" => "ref-output-channel",
                "ref_output_choices" => "ref-output-channels",
                _ => kind,
            }
        )
    }
    fn cached_get(&mut self, key: &str, path: &str) -> Result<Value, RewError> {
        if let Some((until, value)) = self.cache.get(key)
            && *until > Instant::now()
        {
            return Ok(value.clone());
        }
        let data = self.get_timeout(path, self.timeout, true)?;
        if data.as_array().is_some_and(|v| !v.is_empty())
            || data.as_object().is_some_and(|v| !v.is_empty())
        {
            self.cache
                .insert(key.into(), (Instant::now() + LIST_CACHE, data.clone()));
        }
        Ok(data)
    }
    fn channel_names(&mut self, kind: &str) -> Result<Vec<String>, RewError> {
        let path = self.audio_path(&format!("{kind}_choices"));
        let data = self.cached_get(&format!("channels:{kind}"), &path)?;
        let data = data
            .get("channels")
            .or_else(|| data.get("values"))
            .unwrap_or(&data);
        Ok(data
            .as_array()
            .map(|v| {
                v.iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .filter(|name| !name.contains('+'))
                    .collect()
            })
            .unwrap_or_default())
    }
    pub fn output_channel_names(&mut self) -> Vec<String> {
        self.channel_names("output").unwrap_or_default()
    }
    pub fn input_channel_names(&mut self) -> Vec<String> {
        self.channel_names("input").unwrap_or_default()
    }
    fn devices(&mut self, kind: &str) -> Vec<DeviceInfo> {
        self.devices_result(kind).unwrap_or_default()
    }
    fn devices_result(&mut self, kind: &str) -> Result<Vec<DeviceInfo>, RewError> {
        let path = self.audio_path(&format!("{kind}_devices"));
        let mut data = self.cached_get(&format!("devices:{kind}"), &path)?;
        if data.is_object() {
            data = data
                .get("device")
                .or_else(|| data.get("name"))
                .map(|n| json!([n]))
                .unwrap_or(json!([]));
        }
        let Some(items) = data.as_array() else {
            return Err(RewError::schema(&path, "expected device list"));
        };
        let count = self
            .channel_names(kind)
            .map(|n| n.len().max(2))
            .unwrap_or(2);
        let java = !self.asio();
        let current = java.then(|| self.current_device(kind)).flatten();
        Ok(items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if let Some(name) = item.as_str() {
                    let known = !java || current.as_deref().is_none_or(|current| current == name);
                    Some(DeviceInfo {
                        id: name.into(),
                        name: name.into(),
                        channels: if known { count } else { 0 },
                        kind: kind.into(),
                        extra: if known {
                            json!({})
                        } else {
                            json!({"channels_unknown":true})
                        },
                    })
                } else if item.is_object() {
                    let name = item
                        .get("name")
                        .or_else(|| item.get("device"))
                        .or_else(|| item.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or(index.to_string());
                    let known = !java
                        || current
                            .as_deref()
                            .is_none_or(|current| current == name.as_str());
                    let channels = item
                        .get("channels")
                        .or_else(|| item.get("channelCount"))
                        .and_then(Value::as_u64)
                        .filter(|n| *n > 0)
                        .map(|n| n as usize)
                        .unwrap_or(count);
                    let mut extra = item.clone();
                    if !known && let Some(extra) = extra.as_object_mut() {
                        extra.insert("channels_unknown".into(), json!(true));
                    }
                    Some(DeviceInfo {
                        id: item["id"].as_str().unwrap_or(&name).into(),
                        name,
                        channels: if known { channels } else { 0 },
                        kind: kind.into(),
                        extra,
                    })
                } else {
                    None
                }
            })
            .collect())
    }
    /// Read-only connection facts. Unknown fields stay unknown; in particular
    /// API 0.9.4 has no Pro endpoint and must not be probed by starting a sweep.
    pub fn info(&mut self) -> InfoSnapshot {
        let version = self.version();
        if version.is_none() {
            return InfoSnapshot::default();
        }
        let blocking = self
            .get_timeout("/application/blocking", self.timeout, true)
            .ok()
            .and_then(|v| v.as_bool());
        let input_calibration_file = self.input_calibration_file();
        let driver = self.read_driver().ok();
        let input_channel_names = self.channel_names("input").ok();
        let input_devices = self.devices_result("input").ok();
        let output_devices = self.devices_result("output").ok();
        let sample_rate = self
            .get_timeout("/audio/samplerate", self.timeout, true)
            .ok()
            .and_then(|v| number(&v["value"]));
        let sweep = self
            .get_timeout("/measure/sweep/configuration", self.timeout, true)
            .ok()
            .filter(Value::is_object)
            .map(|v| SweepSnapshot {
                start_hz: number(&v["startFrequency"]),
                end_hz: number(&v["endFrequency"]),
                length: v["length"].as_str().map(str::to_owned),
            });
        let level_dbfs = self.measure_level_dbfs();
        InfoSnapshot {
            version,
            reachable: true,
            running: true,
            driver,
            pro: self.pro_state(),
            blocking,
            input_calibration_file,
            input_devices,
            output_devices,
            input_channel_names,
            sample_rate,
            sweep,
            level_dbfs,
        }
    }
    pub fn output_devices(&mut self) -> Vec<DeviceInfo> {
        self.devices("output")
    }
    pub fn input_devices(&mut self) -> Vec<DeviceInfo> {
        self.devices("input")
    }
    pub fn set_output_channel(&mut self, channel: impl ToString) -> Result<(), RewError> {
        let value = channel.to_string();
        let value = if let Ok(n) = value.parse::<u32>() {
            channel_name_for(&self.output_channel_names(), n).unwrap_or(value)
        } else {
            value
        };
        let asio = self.asio();
        let path = self.audio_path("output_set");
        self.post(
            &path,
            if asio {
                json!({"output":value})
            } else {
                json!({"channel":value})
            },
        )?;
        Ok(())
    }
    pub fn set_timing_ref_output(&mut self, channel: impl ToString) -> Result<(), RewError> {
        let mut value = channel.to_string();
        let asio = self.asio();
        if let Ok(n) = value.parse::<u32>() {
            let names = if asio {
                self.output_channel_names()
            } else {
                let path = self.audio_path("ref_output_choices");
                self.get(&path)
                    .ok()
                    .and_then(|v| {
                        v.as_array().map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                    })
                    .unwrap_or_default()
            };
            value = channel_name_for(&names, n).unwrap_or(value);
        }
        let path = self.audio_path("ref_output_set");
        self.post(
            &path,
            if asio {
                json!({"output":value})
            } else {
                json!({"channel":value})
            },
        )?;
        Ok(())
    }
    pub fn set_blocking(&mut self, value: bool) -> Result<(), RewError> {
        self.post("/application/blocking", json!(value))?;
        Ok(())
    }
    pub fn set_sample_rate(&mut self, rate: u32) -> Result<(), RewError> {
        self.post(
            "/audio/samplerate",
            json!({"value":f64::from(rate),"unit":"Hz"}),
        )?;
        Ok(())
    }
    pub fn set_sweep(
        &mut self,
        start: f64,
        end: f64,
        length: impl ToString,
    ) -> Result<(), RewError> {
        self.post("/measure/sweep/configuration",json!({"startFrequency":start.round_ties_even() as i32,"endFrequency":end.round_ties_even() as i32,"length":sweep_length_name(length)?}))?;
        Ok(())
    }
    pub fn set_level(&mut self, dbfs: f64) -> Result<(), RewError> {
        self.post("/measure/level", json!({"value":dbfs,"unit":"dBFS"}))?;
        Ok(())
    }
    pub fn set_repetitions(&mut self, n: u32) -> Result<(), RewError> {
        if self
            .get("/measure/sweep/repetitions")
            .ok()
            .as_ref()
            .and_then(number)
            == Some(f64::from(n))
        {
            return Ok(());
        }
        self.post("/measure/sweep/repetitions", json!(n))?;
        Ok(())
    }
    pub fn set_timing_reference(&mut self, reference: &str) -> Result<(), RewError> {
        let text = reference.trim();
        let name = TIMING_REFERENCES
            .into_iter()
            .find(|s| s.eq_ignore_ascii_case(text))
            .unwrap_or(if text.to_lowercase().starts_with("acoustic") {
                "Acoustic"
            } else {
                text
            });
        self.post("/measure/timing/reference", json!(name))?;
        Ok(())
    }
    pub fn set_naming(&mut self, title: &str) -> Result<(), RewError> {
        self.post(
            "/measure/naming",
            json!({"title":title,"namingOption":"Use as entered","prefixMeasNameWithOutput":false}),
        )?;
        Ok(())
    }
    pub fn set_notes(&mut self, notes: &str) -> Result<(), RewError> {
        self.post("/measure/notes", json!(notes))?;
        Ok(())
    }
    pub fn set_sequential_channels(&mut self, channels: &[String]) -> Result<(), RewError> {
        self.post("/measure/sequential-channels", json!({"channels":channels}))?;
        Ok(())
    }
    pub fn open_plan_devices(&mut self, plan: &MeasurementPlan) -> Result<(), RewError> {
        for (kind, name) in [
            ("output", plan.output_device.as_str()),
            ("input", plan.input_device.as_str()),
        ] {
            if name.is_empty() || !self.devices(kind).iter().any(|device| device.name == name) {
                continue;
            }
            if self.current_device(kind).as_deref() != Some(name) {
                self.set_device(kind, name)?;
            }
        }
        Ok(())
    }
    pub fn configure(&mut self, plan: &MeasurementPlan) -> Result<(), RewError> {
        let previous = self.measuring;
        self.measuring = true;
        let result = (|| {
            self.set_blocking(true)?;
            self.open_plan_devices(plan)?;
            self.set_sample_rate(plan.sample_rate)?;
            self.set_sweep(plan.start_frequency, plan.end_frequency, &plan.sweep_length)?;
            self.set_level(plan.sweep_level_db)?;
            if plan.repetitions > 1 {
                self.set_timing_reference(&plan.timing_reference)?;
                self.set_repetitions(plan.repetitions)?;
            } else {
                self.set_repetitions(plan.repetitions)?;
                self.set_timing_reference(&plan.timing_reference)?;
            }
            if let Some(output) = plan.timing_ref_output
                && !plan.timing_reference.trim().eq_ignore_ascii_case("none")
            {
                self.set_timing_ref_output(output)?;
            }
            Ok(())
        })();
        self.measuring = previous;
        result
    }
    pub fn measurements_map(&mut self) -> Result<BTreeMap<String, Value>, RewError> {
        let data = self.get_timeout(
            "/measurements",
            self.timeout.max(Duration::from_secs(10)),
            false,
        )?;
        Ok(match data {
            Value::Object(m) => m.into_iter().filter(|(_, v)| v.is_object()).collect(),
            Value::Array(a) => a
                .into_iter()
                .enumerate()
                .filter(|(_, v)| v.is_object())
                .map(|(i, v)| ((i + 1).to_string(), v))
                .collect(),
            _ => BTreeMap::new(),
        })
    }
    pub fn measurements(&mut self) -> Result<Vec<Value>, RewError> {
        let mut items: Vec<_> = self.measurements_map()?.into_iter().collect();
        items.sort_by(|a, b| index_cmp(&a.0, &b.0));
        Ok(items.into_iter().map(|(_, v)| v).collect())
    }
    pub fn measurement_ids(&mut self) -> Result<BTreeSet<String>, RewError> {
        Ok(self
            .measurements()?
            .iter()
            .filter_map(measurement_id)
            .collect())
    }
    pub fn start_sweep(&mut self) -> Result<Value, RewError> {
        self.post_timeout(
            "/measure/command",
            json!({"command":"SPL"}),
            self.measure_timeout,
        )
    }
    pub fn abort(&mut self) {
        let _ = self.post("/measure/command", json!({"command":"Cancel"}));
    }
    pub fn impulse_response(&mut self, uuid: &str) -> Result<ImpulseResponse, RewError> {
        let path = format!("/measurements/{}/impulse-response", segment(uuid));
        let mut request = Request::new(
            "GET",
            &path,
            None,
            self.timeout.max(Duration::from_secs(15)),
        );
        request.query.push(("normalised".into(), "false".into()));
        let data = self.request(request, false)?;
        let encoded = data["data"]
            .as_str()
            .ok_or_else(|| RewError::schema(&path, "missing data"))?;
        let rate = data.get("sampleRate").and_then(number).unwrap_or(48000.0);
        if rate <= 0.0 || rate > f64::from(u32::MAX) {
            return Err(RewError::schema(&path, "invalid sample rate"));
        }
        Ok(ImpulseResponse {
            ir: b64_be_f32(encoded)?,
            sample_rate: rate as u32,
            t0_s: data.get("startTime").and_then(number).unwrap_or(0.0),
        })
    }
    pub fn get_ir(&mut self, uuid: &str) -> Result<(Vec<f32>, u32), RewError> {
        let ir = self.impulse_response(uuid)?;
        Ok((ir.ir, ir.sample_rate))
    }
    pub fn spl_offset_db(&mut self, uuid: &str) -> Option<f64> {
        self.get_timeout(
            &format!("/measurements/{}", segment(uuid)),
            self.timeout.min(Duration::from_secs(5)),
            false,
        )
        .ok()
        .and_then(|d| number(&d["splOffsetdB"]))
    }
    pub fn snr_db(&mut self, uuid: &str) -> Option<f64> {
        self.get_timeout(
            &format!("/measurements/{}", segment(uuid)),
            self.timeout.min(Duration::from_secs(5)),
            false,
        )
        .ok()
        .and_then(|d| number(&d["signalToNoisedB"]))
    }
    pub fn measure_level_dbfs(&mut self) -> Option<f64> {
        self.get_timeout(
            "/measure/level",
            self.timeout.min(Duration::from_secs(3)),
            true,
        )
        .ok()
        .and_then(|d| number(&d["value"]))
    }
    pub fn input_calibration_file(&mut self) -> Option<String> {
        let data = self
            .get_timeout(
                "/audio/input-cal",
                self.timeout.min(Duration::from_secs(3)),
                true,
            )
            .ok()?;
        if data["separateCalFileForEachInput"]
            .as_bool()
            .unwrap_or(false)
        {
            return None;
        }
        let cal = data["calDataAllInputs"].as_object()?;
        Some(
            cal.get("calFilePath")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
        )
    }
    pub fn spl(&mut self, meter: u32) -> Option<f64> {
        self.get_timeout(
            &format!("/spl-meter/{meter}/levels"),
            self.timeout.min(Duration::from_secs(3)),
            false,
        )
        .ok()
        .and_then(|d| number(&d["spl"]).or_else(|| number(&d["SPL"])))
        .filter(|n| *n > -179.0)
    }
    pub fn sweep_wait_s(plan: &MeasurementPlan) -> f64 {
        sweep_length_samples(&plan.sweep_length)
            .map(|n| n as f64 / f64::from(plan.sample_rate.max(1)))
            .unwrap_or(4194304.0 / 48000.0)
            * f64::from(plan.repetitions.max(1))
            + 30.0
    }
    pub fn wait_for_new_measurement(
        &mut self,
        before: &BTreeSet<String>,
        timeout: Duration,
        cancel: &CancelToken,
    ) -> Result<Option<String>, RewError> {
        let start = Instant::now();
        while start.elapsed() < timeout.max(Duration::from_secs(1)) {
            if cancel.is_cancelled() {
                return Ok(None);
            }
            let items = self.measurements_map()?;
            if let Some((_, id)) = items
                .into_iter()
                .filter_map(|(k, v)| {
                    measurement_id(&v)
                        .filter(|id| !before.contains(id))
                        .map(|id| (k, id))
                })
                .max_by(|a, b| index_cmp(&a.0, &b.0))
            {
                return Ok(Some(id));
            }
            if cancel.wait(self.poll_interval) {
                return Ok(None);
            }
        }
        Ok(None)
    }
    fn ensure_callback(&mut self) -> Result<&mut CallbackServer, RewError> {
        let server = self.callback.get_or_insert_with(CallbackServer::default);
        server.start()?;
        Ok(server)
    }
    /// Prepare the callback address without registering a REW subscription. Useful
    /// for exact recorded scripts, since callback tokens are deliberately random.
    pub fn callback_url(&mut self, path: &str) -> Result<String, RewError> {
        Ok(self.ensure_callback()?.url_for(path))
    }
    pub fn start(&mut self, on_level: LevelCallback) -> Result<(), RewError> {
        let index = self.level_index;
        let url = self.ensure_callback()?.route(
            "/levels",
            Arc::new(move |v| emit(&Some(on_level.clone()), level_payload(&v, index))),
        );
        self.level_url = Some(url.clone());
        self.post(
            "/input-levels/subscribe",
            json!({"url":url,"parameters":{"unit":"dBFS","timeSpanSeconds":0.25}}),
        )?;
        self.post("/input-levels/command", json!({"command":"Start"}))?;
        Ok(())
    }
    pub fn last_levels(&mut self) -> Value {
        self.get_timeout(
            "/input-levels/last-levels",
            self.timeout.min(Duration::from_secs(3)),
            true,
        )
        .ok()
        .filter(Value::is_object)
        .unwrap_or(json!({}))
    }
    pub fn stop(&mut self) {
        let _ = self.post("/input-levels/command", json!({"command":"Stop"}));
        if let Some(url) = self.level_url.take() {
            let _ = self.post("/input-levels/unsubscribe", json!({"url":url}));
        }
        if let Some(server) = &self.callback {
            server.unroute("/levels");
        }
        if self.progress_url.is_none() {
            self.callback.take();
        }
    }
    pub fn play_pink(&mut self, role: &str, output: u32, level_db: f64) -> Result<(), RewError> {
        self.set_output_channel(output)?;
        let sub = split_role(role).0.to_uppercase().starts_with("SW");
        self.post(
            "/generator/signal",
            json!({"signal":if sub {"sine"} else {"pinknoise"}}),
        )?;
        if sub {
            self.post(
                "/generator/signals/sine/configuration",
                json!({"frequency":50.0}),
            )?;
        }
        self.post("/generator/level", json!({"value":level_db,"unit":"dBFS"}))?;
        self.post("/generator/command", json!({"command":"Play"}))?;
        Ok(())
    }
    pub fn stop_generator(&mut self) {
        let _ = self.post("/generator/command", json!({"command":"Stop"}));
    }
    pub fn subscribe_progress(
        &mut self,
        handler: Arc<dyn Fn(Value) + Send + Sync>,
    ) -> Result<String, RewError> {
        let url = self.ensure_callback()?.route("/measure-progress", handler);
        self.post("/measure/subscribe", json!({"url":url,"parameters":{}}))?;
        self.progress_url = Some(url.clone());
        Ok(url)
    }
    pub fn unsubscribe_progress(&mut self) {
        if let Some(url) = self.progress_url.take() {
            let _ = self.post("/measure/unsubscribe", json!({"url":url}));
        }
        if let Some(server) = &self.callback {
            server.unroute("/measure-progress");
        }
    }
    pub fn process_result(&mut self) -> Value {
        self.get("/measurements/process-result")
            .ok()
            .filter(Value::is_object)
            .unwrap_or(json!({}))
    }
    /// Explicit close only: never send generator Stop from Drop during a sweep.
    pub fn close(&mut self) {
        self.stop_generator();
        self.unsubscribe_progress();
        self.stop();
        self.callback.take();
    }
    pub fn run_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: u32,
        on_progress: Option<ProgressCallback>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, RewError> {
        self.measuring = true;
        let current = Arc::new(Mutex::new(String::new()));
        let mut own = false;
        let result = (|| {
            self.configure(plan)?;
            if on_progress.is_some() && self.progress_url.is_none() {
                let current = current.clone();
                let callback = on_progress.clone();
                own = self
                    .subscribe_progress(Arc::new(move |body| {
                        let (percent, message) = parse_progress(&body);
                        if let Some(percent) = percent {
                            let role = current.lock().unwrap();
                            if !role.is_empty() {
                                emit(
                                    &callback,
                                    progress(
                                        &role,
                                        pos,
                                        "running",
                                        percent.clamp(0.0, 100.0) * 0.9,
                                        &message,
                                    ),
                                );
                            }
                        }
                    }))
                    .is_ok();
            }
            let mut results = vec![];
            for role in &plan.roles {
                *current.lock().unwrap() = role.clone();
                if cancel.is_cancelled() {
                    current.lock().unwrap().clear();
                    emit(
                        &on_progress,
                        progress(role, pos, "cancelled", 0.0, "측정을 취소했습니다"),
                    );
                    break;
                }
                let name = plan.name_for(role, pos);
                emit(
                    &on_progress,
                    progress(
                        role,
                        pos,
                        "sweep",
                        0.0,
                        &format!("{role} 스윕 (위치 {pos})"),
                    ),
                );
                self.set_output_channel(plan.output_for(role))?;
                self.set_naming(&name)?;
                let (base, band) = split_role(role);
                let suffix = band.map(|b| format!("/{b}")).unwrap_or_default();
                self.set_notes(&format!(
                    "SECS {base}{suffix} P{pos} ({} timing)",
                    plan.timing_reference
                ))?;
                let before = self.measurement_ids()?;
                if let Err(e) = self.start_sweep() {
                    if matches!(e, RewError::ProRequired { .. }) || !cancel.is_cancelled() {
                        return Err(e);
                    }
                    current.lock().unwrap().clear();
                    emit(
                        &on_progress,
                        progress(role, pos, "cancelled", 0.0, "측정을 취소했습니다"),
                    );
                    break;
                }
                let id = if cancel.is_cancelled() {
                    None
                } else {
                    self.wait_for_new_measurement(
                        &before,
                        Duration::from_secs_f64(Self::sweep_wait_s(plan)),
                        cancel,
                    )?
                };
                current.lock().unwrap().clear();
                if cancel.is_cancelled() {
                    self.abort();
                    emit(
                        &on_progress,
                        progress(role, pos, "cancelled", 0.0, "측정을 취소했습니다"),
                    );
                    break;
                }
                let id = id.ok_or_else(|| {
                    RewError::schema("/measurements", format!("no new measurement: {name}"))
                })?;
                emit(
                    &on_progress,
                    progress(
                        role,
                        pos,
                        "process",
                        90.0,
                        &format!("{role} 임펄스 응답 수신"),
                    ),
                );
                let ir = self.impulse_response(&id)?;
                results.push(MeasurementResult {
                    role: role.clone(),
                    pos,
                    uuid: Some(id.clone()),
                    ir: ir.ir,
                    sample_rate: ir.sample_rate,
                    name,
                    spl_db: self.spl(1),
                    t0_s: ir.t0_s,
                    snr_db: self.snr_db(&id),
                    spl_offset_db: self.spl_offset_db(&id),
                });
                emit(
                    &on_progress,
                    progress(role, pos, "done", 100.0, &format!("{role} 완료")),
                );
            }
            Ok(results)
        })();
        current.lock().unwrap().clear();
        if own {
            self.unsubscribe_progress();
        }
        self.measuring = false;
        result
    }
}
fn measurement_id(value: &Value) -> Option<String> {
    value
        .get("uuid")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn index_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<i64>(), b.parse::<i64>()) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        _ => a.cmp(b),
    }
}
fn segment(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
