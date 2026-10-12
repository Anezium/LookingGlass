//! Actor-owned layer recipes. Appearance and skinned vertices remain live inputs.
use super::*;

#[derive(Clone,Copy)]
struct LayerState {
    blend:usize, equal:bool, lit:bool, clamp:bool, cutout:f32, reference:f32,
    gain:Vec4, fog:f32, frame:usize, vertex_alpha:bool, dynamic:bool,
}
pub(super) struct State { layers:Vec<LayerState>, time:u32 }
fn gain(s:&Stage,t:f32)->Vec4 {
    (s.constant_rgb.unwrap_or(Vec3::ONE)*s.rgb.as_ref().map_or(1.,|w|w.sample(t)).clamp(0.,2.))
        .extend(s.alpha.as_ref().map(|w|w.sample(t)).or(s.constant_alpha).unwrap_or(1.).clamp(0.,1.))
}
fn simple(s:&Stage)->bool {
    !s.environment && s.vector_uv.is_none() && s.dot_alpha.is_none() && s.mods.is_empty()
}
fn state(s:&Stage,t:f32)->LayerState {
    LayerState {blend:crate::render::blend_index(s.blend),equal:s.depth_equal,
        lit:!s.identity,clamp:s.clamp,cutout:s.alpha_test as f32,reference:s.alpha_reference,
        gain:gain(s,t),fog:fog(s.blend),frame:s.frame(t),vertex_alpha:s.vertex_alpha,
        dynamic:s.rgb.is_some()||s.alpha.is_some()||s.images.len()>1}
}
pub(super) fn prepare(surface:&Surface,mesh:&Mesh,time:f32,appearance:Appearance)->Option<Vec<Pass>> {
    let c=crate::render::vita_renderer::config();
    let limit=c.actor_max_layers.min(surface.layers.len());
    if !surface.layers.iter().take(limit).all(|l|simple(&l.stage)){return None;}
    if surface.layers.len()>limit {retained::count("vita_fx_dropped_layers",(surface.layers.len()-limit) as u64);}
    let mut actors=surface.actors.borrow_mut();
    let id=mesh as *const Mesh as usize;
    // Surface lifetime owns the cache; no geometry or Actor/saved state is retained.
    if actors.len()>=256 && !actors.contains_key(&id){actors.clear();}
    let hit=actors.contains_key(&id);
    let cached=actors.entry(id).or_insert_with(||State {
        layers:surface.layers.iter().take(limit).map(|l|state(&l.stage,time)).collect(),time:time.to_bits()
    });
    let mut refreshed=0;
    if cached.time!=time.to_bits() {
        for (p,l) in cached.layers.iter_mut().zip(&surface.layers) {
            if p.dynamic {p.gain=gain(&l.stage,time);p.frame=l.stage.frame(time);refreshed+=1;}
        }
        cached.time=time.to_bits();
    }
    let mut shared:[Option<Rc<Mesh>>;2]=[None,None];
    let mut copies=0;
    let mut passes=Vec::with_capacity(limit);
    for (p,l) in cached.layers.iter().zip(&surface.layers) {
        let variant=usize::from(p.vertex_alpha);
        let geometry=if c.actor_shared_geometry {
            shared[variant].get_or_insert_with(||{
                copies+=1;
                let mut m=copy_mesh(mesh);
                for v in &mut m.vertices {
                    v.color[3]=if p.vertex_alpha{(255.*(v.color[3] as f32/255.).clamp(0.,1.)) as u8}else{255};
                }
                Rc::new(m)
            }).clone()
        } else {
            copies+=1;
            let mut m=copy_mesh(mesh);
            for v in &mut m.vertices {v.color[3]=if p.vertex_alpha{(255.*(v.color[3] as f32/255.).clamp(0.,1.)) as u8}else{255};}
            Rc::new(m)
        };
        passes.push(Pass {geometry:Some(geometry),effect:false,blend_index:p.blend,depth_equal:p.equal,
            #[cfg(target_os="vita")]
            world_static:None,
            #[cfg(target_os="vita")]
            world_geometry_counted:false,
            mesh:Mesh {vertices:Vec::new(),indices:Vec::new(),texture:Some(l.textures[p.frame].clone())},
            material:None,lit:p.lit,clamp:p.clamp,cutout:p.cutout,cutout_reference:p.reference,
            gain:p.gain,fog:p.fog,model:true,appearance,filter_fade:0.});
    }
    #[cfg(target_os="vita")]
    crate::frame_profile::counts(&[
        ("vita_actor_layer_cache_hits",if hit{limit as u64}else{0}),
        ("vita_actor_layer_cache_builds",if hit{0}else{limit as u64}),
        ("vita_actor_dynamic_layer_refreshes",refreshed),
        ("vita_actor_geometry_copies",copies),
        ("vita_actor_geometry_copies_skipped",limit as u64-copies),
        ("vita_actor_attribute_vertices_skipped",(limit as u64-copies)*mesh.vertices.len() as u64),
    ]);
    #[cfg(not(target_os="vita"))]
    {
        for (k,v) in [("vita_actor_layer_cache_hits",if hit{limit as u64}else{0}),
            ("vita_actor_layer_cache_builds",if hit{0}else{limit as u64}),
            ("vita_actor_dynamic_layer_refreshes",refreshed),("vita_actor_geometry_copies",copies),
            ("vita_actor_geometry_copies_skipped",limit as u64-copies)] {retained::count(k,v);}
    }
    Some(passes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_or_normal_dependent_layers_require_exact_preparation() {
        assert!(simple(&Stage::default()));
        assert!(!simple(&Stage {environment:true,..Default::default()}));
        assert!(!simple(&Stage {dot_alpha:Some(Vec2::ONE),..Default::default()}));
        assert!(!simple(&Stage {mods:vec![crate::materials::TcMod::Scroll(Vec2::ONE)],..Default::default()}));
    }
}
