"""Compiler layout evidence from an isolated historical source archive, host only."""
from pathlib import Path
import io, json, os, re, subprocess, sys, tarfile, tempfile
R=Path(__file__).resolve().parents[2]
O=Path(sys.argv[1]).resolve()
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],
    RUSTUP_TOOLCHAIN='stable',RUSTC_BOOTSTRAP='1',RUSTFLAGS='--cfg observer_free_B',
    CARGO_TARGET_DIR=str(O/'target-diagnostic'),CARGO_INCREMENTAL='0',CARGO_NET_OFFLINE='true')
rows=[]
for label,revision in [('r11b','aec5590'),('r11c','7fdf87b'),('r11d','a286d3a')]:
    log_path=O/'baseline'/label/'type-layout.log'
    if '--parse-only' not in sys.argv and ('--only-r11d' not in sys.argv or label=='r11d'):
      with tempfile.TemporaryDirectory(prefix='r11d-layout-') as temp:
        source=Path(temp)
        with tarfile.open(fileobj=io.BytesIO(subprocess.check_output(['git','archive',revision],cwd=R))) as tar:
            tar.extractall(source,filter='data')
        # Force emission even when Cargo has a same-named archived root cached.
        os.utime(source/'src/main.rs',None)
        with log_path.open('w') as log:
            subprocess.run(['cargo','rustc','--release','--offline','--locked','--features',
                'native_timing,frame_total_only,sim_maps_probe','--','-Zprint-type-sizes'],
                cwd=source,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    groups=[]
    for line in log_path.read_text().splitlines():
        if line.startswith('print-type-size type:'):
            groups.append([line])
        elif line.startswith('print-type-size ') and groups:
            groups[-1].append(line)
    relevant=[group for group in groups if (group[0].startswith('print-type-size type: `{async') and
        ('viewer::run()' in group[0] or ('macroquad::Window::from_config' in group[0] and 'src/main.rs:1559:' in group[0])))]
    assert relevant,(label,'compiler future layouts missing')
    run=next(group for group in relevant if group[0].startswith('print-type-size type: `{async fn body of viewer::run()}'))
    root=next(group for group in relevant if 'macroquad::Window::from_config' in group[0])
    size=lambda group:int(re.search(r'`: (\d+) bytes',group[0])[1])
    variants=[]
    for line in run[1:]:
        if 'variant `' in line:variants.append([line])
        elif variants:variants[-1].append(line)
    maximum=max(variants,key=lambda group:int(re.search(r': (\d+) bytes',group[0])[1]))
    rows.append(dict(label=label,revision=revision,run_future_bytes=size(run),
        root_future_bytes=size(root),largest_run_variant=maximum,layouts=relevant))
    print(label,'\n'.join('\n'.join(group) for group in relevant),flush=True)
assert [row['run_future_bytes'] for row in rows]==[752928,1144544,752928]
assert [row['root_future_bytes'] for row in rows]==[1506576,2289808,1506576]
(O/'proof/FUTURE-LAYOUT.json').write_text(json.dumps(dict(status='PASS',
    method='stable compiler with RUSTC_BOOTSTRAP=1 for diagnostic -Zprint-type-sizes only; shipping flags unchanged',rows=rows),indent=2)+'\n')
