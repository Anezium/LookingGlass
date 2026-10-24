//! Runtime actor identities survive vector moves and never enter saves or workers.
use std::cell::Cell;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ActorId(u64);
thread_local! {
    static NEXT: Cell<u64> = const { Cell::new(1) };
    static CURRENT: Cell<Option<ActorId>> = const { Cell::new(None) };
}
impl Default for ActorId {
    fn default() -> Self {
        NEXT.with(|n| { let id=n.get(); n.set(id.checked_add(1).expect("skin actor identity overflow")); Self(id) })
    }
}
impl ActorId {
    pub(crate) fn census_id(self) -> u64 { self.0 }
    pub(crate) fn scope(self) -> Scope { Scope(CURRENT.with(|c| c.replace(Some(self)))) }
    pub(crate) fn scope_unless_set(self) -> Scope { current().unwrap_or(self).scope() }
}
pub(crate) fn current() -> Option<ActorId> { CURRENT.with(Cell::get) }
pub(crate) struct Scope(Option<ActorId>);
impl Drop for Scope { fn drop(&mut self) { CURRENT.with(|c| c.set(self.0)); } }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_preserve_actor_identity_across_shared_models_and_restore_after_draw() {
        let actor=ActorId::default(); let model=ActorId::default();
        assert_ne!(actor,model); assert_eq!(current(),None);
        { let _actor=actor.scope(); { let _model=model.scope_unless_set(); assert_eq!(current(),Some(actor)); }
          assert_eq!(current(),Some(actor)); }
        assert_eq!(current(),None);
        { let _model=model.scope_unless_set(); assert_eq!(current(),Some(model)); }
        assert_eq!(current(),None);
    }
}
