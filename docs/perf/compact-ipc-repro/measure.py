#!/usr/bin/env python3
"""Serialize a fixed synthetic result through a copied, actual secs-types crate.
Never edits the supplied repository. Large JSON files belong only in --out.
"""
from pathlib import Path
import argparse, hashlib, io, json, os, re, shutil, subprocess, tempfile, zipfile

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--repo',type=Path,default=Path.cwd())
parser.add_argument('--revision',help='Git revision to snapshot; omit for current working-tree types')
parser.add_argument('--phase',choices=['baseline','after'],required=True)
parser.add_argument('--out',type=Path,required=True)
parser.add_argument('--target-dir',type=Path)
parser.add_argument('--offline',action='store_true')
parser.add_argument('--node',action='store_true',help='Also run the optional Node/V8 microbenchmarks')
args=parser.parse_args();repo=args.repo.resolve();out=args.out.resolve();out.mkdir(parents=True,exist_ok=True)
if out.is_relative_to(repo):raise SystemExit('--out must be outside the repository to avoid committing large synthetic payloads')
source_description=args.revision or ('working-tree based on '+subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip())
with tempfile.TemporaryDirectory(prefix='secs-ipc-types-') as temporary:
    workspace=Path(temporary)
    if workspace.resolve().parent != Path(tempfile.gettempdir()).resolve() or not workspace.name.startswith("secs-ipc-types-"):raise ValueError("Unexpected temporary workspace")
    if args.revision:
        archive=subprocess.check_output(['git','-C',str(repo),'archive','--format=zip',args.revision,'Cargo.toml','Cargo.lock','rust-toolchain.toml','crates/secs-types'])
        with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
            for item in zipped.infolist():
                if not (workspace/item.filename).resolve().is_relative_to(workspace.resolve()):raise ValueError('Unsafe archive path')
            zipped.extractall(workspace)
    else:
        shutil.copytree(repo/'crates/secs-types',workspace/'crates/secs-types')
        for name in ['Cargo.toml','Cargo.lock','rust-toolchain.toml']:shutil.copyfile(repo/name,workspace/name)
    hashes={p.relative_to(workspace).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in (workspace/'crates/secs-types').rglob('*') if p.is_file()}
    manifest=workspace/'Cargo.toml';text=manifest.read_text(encoding='utf-8')
    text,count=re.subn(r'(?m)^members\s*=\s*\[[^\n]*\]','members = ["crates/secs-types", "harness"]',text,count=1)
    if count!=1:raise ValueError('Expected the repository workspace member list')
    manifest.write_text(text,encoding='utf-8')
    harness=workspace/'harness';(harness/'src').mkdir(parents=True)
    (harness/'Cargo.toml').write_text('[package]\nname="ipc-payload-probe"\nversion="0.0.0"\nedition="2024"\npublish=false\n[lints]\nworkspace=true\n[dependencies]\nsecs-types.workspace=true\nserde.workspace=true\nserde_json.workspace=true\n',encoding='utf-8')
    helper=Path(__file__).resolve().parent/f'{args.phase}.rs'
    shutil.copyfile(helper,harness/'src/main.rs')
    target=(args.target_dir or out/'target').resolve()
    env=os.environ.copy();env['CARGO_TARGET_DIR']=str(target);env['IPC_PROBE_SOURCE']=source_description
    command=['cargo','build','--release','-p','ipc-payload-probe']
    if args.offline:command.append('--offline')
    with (out/'build.log').open('w',encoding='utf-8') as log:subprocess.run(command,cwd=workspace,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    binary=target/'release'/('ipc-payload-probe.exe' if os.name=='nt' else 'ipc-payload-probe')
    copied_binary=out/binary.name;shutil.copy2(binary,copied_binary)
    with (out/'run.log').open('w',encoding='utf-8') as log:subprocess.run([str(copied_binary),str(out)],cwd=workspace,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    toolchain=subprocess.check_output(['rustc','--version','--verbose'],cwd=workspace,text=True)
    metadata={'phase':args.phase,'source':source_description,'type_source_sha256':hashes,'helper_sha256':hashlib.sha256(helper.read_bytes()).hexdigest(),'toolchain':toolchain,'notes':['Only workspace membership and the external harness are changed in the temporary copy; secs-types source is copied verbatim.','No engine solver, real measurement, IPC transport, WebView rendering, or total application memory is measured.','Both phases generate the same full coefficient data; the after harness keeps it alive while serializing only screen DTOs.']}
    (out/'provenance.json').write_text(json.dumps(metadata,indent=2)+'\n',encoding='utf-8')
if args.node:
    script=Path(__file__).resolve().parent/'node-probe.mjs'
    for mode in ['parse','history']:
        subprocess.run(['node','--expose-gc','--max-old-space-size=2048',str(script),mode,str(out),args.phase,str(out/f'node-{mode}.json')],check=True)
metrics=json.loads((out/'rust-metrics.json').read_text(encoding='utf-8'))
print(json.dumps({'phase':args.phase,'source':source_description,'rows':[{'payload':r['name'],'json_bytes':r['json_bytes']} for r in metrics['rows']]},indent=2))
