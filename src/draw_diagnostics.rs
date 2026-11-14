//! Host/Vita draw omission diagnostics. No simulation callers.
#[derive(Clone, Copy)]
pub(crate) enum Class { AntCorpses, LiveActors, Fx, Decals }
pub(crate) fn skip(class: Class) -> bool {
    #[cfg(feature="vita_renderer")]
    { let c=crate::render::vita_renderer::config(); match class {
        Class::AntCorpses=>c.diag_skip_ant_corpses, Class::LiveActors=>c.diag_skip_live_actors,
        Class::Fx=>c.diag_skip_fx, Class::Decals=>c.diag_skip_decals,
    } }
    #[cfg(not(feature="vita_renderer"))]
    { let _=class; false }
}
