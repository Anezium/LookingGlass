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
}
impl HudInput {
    /// Resident payload capacity, excluding allocator headers. HUD stats contain
    /// no collected/loot trees. Saves are deliberately outside this byte count.
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.prompt.as_ref().map_or(0, String::capacity)
            + self.objective.as_ref().map_or(0, String::capacity)
            + self.fades.capacity() * std::mem::size_of::<Color>()
            + self.subtitle.as_ref().map_or(0, |s| s.text.capacity() + s.speaker.capacity())
    }
}
