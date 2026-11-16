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
    pub fades: Vec<Color>,
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
            + self.fades.capacity() * std::mem::size_of::<Color>()
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
