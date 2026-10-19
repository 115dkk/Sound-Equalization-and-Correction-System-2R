#![forbid(unsafe_code)]
use secs_types::{calibration::Mode, chain::{BandType,MainChannel,SubChannel,SignalChain,MimoFilter,MimoPath,MimoPaths}, compute::{ComputePreference,ComputeReason,ComputeReport}, ipc::{CalibrationSummary,ChannelScore,MetricsSummary,VerdictSummary,Notice,MimoSummary,ResultsSummary,JobEvent,JobResult,JobId,JobKind,JobStatus,JobSnapshot}};
use std::{collections::BTreeMap,path::Path,time::Instant,hint::black_box};
const MAINS: [&str;11]=["L","R","C","Ls","Rs","Lb","Rb","Tfl","Tfr","Tbl","Tbr"];
const SEED:u64=0x5EC5_2026_1020;
fn coefficient(stream:usize,tap:usize)->f64 {
    let mut z=SEED.wrapping_add((stream as u64).wrapping_mul(0xD1342543DE82EF95)).wrapping_add((tap as u64).wrapping_mul(0x9E3779B97F4A7C15));
    z=(z^(z>>30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z=(z^(z>>27)).wrapping_mul(0x94D049BB133111EB);
    z^=z>>31;
    f64::from((z>>32) as u32 as i32)/4294967296.0/(256.0+tap as f64)
}
fn taps(stream:usize,count:usize)->Vec<f64>{(0..count).map(|i|coefficient(stream,i)).collect()}
fn peqs(n:usize)->Vec<(f64,f64,f64)>{(0..n).map(|i|(50.0+25.0*i as f64,1.2,-0.25*i as f64)).collect()}
fn metric(after:bool)->MetricsSummary {
    MetricsSummary{level:75.0,in1_20:if after{78.0}else{45.0},in2_20:90.0,in1_200:80.0,in2_200:92.0,in1_2000:82.0,in2_2000:94.0,turns:1.2,gd_20:15.0,gd_100:8.0,gd_500:3.0,gd24_abs:20.0,gd24_abs_hz:38.0,gd24_max:20.0,gd24_span:18.0,gd_30:12.0,pre_db:-40.0}
}
fn signal_chain()->SignalChain { SignalChain{sample_rate:48000,mains:MAINS.iter().enumerate().map(|(i,r)|MainChannel{role:(*r).into(),input_peq:peqs(9),fir:taps(100+i,2048),gain_db:-(i as f64)*0.1,delay_ms:2.0+0.25*i as f64,xo_hz:Some(80.0),top_peq:peqs(2),align_delay_ms:0.2*i as f64}).collect(),
        subs:["SW1","SW2"].iter().map(|r|SubChannel{role:(*r).into(),gain_db:0.0,delay_ms:0.0,polarity:1,allpass:vec![(60.0,1.0)],hp_hz:0.0,hp_type:BandType::Lr24,lp_hz:0.0,lp_type:BandType::Lr24,peq:peqs(6),mode:"auto".into()}).collect(),sub_bus_peq:peqs(9),main_extra_ms:0.0,lift_ms:8.0,latency_ms:8.0} }
fn calibration(chain:&SignalChain)->CalibrationSummary {
    CalibrationSummary{stale:false,stale_reason:None,mode:Mode::Mixed,ultra_ms:200.0,
      verdict:VerdictSummary{within_1db_before_pct:45.0,within_1db_after_pct:78.0,low_band_time_spread_before_ms:25.0,low_band_time_spread_after_ms:12.0,added_delay_ms:8.0,position_spread_db:2.0},
      scorecard:MAINS.iter().map(|r|ChannelScore{role:(*r).into(),before:metric(false),after:metric(true)}).collect(),latency_ms:8.0,notes:Vec::new(),
      main_roles:chain.mains.iter().map(|c|c.role.clone()).collect(),sub_roles:chain.subs.iter().map(|c|c.role.clone()).collect()}
}
fn mimo_paths()->MimoPaths {
    let supports:Vec<_>=MAINS.iter().copied().chain(["SW1","SW2"]).collect();
    let mut paths=Vec::new();
    for &input in &MAINS {for &support in &supports {if input!=support {let i=paths.len(); paths.push(MimoPath{input:input.into(),support:support.into(),filter:MimoFilter::Fir{taps_48k:taps(i,8000)},low_hz:if support.starts_with("SW"){20.0}else{40.0},delay_ms:(i%5) as f64});}}}
    assert_eq!(paths.len(),132);
    MimoPaths{fsiso:150.0,paths}
}
fn mimo(paths:&MimoPaths)->MimoSummary {
    MimoSummary{compute:ComputeReport{requested:ComputePreference::Auto,cpu_systems:11,reason:Some(ComputeReason::SmallProblem),..ComputeReport::default()},stale:false,kind:"fir".into(),fsiso:150.0,support_count:132,fir_taps_total:1_056_000,
      notes:vec![Notice{code:"mimo_fir_budget".into(),params:BTreeMap::from([("support_count".into(),"132".into()),("taps_total".into(),"1056000".into())])}],input_count:paths.paths.iter().map(|p|p.input.as_str()).collect::<std::collections::BTreeSet<_>>().len()}
}
fn record<T:serde::Serialize>(root:&Path,name:&str,value:&T)->serde_json::Value {
    black_box(serde_json::to_vec(value).unwrap());
    let mut times=Vec::new();
    for _ in 0..7 {let start=Instant::now();let b=black_box(serde_json::to_vec(value).unwrap());times.push(start.elapsed().as_secs_f64()*1000.0);drop(b);}
    let mut sorted=times.clone();sorted.sort_by(f64::total_cmp);
    let bytes=serde_json::to_vec(value).unwrap();std::fs::write(root.join(format!("{name}.json")),&bytes).unwrap();
    serde_json::json!({"name":name,"json_bytes":bytes.len(),"serialize_samples_ms":times,"serialize_median_ms":sorted[sorted.len()/2]})
}
fn main(){
    let folder=std::env::args().nth(1).expect("output folder");let root=Path::new(&folder);std::fs::create_dir_all(root).unwrap();
    let full_chain=signal_chain();let full_paths=mimo_paths();let calibration=calibration(&full_chain);let mimo=mimo(&full_paths);
    let mut rows=Vec::new();
    rows.push(record(root,"calibration-summary",&calibration));rows.push(record(root,"mimo-summary",&mimo));
    let results=ResultsSummary{calibration:Some(calibration),mimo:Some(mimo.clone())};rows.push(record(root,"results-summary",&results));
    let event=JobEvent::Finished{job:JobId(42),status:JobStatus::Succeeded,result:Some(Box::new(JobResult::Mimo)),error:None};rows.push(record(root,"mimo-finished",&event));
    let event=JobEvent::Finished{job:JobId(43),status:JobStatus::Succeeded,result:Some(Box::new(JobResult::Calibration)),error:None};rows.push(record(root,"calibration-finished",&event));
    let snapshot=JobSnapshot{job:JobId(42),kind:JobKind::Mimo,status:JobStatus::Succeeded,cancellable:false,progress:None,error:None};rows.push(record(root,"mimo-job-snapshot",&snapshot));
    let metrics=serde_json::json!({"source_commit":std::env::var("IPC_PROBE_SOURCE").ok(),"implementation":"actual secs-types Rust serde","mimo_paths":132,"taps_per_mimo_path":8000,"mimo_coefficients":1_056_000,"mimo_coefficient_f64_bytes":8_448_000,"calibration_mains":11,"taps_per_calibration_main":2048,"calibration_coefficients":22_528,"calibration_coefficient_f64_bytes":180_224,"coefficient_generator":"SplitMix64-style integer mixing, SEED=0x5EC520261020; signed upper32/2^32/(256+tap); stream route_index or 100+main_index; see harness/src/main.rs","rows":rows});
    std::fs::write(root.join("rust-metrics.json"),serde_json::to_vec_pretty(&metrics).unwrap()).unwrap();black_box((&full_chain,&full_paths));println!("{metrics}");
}
