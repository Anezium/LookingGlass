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
    pub visit: u64,
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
    /// Reject the complete batch before drawing; fall back serially, never partly.
    pub fn valid(&self) -> bool {
        self.level.len() <= 16384 && self.bytes() <= 4 * 1024 * 1024
            && self.level.iter().all(HudCommand::valid)
    }

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

#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) enum Primitive {
    Dialog(Rect),
    Label(String, f32, f32, f32, Color),
    Rectangle(f32, f32, f32, f32, Color),
    Boss(String, f32),
}
/// Numeric resource identity only. Ui and all texture/font handles remain in
/// the graphics owner registry; this command can cross the simulation boundary.
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct HudCommand {
    ui: u64,
    primitive: Primitive,
}
impl HudCommand {
    pub fn valid(&self) -> bool { crate::ui::Ui::resolve_resource(self.ui).is_some() }
    pub fn draw(&self) {
        let ui = crate::ui::Ui::resolve_resource(self.ui).expect("validated resident HUD resource");
        match &self.primitive {
            Primitive::Dialog(r) => ui.dialog(*r),
            Primitive::Label(t, x, y, size, color) => ui.label(t, *x, *y, *size, *color),
            Primitive::Rectangle(x, y, w, h, c) => draw_rectangle(*x, *y, *w, *h, *c),
            Primitive::Boss(t, remaining) => crate::hud::boss_meter(&ui, t, *remaining),
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
    ui: u64,
    out: &'a mut Vec<HudCommand>,
    bytes: usize,
}
impl<'a> Painter<'a> {
    pub fn new(ui: &std::rc::Rc<crate::ui::Ui>, out: &'a mut Vec<HudCommand>) -> Self {
        Self::for_resource(ui.resource_id(), out)
    }

    /// Accept a sampled resource id without consulting the owner UI registry.
    pub(crate) fn for_resource(ui: u64, out: &'a mut Vec<HudCommand>) -> Self {
        let bytes = out.len()*std::mem::size_of::<HudCommand>()
            + out.iter().map(HudCommand::text_capacity).sum::<usize>();
        Self { ui, out, bytes }
    }
    fn reject(&mut self) {
        if self.out.len() <= 16384 && self.out.last().is_none_or(|c| c.ui != 0) {
            self.out.push(HudCommand { ui: 0, primitive: Primitive::Rectangle(0.,0.,0.,0.,BLACK) });
        }
    }
    fn room(&mut self, text: usize) -> bool {
        if self.out.len() >= 16384 || self.out.last().is_some_and(|c| c.ui == 0)
            || text > (4*1024*1024usize).saturating_sub(self.bytes + std::mem::size_of::<HudCommand>()) {
            self.reject(); false
        } else { true }
    }
    fn push(&mut self, primitive: Primitive) {
        if !self.room(0) { return; }
        let command = HudCommand { ui: self.ui, primitive };
        self.bytes += std::mem::size_of::<HudCommand>() + command.text_capacity();
        self.out.push(command);
    }
    pub fn dialog(&mut self, r: Rect) { self.push(Primitive::Dialog(r)); }
    pub fn label(&mut self, t: &str, x: f32, y: f32, size: f32, c: Color) { if self.room(t.len()) { self.push(Primitive::Label(t.into(), x, y, size, c)); } }
    pub fn rectangle(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) { self.push(Primitive::Rectangle(x, y, w, h, c)); }
    pub fn boss(&mut self, t: &str, remaining: f32) { if self.room(t.len()) { self.push(Primitive::Boss(t.into(), remaining)); } }
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

/// Numeric particle inputs. Steam qualifies emitter slots with a renderer visit.
#[derive(Clone, Copy)]
pub(crate) struct ParticleInput {
    pub emitter: usize, pub position: Vec3, pub age: f32,
    pub life: f32, pub size: f32, pub roll: f32,
}
#[derive(Default)]
pub(crate) struct DynamicInput {
    pub alice_feet:Vec3, pub alice_visible:bool, pub alice_shadow:bool, pub alice_ghost:bool, pub alice_effects:bool,
    pub view: Mat4, pub environment_clock: f32, pub pickup_clock: f32,
    pub fog: Vec4, pub liquid: Vec4,
    pub transforms: Vec<(usize, Vec3, Quat)>,
}
impl DynamicInput {
    pub(crate) fn finish_view(&mut self, view: Mat4, atmosphere: &crate::environment::Atmosphere) {
        self.view = view;
        self.fog = atmosphere.distance;
        self.liquid = atmosphere.liquid;
    }


    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.transforms.len() * std::mem::size_of::<(usize, Vec3, Quat)>()
    }
    pub fn capacity_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.transforms.capacity() * std::mem::size_of::<(usize, Vec3, Quat)>()
    }
}

/// A renderer visit has no relationship to an address or a saved game identifier.
/// Rebuilding even the same map creates a fresh namespace for completions.
pub(crate) struct RenderVisit(u64);
impl RenderVisit { pub(crate) fn id(&self) -> u64 { self.0 } }
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
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + [&self.pending, &self.acknowledged].into_iter()
            .filter_map(|c| c.as_ref()).filter_map(|c| c.pose.as_ref())
            .map(|p| p.local.len()*std::mem::size_of::<crate::skeletal::Transform>()).sum::<usize>()
    }
    pub fn retire(&mut self) { *self = Self::default(); }
    pub fn expect(&mut self, visit: &RenderVisit, slot: usize, frame: u64) {
        if self.visit != visit.0 { self.retire(); }
        self.visit = visit.0;
        self.expected = Some(HandoffTag { visit: visit.0, slot, frame });
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
        crate::cinematic::ActorPose { rendered: None, local: vec![transform], transform, scale: 1. }
    }
    #[test]
    fn handoff_exact_completion_and_last_draw_survive_pause_and_skip() {
        let visit = RenderVisit::default();
        let mut state = HandoffReturn::default();
        let mut source = pose(4.);
        let completion = HandoffCompletion::rendered(&visit, 3, 10, Some(&source));
        state.expect(&visit, 3, 10);
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
        state.expect(&visit, 3, 11); assert!(state.publish(next)); assert!(state.consume(&visit));
        assert_eq!(state.pose(&visit).unwrap().local[0].translation.x, 7.);
    }
    #[test]
    fn handoff_restore_failed_extraction_and_no_pose_retire_old_output() {
        let first = RenderVisit::default(); let restored = RenderVisit::default();
        let mut state = HandoffReturn::default();
        let completion = HandoffCompletion::rendered(&first, 0, 1, Some(&pose(1.)));
        state.expect(&first, 0, 1); assert!(state.publish(completion)); assert!(state.consume(&first));
        assert!(state.consume(&restored)); assert!(state.pose(&restored).is_none());
        assert!(!state.publish(HandoffCompletion::rendered(&first, 0, 1, Some(&pose(2.)))));
        let empty = HandoffCompletion::rendered(&restored, 0, 2, None);
        state.expect(&restored, 0, 2); assert!(state.publish(empty)); assert!(state.consume(&restored));
        assert!(state.pose(&restored).is_none());
        let failed = HandoffCompletion::rendered(&restored, 0, 3, Some(&pose(3.)));
        state.expect(&restored, 0, 3); state.retire(); assert!(!state.publish(failed));
        assert!(state.pose(&restored).is_none());
    }
}

thread_local! {
    static HANDOFF_RENDER_STAMP: std::cell::Cell<Option<(u64, u64)>> = const { std::cell::Cell::new(None) };
}
pub(crate) fn handoff_render_begin(visit: &RenderVisit, frame: u64) {
    HANDOFF_RENDER_STAMP.with(|stamp| stamp.set(Some((visit.0, frame))));
}
pub(crate) fn handoff_render_end() { HANDOFF_RENDER_STAMP.with(|stamp| stamp.set(None)); }
pub(crate) fn handoff_render_stamp() -> Option<(u64, u64)> { HANDOFF_RENDER_STAMP.with(|stamp| stamp.get()) }

/// Ordered numeric wave/impact descriptions. Geometry remains renderer work.
pub(crate) enum WaveInput { Boojum(Vec3,Vec3,f32), Ladybug(Vec3,f32) }
impl WaveInput {
    pub(crate) fn draw(&self) {
        match *self {
            Self::Boojum(position,direction,age)=>{
                let right=direction.cross(Vec3::Z).normalize_or_zero();let up=right.cross(direction);
                let radius=4.+age*40.;let color=Color::new(0.65,0.85,1.,(1.-age)*0.6);
                for i in 0..16 {let a=i as f32*std::f32::consts::TAU/16.;let b=(i+1) as f32*std::f32::consts::TAU/16.;
                    crate::render_fx::line(position+(right*a.cos()+up*a.sin())*radius,position+(right*b.cos()+up*b.sin())*radius,color);}
            },
            Self::Ladybug(position,age)=>{
                let r=12.+age*250.;let color=Color::new(1.,0.65,0.15,(1.-age/0.6)*0.8);
                for i in 0..24 {let a=i as f32*std::f32::consts::TAU/24.;let c=(i+1) as f32*std::f32::consts::TAU/24.;
                    crate::render_fx::legacy_line(position+vec3(a.cos(),a.sin(),0.)*r,position+vec3(c.cos(),c.sin(),0.)*r,color);
                    crate::render_fx::legacy_line(position,position+vec3(a.cos(),a.sin(),((i%5) as f32-2.)*0.4)*r*0.5,color);}
            }
        }
    }
}

#[cfg(test)]
mod hud_ownership_tests {
    use super::*;
    #[test]
    fn hud_payload_is_send_and_stale_resources_are_rejected() {
        fn assert_send<T: Send>() {} assert_send::<HudInput>(); assert_send::<HudCommand>();
        assert!(!HudCommand { ui: 0, primitive: Primitive::Boss("test".into(), 0.5) }.valid());
        assert!(!HudCommand { ui: u64::MAX, primitive: Primitive::Dialog(Rect::default()) }.valid());
    }
}

/// Real-resource control: packets exercise each primitive and old identities
/// cannot resolve after their graphics owner is retired/recreated.
#[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
pub(crate) async fn hud_resource_check(assets: &mut crate::assets::Assets) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::load(assets)?;
    let rect = Rect::new(32., 32., 200., 100.);
    let mut commands = Vec::new();
    let mut painter = Painter::new(&ui, &mut commands);
    painter.dialog(rect); painter.label("Owned HUD", 64., 80., 18., WHITE);
    painter.rectangle(42., 180., 80., 8., RED); painter.boss("Test boss", 0.5);
    assert!(commands.iter().all(HudCommand::valid));
    let mut reference = None;
    for owned in [false, true] {
        clear_background(BLACK); set_default_camera();
        if owned { for command in &commands { command.draw(); } }
        else {
            ui.dialog(rect); ui.label("Owned HUD", 64., 80., 18., WHITE);
            draw_rectangle(42., 180., 80., 8., RED); crate::hud::boss_meter(&ui, "Test boss", 0.5);
        }
        let image = get_screen_data();
        anyhow::ensure!(image.bytes.chunks_exact(4).filter(|p| p[..3] != [0,0,0]).count() > 20, "Empty HUD control");
        if let Some(expected) = &reference { anyhow::ensure!(*expected == image.bytes, "HUD pixels differ"); }
        else { reference = Some(image.bytes); }
        next_frame().await;
    }
    let mut bounded = Vec::new();
    let mut painter = Painter::new(&ui, &mut bounded);
    for _ in 0..20000 { painter.rectangle(0.,0.,1.,1.,WHITE); }
    anyhow::ensure!(bounded.len() == 16385 && bounded.iter().any(|c| !c.valid()), "HUD count not bounded/rejected");
    let mut oversized = Vec::new();
    Painter::new(&ui, &mut oversized).label(&"x".repeat(4*1024*1024+1),0.,0.,12.,WHITE);
    anyhow::ensure!(oversized.len() == 1 && !oversized[0].valid() && oversized[0].text_capacity() == 0, "Oversized HUD text copied");
    let old = ui.resource_id(); drop(ui);
    anyhow::ensure!(commands.iter().all(|c| !c.valid()), "Retired UI accepted");
    let replacement = crate::ui::Ui::load(assets)?;
    anyhow::ensure!(replacement.resource_id() != old && commands.iter().all(|c| !c.valid()), "Recreated UI accepted old command");
    println!("PASS snapshot HUD: exact primitives, retired/recreated resource rejection");
    Ok(())
}
