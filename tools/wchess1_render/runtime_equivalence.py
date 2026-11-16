"""Verify the recorded revision differences are exactly known cfg(test)-only edits."""
import json,subprocess
from pathlib import Path
P=Path(__file__).resolve().parent
S=Path('/Users/anezium/vita-work/lg-wchess1')
MARK='#[cfg(test)]\nmod tests {'
FILES={'src/render/vita_renderer.rs','src/render/vita_renderer/world_list.rs'}
def text_at(head,path):return subprocess.check_output(['git','show',head+':'+path],cwd=S,text=True)
def normalized(text):
    prefix,tail=text.split(MARK,1)
    tail=tail.replace('list_serial: 0, alpha_serial: 0, class: 0 }','list_serial: 0, class: 0 }')
    tail=tail.replace(' | "world_alpha_parameter_once" | "actor_skin_output_reuse"','')
    tail=tail.replace(' || key == "world_alpha_parameter_once" || key == "actor_skin_output_reuse"','')
    for key in ['world_alpha_parameter_once','actor_skin_output_reuse']:
        tail=tail.replace('        assert!(v.get("'+key+'").is_none_or(|on| on == false));\n','')
    return prefix+MARK+tail
def verify(old,new):
    paths=subprocess.check_output(['git','diff','--name-only',old,new,'--','src','Cargo.toml','Cargo.lock','build.rs'],cwd=S,text=True).splitlines()
    assert set(paths)<=FILES,paths
    for path in paths:assert normalized(text_at(old,path))==normalized(text_at(new,path)),path
    delta=subprocess.check_output(['git','diff',old,new,'--','src'],cwd=S,text=True)
    return dict(old=old,new=new,kind='exact-known-cfg-test-only-edits',diff=delta)
if __name__=='__main__':
    host=json.loads((P/'logs/build-host-arm-binding.json').read_text())['head']
    native=json.loads((P/'builds/9da106a/BUILD-RECEIPT.json').read_text())['head']
    unit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=S,text=True).strip()
    (P/'runtime-equivalence.json').write_text(json.dumps(dict(oracle_head=host,native_head=native,unit_head=unit,checks=[verify(host,native),verify(native,unit)]),indent=2)+'\n')
