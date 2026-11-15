//! Dead Ants can straddle the eye while each triangle misses the actual view.
//! A mesh AABB cannot prove that case. Reject only triangles wholly outside one
//! homogeneous clip half-space; never move vertices or change surviving indices.
use super::*;
use std::cell::Cell;
use std::collections::HashMap;
thread_local! { static ANT: Cell<Option<bool>> = const { Cell::new(None) }; }
pub(crate) struct Scope(Option<bool>);
pub(crate) fn scope(dead: bool) -> Scope { Scope(ANT.with(|a| a.replace(Some(dead)))) }
pub(super) fn active() -> Option<bool> { ANT.with(Cell::get) }
impl Drop for Scope { fn drop(&mut self) { ANT.with(|a| a.set(self.0)); } }

fn outcode(v: Vec4) -> u8 {
    if !v.is_finite() { return 0; }
    // Retain the GPU's rounding neighbourhood, including tiny negative w.
    let margin = 1e-5 * v.abs().max_element().max(1.);
    let mut code = u8::from(v.w < -margin) << 6;
    for (i, x) in [v.x, v.y, v.z].into_iter().enumerate() {
        code |= u8::from(x < -v.w - margin) << (i * 2);
        code |= u8::from(x > v.w + margin) << (i * 2 + 1);
    }
    code
}
fn keep(tri: &[u16], codes: &[u8]) -> bool {
    tri.iter().fold(127, |common, &i| common & codes[i as usize]) == 0
}

pub(super) fn process(groups: &mut [Group]) {
    let enabled = crate::render::vita_renderer::config().ant_corpse_triangle_cull;
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    let trace = std::env::var_os("LOOKING_GLASS_CORPSE_PROBE").is_some()
        && matches!(crate::sim_maps::probe::frame(), 300 | 1400 | 1980 | 2030 | 2100..=2120);
    #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
    let trace = false;
    if !enabled && !trace { return; }
    let Some(matrix) = FRAME.with(|f| f.borrow().projection) else { return; };
    // Several material layers may refer to one immutable publication. Preserve
    // that sharing, but never modify a worker bank or its version in place.
    let mut changed: HashMap<(u64,u64,u64), Rc<prepared::Geometry>> = HashMap::new();
    let mut cut = 0;
    for g in groups {
        if !trace && g.ant_dead != Some(true) { continue; }
        for p in &mut g.passes {
            let mesh = p.geometry();
            if mesh.indices.len() % 3 != 0 || mesh.indices.iter().any(|&i| i as usize >= mesh.vertices.len()) { continue; }
            let codes: Vec<_> = mesh.vertices.iter().map(|v| outcode(matrix * v.position.extend(1.))).collect();
            if trace {
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                trace_pass(p, g.ant_dead.unwrap_or(false), g.transparent, matrix, &codes);
            }
            if !enabled || g.ant_dead != Some(true) { continue; }
            let removed = mesh.indices.chunks_exact(3).filter(|t| !keep(t, &codes)).count();
            if removed == 0 { continue; }
            cut += removed as u64;
            let version = p.geometry.as_ref().map(|m| (m.version.identity,m.version.vertices,m.version.topology));
            if let Some(new) = version.and_then(|v| changed.get(&v)) { p.geometry = Some(new.clone()); continue; }
            let mut copy = Mesh { vertices: mesh.vertices.clone(), indices: Vec::with_capacity(mesh.indices.len()-removed*3), texture: mesh.texture.clone() };
            for t in mesh.indices.chunks_exact(3).filter(|t| keep(t, &codes)) { copy.indices.extend_from_slice(t); }
            if copy.indices.is_empty() {
                // An empty group must not reach upload's nonempty validation.
                p.mesh.indices.clear(); p.geometry = None;
            } else {
                let new = Rc::new(prepared::Geometry::new(copy));
                if let Some(v) = version { changed.insert(v,new.clone()); }
                p.geometry = Some(new);
            }
        }
        g.passes.retain(|p| !p.geometry().indices.is_empty());
    }
    retained::count("vita_ant_corpse_triangles_cut", cut);
}

#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
fn trace_pass(p: &Pass, dead: bool, transparent: bool, m: Mat4, codes: &[u8]) {
    let mesh=p.geometry(); let mut behind=0;let mut crossing=0;let mut outside=0;let mut area=0f64;
    for tri in mesh.indices.chunks_exact(3) {
        let v:Vec<_>=tri.iter().map(|&i|m*mesh.vertices[i as usize].position.extend(1.)).collect();
        let n=v.iter().filter(|v|v.w<=0.).count();if n==3 {behind+=1;} else if n>0 {crossing+=1;}
        outside+=usize::from(!keep(tri,codes));
        area+=clipped_area(&v);
    }
    println!("[lg/corpse-probe] texture={:?} frame={} dead={dead} transparent={transparent} blend={} depth_equal={} cutout={} appearance={}/{}/{} vertices={} triangles={} behind={behind} crossing={crossing} outside={outside} clipped_screen_area={area}",p.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id),crate::sim_maps::probe::frame(),p.blend_index,p.depth_equal,p.cutout,p.appearance.dissolve,p.appearance.ghost,p.appearance.power,mesh.vertices.len(),mesh.indices.len()/3);
}
#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
fn clipped_area(triangle: &[Vec4]) -> f64 {
    let mut polygon=triangle.to_vec();
    for plane in 0..6 {
        let distance=|v:Vec4| match plane {0=>v.w+v.x,1=>v.w-v.x,2=>v.w+v.y,3=>v.w-v.y,4=>v.w+v.z,_=>v.w-v.z};
        let mut result=Vec::new();
        if let Some(&last)=polygon.last() {
            let mut a=last;let mut da=distance(a);
            for &b in &polygon {let db=distance(b);
                if (da>=0.) != (db>=0.) {result.push(a+(b-a)*(da/(da-db)));}
                if db>=0. {result.push(b);} a=b;da=db;
            }
        }
        polygon=result;
    }
    if polygon.len()<3 || polygon.iter().any(|p|p.w<=0.) {return 0.;}
    let points:Vec<_>=polygon.iter().map(|p| (p.x as f64/p.w as f64,p.y as f64/p.w as f64)).collect();
    (0..points.len()).map(|i| {let (a,b)=(points[i],points[(i+1)%points.len()]);a.0*b.1-b.0*a.1}).sum::<f64>().abs()/8.
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_only_a_common_strict_clip_half_space() {
        let points=[vec4(-2.,0.,0.,1.),vec4(-3.,1.,0.,1.),vec4(-4.,-1.,0.,1.),vec4(2.,0.,0.,1.)];
        let codes:Vec<_>=points.into_iter().map(outcode).collect();
        assert!(!keep(&[0,1,2],&codes));assert!(keep(&[0,1,3],&codes));
        assert_eq!(outcode(vec4(1.+1e-6,0.,0.,1.)),0);
        assert_eq!(outcode(Vec4::splat(f32::NAN)),0);
        let behind:Vec<_>=[vec4(-10.,0.,0.,-1.),vec4(10.,0.,0.,-1.),vec4(0.,10.,0.,-1.)].into_iter().map(outcode).collect();
        assert!(!keep(&[0,1,2],&behind));
        // Any convex combination of vertices sharing an outside plane remains
        // outside. Test mixed w, far and near planes as well as screen sides.
        let mut seed=7u32;
        for _ in 0..10000 {
            let mut next=||{seed=seed.wrapping_mul(1664525).wrapping_add(1013904223);(seed as f32/u32::MAX as f32)*40.-20.};
            let v=[vec4(next(),next(),next(),next()),vec4(next(),next(),next(),next()),vec4(next(),next(),next(),next())];
            let common=v.into_iter().map(outcode).fold(127,|a,b|a&b);
            if common!=0 {for a in 0..=10 {for b in 0..=10-a {
                let p=(v[0]*(a as f32)+v[1]*(b as f32)+v[2]*((10-a-b) as f32))/10.;
                assert_ne!(outcode(p)&common,0);
            }}}
        }
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;
    #[test]
    fn key_defaults_off_and_actor_scope_restores_on_unwind() {
        let c=crate::render::vita_renderer::Config::default();
        assert!(!c.ant_corpse_triangle_cull);
        assert!(serde_json::from_str::<crate::render::vita_renderer::Config>(r#"{"ant_corpse_triangle_cull":1}"#).is_err());
        assert_eq!(active(),None);
        let _live=scope(false);assert_eq!(active(),Some(false));
        let _=std::panic::catch_unwind(||{let _dead=scope(true);assert_eq!(active(),Some(true));panic!("scope test");});
        assert_eq!(active(),Some(false));
    }
}

#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
pub(super) fn report(groups: &[Group]) {
    let frame=crate::sim_maps::probe::frame();
    if std::env::var_os("LOOKING_GLASS_CORPSE_PROBE").is_none() || !matches!(frame,1400 | 1980 | 2030 | 2100..=2120) {return;}
    let Some(matrix)=FRAME.with(|f| f.borrow().projection) else {return;};
    for dead in [false,true] {
        let mut passes=0;let mut vertices=0;let mut triangles=0;let mut area=0.;
        for g in groups.iter().filter(|g|g.ant_dead==Some(dead)) {for p in &g.passes {
            passes+=1;vertices+=p.geometry().vertices.len();triangles+=p.geometry().indices.len()/3;
            for t in p.geometry().indices.chunks_exact(3) {
                let v:Vec<_>=t.iter().map(|&i|matrix*p.geometry().vertices[i as usize].position.extend(1.)).collect();
                area+=clipped_area(&v);
            }
        }}
        println!("[lg/corpse-submit] frame={frame} dead={dead} passes={passes} vertices={vertices} triangles={triangles} clipped_screen_area={area}");
    }
}
