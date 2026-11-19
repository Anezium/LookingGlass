use std::rc::Rc;

/// Keeps NPC renderer resources on the owner thread. The pinned macroquad
/// `Texture2D` is `Send`, but its destructor touches the GL context.
///
/// ```compile_fail,E0277
/// use owner_thread::NpcArt;
/// fn require_send<T: Send>() {}
/// require_send::<NpcArt<()>>();
/// ```
pub struct NpcArt<T> {
    pub(crate) resources: T,
    _owner_thread: Rc<()>,
}

impl<T> NpcArt<T> {
    pub(crate) fn new(resources: T) -> Self {
        Self { resources, _owner_thread: Rc::new(()) }
    }
}
