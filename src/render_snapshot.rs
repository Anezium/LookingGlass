//! Serial extraction experiment. Values here own their payload; resident UI/art
//! resources remain on the graphics owner. This is not a complete render snapshot.
use macroquad::prelude::*;

pub(crate) struct Subtitle {
    pub text: String,
    pub speaker: String,
    pub elapsed: f32,
    pub duration: f32,
}
impl Subtitle {
    pub fn draw(&self, key: &str, ui: &crate::ui::Ui) {
        crate::story::draw_subtitle(&self.text, &self.speaker, key, self.elapsed, self.duration, ui);
    }
}

pub(crate) struct HudInput {
    pub prompt: Option<String>,
    pub objective: Option<String>,
    pub help: bool,
    pub stats: crate::inventory::Stats,
    pub breath: crate::water::Breath,
    pub immersion: u8,
    pub script_motion: u8,
    pub swimming: bool,
    pub scripted: bool,
    pub fades: Fades,
    pub subtitle: Option<Subtitle>,
    pub skip: bool,
    pub level: Vec<HudCommand>,
}
impl HudInput {
    /// Resident payload capacity, excluding allocator headers. HUD stats contain
    /// no collected/loot trees. Saves are deliberately outside this byte count.
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.prompt.as_ref().map_or(0, String::capacity)
            + self.objective.as_ref().map_or(0, String::capacity)
            + self.level.capacity() * std::mem::size_of::<HudCommand>()
            + self.level.iter().map(HudCommand::text_capacity).sum::<usize>()
            + self.subtitle.as_ref().map_or(0, |s| s.text.capacity() + s.speaker.capacity())
    }
}

pub(crate) enum Primitive {
    Dialog(Rect),
    Label(String, f32, f32, f32, Color),
    Rectangle(f32, f32, f32, f32, Color),
    Boss(String, f32),
}
/// Rc is a resident renderer resource handle. No controller is retained.
pub(crate) struct HudCommand {
    ui: std::rc::Rc<crate::ui::Ui>,
    primitive: Primitive,
}
impl HudCommand {
    pub fn draw(&self) {
        match &self.primitive {
            Primitive::Dialog(r) => self.ui.dialog(*r),
            Primitive::Label(t, x, y, size, color) => self.ui.label(t, *x, *y, *size, *color),
            Primitive::Rectangle(x, y, w, h, c) => draw_rectangle(*x, *y, *w, *h, *c),
            Primitive::Boss(t, remaining) => crate::hud::boss_meter(&self.ui, t, *remaining),
        }
    }
    fn text_capacity(&self) -> usize {
        match &self.primitive {
            Primitive::Label(t, ..) | Primitive::Boss(t, _) => t.capacity(),
            _ => 0,
        }
    }
}
pub(crate) struct Painter<'a> {
    ui: &'a std::rc::Rc<crate::ui::Ui>,
    out: &'a mut Vec<HudCommand>,
}
impl<'a> Painter<'a> {
    pub fn new(ui: &'a std::rc::Rc<crate::ui::Ui>, out: &'a mut Vec<HudCommand>) -> Self { Self { ui, out } }
    fn push(&mut self, primitive: Primitive) { self.out.push(HudCommand { ui: self.ui.clone(), primitive }); }
    pub fn dialog(&mut self, r: Rect) { self.push(Primitive::Dialog(r)); }
    pub fn label(&mut self, t: &str, x: f32, y: f32, size: f32, c: Color) { self.push(Primitive::Label(t.into(), x, y, size, c)); }
    pub fn rectangle(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) { self.push(Primitive::Rectangle(x, y, w, h, c)); }
    pub fn boss(&mut self, t: &str, remaining: f32) { self.push(Primitive::Boss(t.into(), remaining)); }
}

/// Bounded by the eight authored fade sources; no per-frame vector allocation.
#[derive(Default)]
pub(crate) struct Fades { colors: [Color; 8], len: usize }
impl Fades {
    pub fn push(&mut self, color: Color) { self.colors[self.len] = color; self.len += 1; }
}
impl<'a> IntoIterator for &'a Fades {
    type Item = &'a Color;
    type IntoIter = std::slice::Iter<'a, Color>;
    fn into_iter(self) -> Self::IntoIter { self.colors[..self.len].iter() }
}
pub(crate) struct TailInput {
    pub alive: bool,
    pub feet: Vec3,
    pub cinema: bool,
    #[cfg(feature="native_timing")]
    pub timing: crate::native_timing::State,
}

/// Compact census/prototype samples. Handles are indices scoped to one visit;
/// they must be paired with a retained visit generation before asynchronous use.
#[derive(Clone, Copy)]
pub(crate) struct ParticleInput {
    pub emitter: usize, pub position: Vec3, pub age: f32,
    pub life: f32, pub size: f32, pub roll: f32,
}
#[derive(Clone, Copy)]
pub(crate) struct PickupInput { pub handle: usize, pub position: Vec3, pub collected: bool }
#[derive(Default)]
pub(crate) struct DynamicInput {
    pub view: Mat4, pub environment_clock: f32, pub pickup_clock: f32,
    pub fog: Vec4, pub liquid: Vec4,
    pub transforms: Vec<(usize, Vec3, Quat)>,
    pub particles: Vec<ParticleInput>, pub pickups: Vec<PickupInput>,
}
impl DynamicInput {
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.transforms.len() * std::mem::size_of::<(usize, Vec3, Quat)>()
            + self.particles.len() * std::mem::size_of::<ParticleInput>()
            + self.pickups.len() * std::mem::size_of::<PickupInput>()
    }
    pub fn capacity_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.transforms.capacity() * std::mem::size_of::<(usize, Vec3, Quat)>()
            + self.particles.capacity() * std::mem::size_of::<ParticleInput>()
            + self.pickups.capacity() * std::mem::size_of::<PickupInput>()
    }
}

/// A renderer visit has no relationship to an address or a saved game identifier.
/// Rebuilding even the same map creates a fresh namespace for completions.
pub(crate) struct RenderVisit(u64);
impl Default for RenderVisit {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        assert!(id != 0 && id != u64::MAX, "render visit namespace exhausted");
        Self(id)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HandoffTag {
    visit: u64,
    slot: usize,
    frame: u64,
}
/// Owned renderer output. Local animation transforms are deliberately retained
/// separately from the facial/global pose submitted to skinning.
pub(crate) struct HandoffCompletion {
    tag: HandoffTag,
    pose: Option<crate::cinematic::ActorPose>,
}
impl HandoffCompletion {
    pub fn rendered(visit: &RenderVisit, slot: usize, frame: u64,
        pose: Option<&crate::cinematic::ActorPose>) -> Self {
        Self { tag: HandoffTag { visit: visit.0, slot, frame }, pose: pose.cloned() }
    }
}
/// Serial acknowledgement prototype. A future producer must wait for the exact
/// expected completion; it cannot substitute the current simulation pose.
#[derive(Default)]
pub(crate) struct HandoffReturn {
    visit: u64,
    expected: Option<HandoffTag>,
    pending: Option<HandoffCompletion>,
    acknowledged: Option<HandoffCompletion>,
}
impl HandoffReturn {
    pub fn retire(&mut self) { *self = Self::default(); }
    pub fn expect(&mut self, completion: &HandoffCompletion) {
        if self.visit != completion.tag.visit { self.retire(); }
        self.visit = completion.tag.visit;
        self.expected = Some(completion.tag);
        self.pending = None;
    }
    pub fn publish(&mut self, completion: HandoffCompletion) -> bool {
        if self.expected != Some(completion.tag) { return false; }
        self.pending = Some(completion);
        true
    }
    /// Called before simulation, including paused and menu iterations. Frames
    /// without a new draw keep the acknowledged last draw, not a boundary sample.
    pub fn consume(&mut self, visit: &RenderVisit) -> bool {
        if self.visit != visit.0 { self.retire(); return true; }
        if let Some(expected) = self.expected {
            let Some(completion) = self.pending.take() else { return false; };
            if completion.tag != expected { return false; }
            self.acknowledged = Some(completion);
            self.expected = None;
        }
        true
    }
    pub fn pose(&self, visit: &RenderVisit) -> Option<&crate::cinematic::ActorPose> {
        if self.visit != visit.0 || self.expected.is_some() { return None; }
        self.acknowledged.as_ref().and_then(|c| c.pose.as_ref())
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::*;
    fn pose(x: f32) -> crate::cinematic::ActorPose {
        let transform = crate::skeletal::Transform { translation: vec3(x, 0., 0.), rotation: Quat::IDENTITY };
        crate::cinematic::ActorPose { local: vec![transform], transform, scale: 1. }
    }
    #[test]
    fn handoff_exact_completion_and_last_draw_survive_pause_and_skip() {
        let visit = RenderVisit::default();
        let mut state = HandoffReturn::default();
        let mut source = pose(4.);
        let completion = HandoffCompletion::rendered(&visit, 3, 10, Some(&source));
        state.expect(&completion);
        assert!(!state.consume(&visit));
        assert!(state.pose(&visit).is_none());
        assert!(!state.publish(HandoffCompletion::rendered(&visit, 4, 10, Some(&source))));
        assert!(!state.publish(HandoffCompletion::rendered(&visit, 3, 9, Some(&source))));
        source.local[0].translation.x = 99.;
        assert!(state.publish(completion));
        assert!(state.consume(&visit));
        assert_eq!(state.pose(&visit).unwrap().local[0].translation.x, 4.);
        // No renderer publication during a menu/pause; an immediate skip/natural
        // end reads exactly the same acknowledged local pose.
        for _ in 0..4 { assert!(state.consume(&visit)); }
        assert_eq!(state.pose(&visit).unwrap().local[0].translation.x, 4.);
        let next = HandoffCompletion::rendered(&visit, 3, 11, Some(&pose(7.)));
        state.expect(&next); assert!(state.publish(next)); assert!(state.consume(&visit));
        assert_eq!(state.pose(&visit).unwrap().local[0].translation.x, 7.);
    }
    #[test]
    fn handoff_restore_failed_extraction_and_no_pose_retire_old_output() {
        let first = RenderVisit::default(); let restored = RenderVisit::default();
        let mut state = HandoffReturn::default();
        let completion = HandoffCompletion::rendered(&first, 0, 1, Some(&pose(1.)));
        state.expect(&completion); assert!(state.publish(completion)); assert!(state.consume(&first));
        assert!(state.consume(&restored)); assert!(state.pose(&restored).is_none());
        assert!(!state.publish(HandoffCompletion::rendered(&first, 0, 1, Some(&pose(2.)))));
        let empty = HandoffCompletion::rendered(&restored, 0, 2, None);
        state.expect(&empty); assert!(state.publish(empty)); assert!(state.consume(&restored));
        assert!(state.pose(&restored).is_none());
        let failed = HandoffCompletion::rendered(&restored, 0, 3, Some(&pose(3.)));
        state.expect(&failed); state.retire(); assert!(!state.publish(failed));
        assert!(state.pose(&restored).is_none());
    }
}
