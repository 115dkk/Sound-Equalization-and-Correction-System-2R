import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import v8 from 'node:v8';
import { performance } from 'node:perf_hooks';

if (!global.gc) throw new Error('Run Node with --expose-gc');
const [mode, folder, phase, output] = process.argv.slice(2);
const forceGc = () => { for (let i=0;i<3;i++) global.gc(); };
const environment = { node:process.version,v8:process.versions.v8,platform:process.platform,arch:process.arch,cpu:os.cpus()[0]?.model,heap_size_limit:v8.getHeapStatistics().heap_size_limit };
const countCoefficients = (v) => {
  if (!v || typeof v!=='object') return 0;
  let n=0;
  for (const [k,x] of Object.entries(v)) {
    if ((k==='taps_48k'||k==='fir') && Array.isArray(x) && x.every(y=>typeof y==='number')) n+=x.length;
    else if (x && typeof x==='object') n+=countCoefficients(x);
  }
  return n;
};
let report;
if (mode==='parse') {
  const rows=[];
  for(const name of ['calibration-summary','mimo-summary','results-summary','mimo-finished','calibration-finished','mimo-job-snapshot']) {
    const raw=fs.readFileSync(path.join(folder,`${name}.json`),'utf8');
    JSON.parse(raw);forceGc();
    const samples=[];
    for(let sample=0;sample<11;sample++) {
      forceGc();const start=performance.now();let parsed=JSON.parse(raw);samples.push(performance.now()-start);
      if(parsed===null) throw new Error('Unexpected null payload');parsed=null;
    }
    forceGc();const before=process.memoryUsage().heapUsed;
    let held=JSON.parse(raw);forceGc();const after=process.memoryUsage().heapUsed;
    const coefficients=countCoefficients(held);
    rows.push({payload:name,json_bytes:Buffer.byteLength(raw),parse_samples_ms:samples,parse_median_ms:[...samples].sort((a,b)=>a-b)[5],single_parsed_retained_heap_bytes:after-before,coefficient_count:coefficients});
    held=null;forceGc();
  }
  report={environment,phase,protocol:'One JSON.parse warmup, 11 timed parses; explicit GC outside timers. Heap delta is one retained parsed object; source JSON string is already resident and excluded from the delta. This is Node V8, not WebView IPC or rendering latency. Host background work was not excluded; timings are illustrative microbenchmarks, not headline claims.',rows};
} else if(mode==='history') {
  const eventRaw=fs.readFileSync(path.join(folder,'mimo-finished.json'),'utf8');
  const resultsRaw=fs.readFileSync(path.join(folder,'results-summary.json'),'utf8');
  JSON.parse(eventRaw);JSON.parse(resultsRaw);forceGc();
  const baselineHeap=process.memoryUsage().heapUsed;
  const active=[{job:100001,kind:'measure',status:'running',cancellable:true,progress:null,error:null},{job:100002,kind:'status_watch',status:'running',cancellable:true,progress:null,error:null}];
  let jobs=[...active];let current=JSON.parse(resultsRaw);let parseTotal=0;const rows=[];
  const capture=(completed)=>{forceGc();rows.push({completed_jobs:completed,retained_terminal_jobs:jobs.length-active.length,retained_active_jobs:active.length,retained_heap_delta_bytes:process.memoryUsage().heapUsed-baselineHeap,accumulated_json_parse_ms:parseTotal});};
  capture(0);
  for(let i=1;i<=40;i++) {
    const start=performance.now();const event=JSON.parse(eventRaw);current=JSON.parse(resultsRaw);parseTotal+=performance.now()-start;
    const row={job:i,kind:'mimo',status:event.status,cancellable:false,progress:null,error:event.error};
    if(phase==='baseline') row.result=event.result;
    jobs.push(row);
    if(phase!=='baseline') {const terminal=jobs.filter(j=>j.status==='succeeded').slice(-32);jobs=[...active,...terminal];}
    if([1,8,16,32,40].includes(i)) capture(i);
  }
  // Keep both modeled stores strongly reachable through the final observation.
  if(!current || jobs.length<active.length) throw new Error('Model lost current state');
  report={environment,phase,protocol:'Synthetic host.jobs plus a separately JSON-decoded current ResultsSummary. Each completed MIMO job parses Finished and reloads ResultsSummary. Baseline retains event.result for every terminal job, matching baseline state/session.ts. After retains metadata for the latest 32 terminal jobs plus all active jobs and one compact current summary. Source JSON strings excluded from retained heap delta. Not a React/WebView/process-RSS or total Rust-service-memory measurement.',event_json_bytes:Buffer.byteLength(eventRaw),results_json_bytes:Buffer.byteLength(resultsRaw),rows};
} else throw new Error('mode must be parse or history');
fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify({phase,mode,rows:report.rows.map(r=>({payload:r.payload,completed_jobs:r.completed_jobs,parse_median_ms:r.parse_median_ms,retained_heap_bytes:r.single_parsed_retained_heap_bytes??r.retained_heap_delta_bytes,retained_terminal_jobs:r.retained_terminal_jobs,coefficient_count:r.coefficient_count}))}));
