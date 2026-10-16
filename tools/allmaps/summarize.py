#!/usr/bin/env python3
"""Attach audited Vita ABI projections and human-reviewed entry image classes to host evidence."""
import argparse,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--evidence',type=Path,required=True);a=p.parse_args();E=a.evidence
layout=json.loads((E/'layout/VITA-LAYOUT.json').read_text());inv=json.loads((E/'INVENTORY.json').read_text())
inv['vita_layout']=layout
inv['abi_note']='Geometry uploads are platform invariant. PVS and uniform staging have explicit Vita ABI projections; construction peaks remain measured Mac buffer-capacity/payload peaks, not Vita total RAM.'
for r in inv['maps']:
 if r.get('retained_construction_failed'):continue
 r['vita_uniform_staging_capacity_bytes']=r['groups']*layout['prepared_bytes']
 r['vita_cpu_cluster_list_payload_bytes']=r['cpu_cluster_list_payload_bytes']//r['host_pointer_bytes']*layout['usize_bytes']
(E/'INVENTORY.json').write_text(json.dumps(inv,indent=2)+'\n')
lines=['# Retained host inventory','',inv['abi_note'],'',
 '| Map | Geometry bytes | Vita PVS incl. unknown | Vita uniform staging | Mac buffer peak | Groups | Drop / deform / ambient | Skipped sky / frozen rope |',
 '|---|---:|---:|---:|---:|---:|---:|---:|']
for r in inv['maps']:
 if r.get('retained_construction_failed'):lines.append(f"| {r['map']} | FAILED | | | | | | |");continue
 lines.append(f"| {r['map']} | {r['gpu_geometry_bytes']} | {r['vita_cluster_list_payload_including_unknown_bytes']} | {r['vita_uniform_staging_capacity_bytes']} | {r['construction_buffer_peak_bytes']} | {r['groups']} | {r['dropped_layers']} / {r['frozen_deforms']} / {r['frozen_ambient_models']} | {r['skipped_sky_batches']} / {r['frozen_ropes']} |")
(E/'INVENTORY.md').write_text('\n'.join(lines)+'\n')
gv=next(r for r in inv['maps'] if r['map']=='gvillage')
expected={'gpu_geometry_bytes':2271306,'vita_cpu_cluster_list_payload_bytes':2756028,'vita_uniform_staging_capacity_bytes':691456,'dropped_layers':156,'frozen_ambient_models':10,'frozen_deforms':1}
assert all(gv[k]==v for k,v in expected.items())
(E/'VILLAGE-INVENTORY-REGRESSION.json').write_text(json.dumps(dict(reference='pkg-2 renderer coverage census, 86792fe',expected=expected,actual={k:gv[k] for k in expected},passed=True),indent=2)+'\n')

# Review of the paired full-resolution PNGs/contact sheets. Causes are not inferred
# solely from a nonzero pixel difference. Off-camera/temporal gaps stay candidates.
sky=set('hedge3 wchess1 jlair1 jlair2 hatter2 wforest hedge2 rchess1 facade fortress1 hedge1 grounds2 grounds1'.split())
fog=set('potears1 potears2 garden2 garden4 jlair1 wforest'.split())
water=set('garden1 hedge2 hedge3 potears1 potears2 potears3 rchess1 tower2 wchess1 wchess2 funhouse hatter1 hatter2'.split())
portals=set('fortress1 hatter1 keep skool2'.split())
notes={
 'gvillage':'Fine surface/detail changes and different purple ambient prop shading at the lower right; this world fixture is not the separate pkg-2 parity fixture.',
 'utemple':'Subtle stone/water-edge tonal differences; no major entry geometry loss visible.',
 'hedge3':'Authored dark skyline is replaced by gradient; foreground/detail and flame differences.',
 'hatter1':'Clock-face centre and adjacent shading change; exact layer/alpha cause unisolated. Mirror is not clearly exposed at this entry.',
 'wchess1':'Small exposed background strip changes; masonry/floor detail differs.',
 'funhouse':'Small panel/foreground detail differences; entry stills do not prove mercury animation fidelity.',
 'jlair1':'Orange authored sky is replaced; low opaque fog/colour treatment also changes.',
 'wchess2':'Interior floor and masonry detail changes; authored sky is outside this view.',
 'qlair':'Small interior surface/detail/shading changes; no visible sky test at entry.',
 'jlair2':'Large distant authored sky-world/castle silhouette disappears into gradient.',
 'hatter2':'Authored circular sky/backdrop and background scenery are replaced; foreground geometry remains in the entry view.',
 'centipede2':'Rock/wood detail and shading changes; sky is outside the entrance framing.',
 'potears3':'Small corridor surface changes; water deformation is not established by this pose.',
 'fortress2':'Flame/billboard shape and surface-detail changes; sky outside this corridor.',
 'skool1':'Carpet/wall shading and fine texture changes; similar entry silhouette.',
 'skool2':'Floor/wall detail changes. Four portal material candidates but zero legacy portal objects load at this visit; no visible portal claim.',
 'wforest':'Star/cloud sky replaced by gradient in both visits; additional low-preset atmosphere difference.',
 'hedge2':'Small upper opening changes to gradient; ground/wall detail changes. Water outside the entry focus.',
 'potears1':'Full world broadly retains the fogged waterfall view; low loses substantial opaque fog and changes distant wall visibility. No temporal water claim.',
 'rchess1':'Small distant exposed background and floor/detail changes.',
 'pandemonium':'Bridge/wall texture and shading changes; moving rope/slime fidelity untested by entry stills.',
 'keep':'Wall/floor texture and tonal changes; mirror is not exposed clearly in this entry framing.',
 'facade':'Small exposed sky strip loses authored colour/backdrop; foreground detail changes.',
 'potears2':'Waterfall/pond surface/detail changes and low-preset atmospheric difference; freezing cannot be inferred from a single clock.',
 'garden2':'Foreground detail and low-preset distance treatment change; slime alpha/blend remains a candidate, not isolated by this pose.',
 'fortress1':'Authored blue sky replaced by gradient; return entry exposes absent distant sky geometry and dark silhouettes. School-window portal is not isolated here.',
 'garden1':'Very dark entry; small surface differences. Entry pose provides weak evidence for rope/water fidelity.',
 'garden4':'Strong loss of fine ice/snow surface brightness/detail already at full quality; blend/depth semantics are a candidate. Low also removes opaque fog.',
 'centipede1':'Masonry/ground detail and shading differences; no sky/rope gap applies.',
 'tower3':'Interior surface/edge changes; authored sky outside view.',
 'tower2':'Floor/wall surface differences; water deformation not exposed clearly.',
 'hedge1':'Small sky opening changes; ground/detail and flame shape differ.',
 'grounds2':'Orange authored backdrop becomes gradient; foreground detail remains different.',
 'garden3':'Tunnel/ground detail and shading change; no sky/rope gap applies.',
 'tower1':'Door/stone/wood detail changes; sky outside this interior view.',
 'grounds1':'Authored background/sky is simplified to gradient; foreground detail and flame shapes change.',
}
raw=json.loads((E/'IMAGE-DIFFERENCES.json').read_text());classes=[]
lines=['# Entry image review','',
 '39 visits × legacy/retained × full/installed-low = 156 PNGs, plus difference PNGs. All pairs use identical camera, transforms, seed and material time 1.0 at 960×544. The full preset uses scale 1; installed low uses scale 0.75. No pixel parity is claimed between renderers. Values count any nonzero RGB change, so sampling/rounding can raise changed-pixel percentages without major visual loss.', '',
 'Scope: world and ambient props at the initial chapter gameplay spawn; no cast/HUD or opening cinematic replay. A still at one clock does not prove or disprove frozen water/rope animation. Some portals/skies are off camera. Portal material candidates do not guarantee active legacy portals (Skool2 loads zero). `authored_sky_surfaces` in the raw entry JSON counts material declarations only; use skipped_sky_batches for actual scene omissions, which also include BSP flags and sky props.', '',
 'Observed groups: authored sky/backdrop loss in the maps marked sky gradient; surface/detail/tonal differences throughout; pronounced ice/snow differences in garden4; flame/billboard differences in fortress2, hedge1, hedge3 and grounds1; low opaque-fog differences most visible in potears1 and garden4. Alpha/blend and clock-face changes are candidates needing isolated layer tests. Missing portals and frozen water are retained-path omissions/risks, not all directly visible or temporally tested in these entry poses.', '',
 '| Visit | Full changed % / MAE | Low changed % / MAE | Visible classes | Review |',
 '|---|---:|---:|---|---|']
for v in raw['visits']:
 m=v['map'];observed=['dropped detail / surface shading','other (sampling/lighting cause not isolated)']
 if m in sky:observed.append('sky gradient')
 if m in fog:observed.append('other (low opaque fog)')
 if m in {'fortress2','hedge1','hedge3','grounds1'}:observed.append('other (frozen billboard/flame shape)')
 candidates=[]
 if m in water:candidates.append('frozen water / normal deformation or ambient mercury: temporal fidelity untested')
 if m in portals:candidates.append('missing portal: retained disables loading; not isolated in entry view')
 if m in {'garden2','garden3','garden4','utemple','hatter1','pandemonium','hedge2','hedge3','rchess1','wchess1','wchess2','tower1','grounds1'}:candidates.append('alpha/blend/depth: exact cause not isolated')
 c=dict(map=m,entry=v['entry'],label=v['label'],visible_classes=observed,candidates=candidates,review=notes[m],pixel_differences={q:v['comparisons'][q]['difference'] for q in ['full','low']})
 classes.append(c)
 f,l=[v['comparisons'][q]['difference'] for q in ['full','low']]
 lines.append(f"| {v['label']} | {f['changed_fraction']*100:.2f} / {f['mean_absolute_rgb']:.3f} | {l['changed_fraction']*100:.2f} / {l['mean_absolute_rgb']:.3f} | {', '.join(observed)} | {notes[m]} |")
(E/'IMAGE-CLASSIFICATION.json').write_text(json.dumps(dict(scope=raw['scope'],parity_claim=False,classification_method='human review of entry PNG pairs and contact sheets; static inventory used only for separately labelled candidates',visits=classes),indent=2)+'\n')
(E/'IMAGE-REVIEW.md').write_text('\n'.join(lines)+'\n')
print('PASS Vita ABI projections, village inventory and 39-visit image review')
