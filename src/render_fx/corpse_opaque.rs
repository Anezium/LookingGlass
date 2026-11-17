//! A discard-free variant of the same shader for proven opaque actor packets.
//! Immutable all-255 texture alpha survives filtering, size reduction and DXT5.
//! Weak managed handles include their slot generation, so reused GPU ids cannot
//! inherit an old texture's certificate. The table does not keep textures alive.
use super::*;
use std::collections::HashMap;
thread_local! { static OPAQUE: RefCell<HashMap<macroquad::miniquad::TextureId,Texture2D>> = RefCell::default(); }
fn all_opaque(rgba:&[u8]) -> bool { !rgba.is_empty() && rgba.len()%4==0 && rgba.chunks_exact(4).all(|p|p[3]==255) }
pub(crate) fn note_texture(texture:&Texture2D, rgba:&[u8]) {
    let c=crate::render::vita_renderer::config();
    if !c.ant_corpse_opaque_shader && !c.actor_opaque_shader {return;}
    OPAQUE.with(|table| {
        let mut table=table.borrow_mut();
        if table.len()>=4096 {table.clear();}
        let id=texture.raw_miniquad_id();
        if all_opaque(rgba) {
            table.insert(id,texture.weak_clone());
        } else {table.remove(&id);}
    });
}
fn opaque_texture(texture:Option<&Texture2D>) -> bool {
    texture.is_none_or(|t|OPAQUE.with(|table|table.borrow().get(&t.raw_miniquad_id()).is_some_and(|old| *old==t.weak_clone())))
}
pub(super) fn eligible(p:&Pass) -> bool {
    p.blend_index==0 && !p.depth_equal && rejection_free(p)
}
/// Blending and depth equality do not affect whether a fragment can execute discard.
/// Preserve all output arithmetic and all pipeline state; reject every appearance/cutout
/// whose rejection predicate is not certified false for the complete primitive.
pub(super) fn discard_free(p:&Pass) -> bool {
    // Two strict pixel rejections narrowed this key: the broad certified-texture
    // route changed potears1 pixels, and an environment-anywhere rule changed the
    // garden3 marble statue by 1..2 channel units on 24 frames. The proven case is
    // the chess bodies' two authored layers: environment reflection first, skin
    // second. Require exactly that pair; single-layer pickups and multi-layer props
    // with a trailing environment stage keep the original program.
    p.cutout==0. && rejection_free(p)
        && p.mesh.surface.as_ref().is_some_and(|s| s.layers.len()==2
            && s.layers[0].stage.environment && !s.layers[1].stage.environment)
}
fn rejection_free(p:&Pass) -> bool {
    !p.effect && p.appearance.dissolve==0. && !p.appearance.ghost
        && (p.cutout==0. || (p.cutout==4. && p.gain.w==1.
            && opaque_texture(p.mesh.texture.as_ref()) && p.geometry().vertices.iter().all(|v|v.color[3]==255)))
}
pub(super) fn fragment() -> String {
    // Keep every lighting, fog, texture, colour and alpha expression verbatim.
    crate::render::FRAGMENT.lines().filter(|line|!line.contains("discard;")).collect::<Vec<_>>().join("\n")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_keeps_alpha_and_appearance_fallbacks() {
        let mut p=Pass {
            _game_thread:crate::game_thread::GameThread::default(),geometry:None,effect:false,
            blend_index:0,depth_equal:false,
            #[cfg(target_os="vita")] world_static:None,
            #[cfg(target_os="vita")] world_geometry_counted:false,
            mesh:Mesh { upload_identity: None, surface: None,vertices:vec![Vertex::new(0.,0.,0.,0.,0.,WHITE);3],indices:vec![0,1,2],texture:None},
            material:None,lit:true,clamp:false,cutout:4.,cutout_reference:0.5,gain:Vec4::ONE,
            fog:0.,model:true,appearance:Appearance::default(),filter_fade:0.};
        assert!(eligible(&p));
        assert!(!discard_free(&p), "certified cutout remains on the original actor shader");
        p.gain.w=0.3;assert!(!eligible(&p));p.gain.w=1.;
        p.mesh.vertices[0].color[3]=0;assert!(!eligible(&p));
        p.cutout=0.;assert!(eligible(&p)); // no alpha rejection in the full program
        assert!(!discard_free(&p), "ordinary actors retain their original program");
        let layer=|environment|Layer {stage:Stage {environment,..Default::default()},textures:vec![],upload_identities:vec![]};
        p.mesh.surface=Some(Rc::new(Surface {
            layers:vec![layer(true),layer(false)],
            transparent:false,preserve_skin:true,
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            label:(false,None),
            actors:RefCell::default(),
        }));
        assert!(discard_free(&p), "two-stage environment+skin actor body");
        let pair=p.mesh.surface.take().unwrap();
        p.mesh.surface=Some(Rc::new(Surface {
            layers:vec![layer(false),layer(false),layer(true)],
            transparent:false,preserve_skin:true,
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            label:(true,Some("models/marble/skin01.tga".into())),
            actors:RefCell::default(),
        }));
        assert!(!discard_free(&p), "trailing environment prop keeps the original program");
        p.mesh.surface=Some(Rc::new(Surface {
            layers:vec![layer(true)],
            transparent:false,preserve_skin:true,
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            label:(true,Some("models/pickups/health_small".into())),
            actors:RefCell::default(),
        }));
        assert!(!discard_free(&p), "single-layer pickup keeps the original program");
        p.mesh.surface=Some(pair);
        p.appearance.dissolve=0.1;assert!(!eligible(&p));p.appearance.dissolve=0.;
        p.appearance.ghost=true;assert!(!eligible(&p));p.appearance.ghost=false;
        p.blend_index=1;assert!(!eligible(&p));assert!(discard_free(&p));p.blend_index=0;
        p.depth_equal=true;assert!(!eligible(&p));assert!(discard_free(&p));p.depth_equal=false;
        p.effect=true;assert!(!eligible(&p));assert!(!discard_free(&p));p.effect=false;
        p.cutout=2.;assert!(!eligible(&p));assert!(!discard_free(&p));
    }
    #[test]
    fn incomplete_or_nonopaque_texels_are_not_certified() {
        assert!(super::all_opaque(&[0,0,0,255,255,255,255,255]));
        for bytes in [vec![],vec![255;3],vec![255;5],vec![255,255,255,254],vec![255,255,255,0]] {assert!(!super::all_opaque(&bytes));}
        assert!(!crate::render::vita_renderer::Config::default().ant_corpse_opaque_shader);
        let c:crate::render::vita_renderer::Config=serde_json::from_value(serde_json::json!({"actor_opaque_shader":true})).unwrap();
        assert!(c.actor_opaque_shader);
        assert!(!c.ant_corpse_opaque_shader);
        assert!(!crate::render::vita_renderer::Config::default().actor_opaque_shader);
        assert!(!crate::render::vita_renderer::Config::default().actor_discard_free_shader);
    }
    #[test]
    fn specialization_removes_only_the_four_discard_statements() {
        let source=crate::render::FRAGMENT;
        assert_eq!(source.lines().filter(|l|l.contains("discard;")).count(),4);
        let specialized=super::fragment();assert!(!specialized.contains("discard"));
        assert_eq!(specialized,source.lines().filter(|l|!l.contains("discard;")).collect::<Vec<_>>().join("\n"));
        assert!(specialized.contains("float alpha=diffuse.a*Gain.a*color.a;"));
        assert!(specialized.contains("dynamicLight(worldPosition)"));
    }
}
