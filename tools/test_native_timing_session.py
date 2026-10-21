#!/usr/bin/env python3
"""Actual Session open/config/bind/export paths, in separate owned host processes."""
import argparse,hashlib,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def main():
    p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);p.add_argument('--artifacts',required=True,type=Path);a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
    artifacts={};parents=set()
    for line in a.artifacts.read_text().splitlines():
        row=json.loads(line)
        if row.get('reason')!='compiler-artifact':continue
        for path in map(Path,row['filenames']):
            parents.add(path.parent)
            if path.suffix=='.rlib':artifacts[row['target']['name']]=path
    code='''#![allow(dead_code)]
#[path="MODULE"]mod native_timing;
// Bind the production Session filesystem calls to this case's owned cwd.
mod preferences {pub fn path(name: &str)->std::path::PathBuf {std::path::Path::new("private").join(name)}}
use native_timing::*;
fn main(){
 let want=std::env::args().nth(1).unwrap();
 let mut session=Session::open().unwrap();
 let pcm=std::sync::OnceLock::new();let music=std::sync::OnceLock::new();
 session.with(|r|r.bind_raw_counters(Some(&pcm),Some(&music)));
 let raw=want=="enabled-audio"||want=="enabled-producer";
 session.with(|r|assert_eq!(r.producer_requested,want=="enabled-producer","producer config gate"));
 assert_eq!(pcm.get().is_some(),raw);assert_eq!(music.get().is_some(),raw);
 if raw {pcm.get().unwrap().event();music.get().unwrap().event();}
 session.export_after_viewer().unwrap();
 if raw {
  let r=session.recorder.as_ref().unwrap();let data=r.raw_counters.as_ref().unwrap();
  assert_eq!(data.snapshots.len(),4,"AfterViewer field reads missing");assert_eq!(data.snapshots[2].value,1);assert_eq!(data.snapshots[3].value,1);
  assert_eq!(data.snapshots[2].tag,audio_counters::Tag::AfterViewer);
 }
 if want.starts_with("enabled") {assert!(Session::open().is_err(),"second Session must fail before buffer allocation");}
 println!("actual Session open/config/bind/export PASS {}",want);
}
'''.replace('MODULE',str(ROOT/'src/native_timing/mod.rs'))
    (out/'probe.rs').write_text(code);cmd=[str(Path.home()/'.cargo/bin/rustc'),'--edition=2021','--cfg','feature="native_timing"',str(out/'probe.rs'),'-o',str(out/'probe')]
    for parent in sorted(parents):cmd+=['-L','dependency='+str(parent)]
    for name in ('serde','serde_json','anyhow','sha2'):
        cmd+=['--extern',name+'='+str(artifacts[name])]
        if artifacts[name].with_suffix('.rmeta').exists():cmd+=['--extern',name+'='+str(artifacts[name].with_suffix('.rmeta'))]
    result=subprocess.run(cmd,capture_output=True,text=True);(out/'compile.log').write_text(result.stdout+result.stderr);commands=[dict(argv=cmd,exit=result.returncode)];assert result.returncode==0,result.stderr
    for name,cfg in [('missing',None),('disabled',dict(enabled=False,audio_raw_counters=True,audio_producer_capture=True)),('enabled-default',dict(enabled=True)),('enabled-audio',dict(enabled=True,audio_raw_counters=True)),('enabled-producer',dict(enabled=True,audio_raw_counters=True,audio_producer_capture=True))]:
        case=out/name;case.mkdir();(case/'private').mkdir()
        if cfg is not None:(case/'private/native-timing-config.json').write_text(json.dumps(cfg))
        cmd=[str(out/'probe'),name];result=subprocess.run(cmd,cwd=case,capture_output=True,text=True);(case/'run.log').write_text(result.stdout+result.stderr);commands.append(dict(argv=cmd,cwd=str(case),exit=result.returncode));(out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n');assert result.returncode==0,result.stdout+result.stderr;print(result.stdout,end='')
    (out/'source.json').write_text(json.dumps({'module_sha256':hashlib.sha256((ROOT/'src/native_timing/mod.rs').read_bytes()).hexdigest(),'scope':'actual Session, synthetic counter slots; actual Audio binding covered by in-crate tests'},indent=2)+'\n')
if __name__=='__main__':main()
