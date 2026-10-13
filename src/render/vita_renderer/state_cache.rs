//! Only the uninterrupted owned opaque pass may retain backend state.
//! A new instance after begin_pass/sky discards ALL prior state. Dropping it
//! before end_pass also invalidates at targets, actors, alpha and Macroquad.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Applications {
    pub pipeline: bool,
    pub depth: bool,
    pub bindings: bool,
    pub uniforms: bool,
}

pub(super) struct Cache<P, B, const N: usize> {
    pipeline: Option<P>,
    depth: Option<bool>,
    bindings: Option<B>,
    // Cull/blend pipelines share a program. Uniforms belong to that program,
    // NOT to the pipeline; otherwise A/B/A can silently reuse stale values.
    uniforms: [Option<[u32; N]>; 2],
}
impl<P: Copy + Eq, B: Copy + Eq, const N: usize> Cache<P, B, N> {
    pub fn new() -> Self {
        Self { pipeline: None, depth: None, bindings: None, uniforms: [None; 2] }
    }
    pub fn observe(&mut self, pipeline: P, low_program: bool, depth: bool,
                   bindings: B, uniforms: [u32; N]) -> Applications {
        let pipeline_changed = self.pipeline != Some(pipeline);
        let program = usize::from(low_program);
        let result = Applications {
            pipeline: pipeline_changed,
            depth: self.depth != Some(depth),
            // Bindings also establish attributes and sampler locations for the
            // current program/layout, even if the buffer/texture IDs match.
            bindings: pipeline_changed || self.bindings != Some(bindings),
            uniforms: self.uniforms[program] != Some(uniforms),
        };
        self.pipeline = Some(pipeline);
        self.depth = Some(depth);
        self.bindings = Some(bindings);
        self.uniforms[program] = Some(uniforms);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_state_decisions_preserve_program_binding_and_uniform_ownership() {
        let mut c = Cache::new();
        let u = [0_f32.to_bits(), f32::from_bits(0x7fc00001).to_bits()];
        let all = Applications { pipeline: true, depth: true, bindings: true, uniforms: true };
        let none = Applications { pipeline: false, depth: false, bindings: false, uniforms: false };
        assert_eq!(c.observe(1, false, true, (10, 20, 30, 40), u), all);
        assert_eq!(c.observe(1, false, true, (10, 20, 30, 40), u), none);
        // Same program on another actual pipeline: bindings must be re-applied.
        let p = c.observe(2, false, true, (10, 20, 30, 40), u);
        assert!(p.pipeline && p.bindings && !p.depth && !p.uniforms);
        let changed = [(-0_f32).to_bits(), u[1]];
        assert!(c.observe(2, false, true, (10, 20, 30, 40), changed).uniforms);
        assert!(c.observe(1, false, true, (10, 20, 30, 40), u).uniforms);
        // Different programs retain independent uniform storage.
        assert!(c.observe(3, true, true, (10, 20, 30, 40), changed).uniforms);
        assert!(!c.observe(1, false, true, (10, 20, 30, 40), u).uniforms);
        for bindings in [(11,20,30,40), (11,21,30,40), (11,21,31,40), (11,21,31,41)] {
            assert!(c.observe(1, false, true, bindings, u).bindings);
        }
        assert!(c.observe(1, false, false, (11,21,31,41), u).depth);
        assert!(c.observe(1, false, false, (11,21,31,41), [u[0],0x7fc00002]).uniforms);
        // All interruption sites end the cache's lifetime.
        for _ in ["sky", "world alpha", "actor", "target", "Macroquad", "frame"] {
            assert_eq!(Cache::new().observe(1, false, true, (10,20,30,40), u), all);
        }
    }

    #[test]
    fn cache_replays_identical_draw_state_across_mixed_programs() {
        let mut c = Cache::new();
        let (mut pipeline, mut depth, mut bindings) = (None, None, None);
        let mut uniforms = [None; 2];
        let mut applications = [0; 4];
        let mut draws = 0;
        for (p, low, b, u) in [
            (1,false,10,[0,1]), (1,false,10,[0,1]), (2,false,10,[2,1]),
            (1,false,11,[0,1]), (3,true,11,[3,1]), (1,false,11,[0,1]),
            (3,true,12,[3,1]), (3,true,12,[4,1]),
        ] {
            let plan = c.observe(p, low, true, b, u);
            if plan.pipeline { pipeline = Some(p); applications[0] += 1; }
            if plan.depth { depth = Some(true); applications[1] += 1; }
            if plan.bindings { bindings = Some(b); applications[2] += 1; }
            if plan.uniforms { uniforms[usize::from(low)] = Some(u); applications[3] += 1; }
            assert_eq!((pipeline,depth,bindings,uniforms[usize::from(low)]),
                       (Some(p),Some(true),Some(b),Some(u)));
            draws += 1;
        }
        assert_eq!(draws, 8);
        assert!(applications.iter().all(|n| *n < draws));
    }
}
