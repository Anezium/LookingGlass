use crate::{input::Input, preferences::Preferences};

/// The owner's control context for one HUD pass.
pub(crate) struct HudView {
    pub(crate) using_pad: bool,
    pub(crate) handheld: bool,
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<HudView>();
};

impl HudView {
    pub(crate) fn capture(input: &Input) -> Self {
        Self {
            using_pad: input.using_pad,
            handheld: Preferences::vita_emulation(),
        }
    }

    pub(crate) fn desktop_pad(&self) -> bool {
        self.using_pad && !self.handheld
    }

    pub(crate) fn label(&self, prefs: &Preferences, action: &str) -> String {
        // The shared resolver only reads using_pad from Input.
        let mut input = Input::default();
        input.using_pad = self.using_pad;
        input.label_with(prefs, action, self.handheld)
    }

    pub(crate) fn film_skip_label(&self, prefs: &Preferences) -> String {
        if self.handheld {
            format!("{} / Cross", self.label(prefs, "Enter"))
        } else if self.desktop_pad() {
            "A".into()
        } else {
            self.label(prefs, "Enter")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::BINDINGS;

    #[test]
    fn labels_match_input_for_every_binding() {
        let mut rebound = Preferences::default();
        for i in 0..BINDINGS.len() {
            rebound.bindings[i] = "Tab".into();
            rebound.pad_bindings[i] = if i % 2 == 0 { "None" } else { "B" }.into();
        }
        let mut vita = Preferences::default();
        vita.apply_vita_preset();

        for prefs in [Preferences::default(), vita, rebound] {
            for using_pad in [false, true] {
                let mut input = Input::default();
                input.using_pad = using_pad;
                for handheld in [false, true] {
                    let view = HudView { using_pad, handheld };
                    for action in BINDINGS.iter().map(|&(_, key)| key).chain(["unlisted"]) {
                        assert_eq!(
                            view.label(&prefs, action),
                            input.label_with(&prefs, action, handheld),
                            "{action}, pad={using_pad}, handheld={handheld}"
                        );
                    }
                }

                // Run the suite with and without Vita controls to exercise both live modes.
                let captured = HudView::capture(&input);
                assert_eq!(captured.desktop_pad(), input.desktop_pad());
                assert_eq!(captured.film_skip_label(&prefs), input.film_skip_label(&prefs));
                for &(_, action) in BINDINGS {
                    assert_eq!(captured.label(&prefs, action), input.label(&prefs, action));
                }
            }
        }
    }
}
