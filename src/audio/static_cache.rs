//! Main-thread, exact per-ray immutable acoustic trace cache.
//! The only trace operation accepted here is StaticGeometry::sweep: point extents
//! (+0 bits), no ignored entity, and the solid mask baked into the immutable World
//! at construction. Scene identity therefore also identifies its mask/tree/hulls.
//! Mutable completion is deliberately outside the cached value.
use crate::collision::{StaticGeometry, Trace, World};
use macroquad::prelude::Vec3;
use std::sync::Arc;

const SETS: usize = 128;
const WAYS: usize = 4;
#[derive(Clone, Copy)]
struct Entry { bits: [u32; 9], trace: Trace, valid: bool }
impl Entry {
    const EMPTY: Self = Self { bits: [0;9], trace: Trace {
        fraction: 1., normal: Vec3::ZERO, start_solid: false, all_solid: false,
    }, valid: false };
}
pub(super) struct Cache {
    // Pin the old allocation until identity comparison: allocator address reuse
    // cannot turn a different scene into a hit, even if Audio::load was skipped.
    geometry: Option<Arc<StaticGeometry>>,
    entries: [[Entry; WAYS]; SETS],
    next: [usize; SETS],
    #[cfg(test)]
    pub hits: u64,
    #[cfg(test)]
    pub misses: u64,
}
impl Cache {
    pub(super) const fn new() -> Self {
        Self { geometry: None, entries: [[Entry::EMPTY;WAYS];SETS], next: [0;SETS],
            #[cfg(test)] hits: 0, #[cfg(test)] misses: 0 }
    }
    pub(super) fn reset(&mut self) { self.reset_for("explicit"); }
    pub(super) fn reset_for(&mut self, reason:&str) {
        let evidence=crate::cache_qualification::enabled().then(|| (
            self.geometry.as_ref().map_or(0,|g|Arc::as_ptr(g) as usize),
            self.entries.iter().flatten().filter(|e|e.valid).count(),
            self.geometry.as_ref().map_or(0,Arc::strong_count)));
        self.entries.fill([Entry::EMPTY;WAYS]);
        self.next.fill(0);
        self.geometry = None;
        if let Some((scene,before,refs))=evidence {
            let after=self.entries.iter().flatten().filter(|e|e.valid).count();
            crate::cache_qualification::reset(reason,scene,before,after,self.geometry.is_some(),refs);
        }
    }
    pub(super) fn begin(&mut self, world: &World) {
        if self.geometry.as_ref().is_none_or(|g| Arc::as_ptr(g) as usize != world.acoustic_scene_key()) {
            self.reset_for("scene-identity");
            self.geometry = Some(world.audio_geometry());
        }
    }
    pub(super) fn sweep(&mut self, world: &World, start: Vec3, end: Vec3) -> Trace {
        let _census = crate::phase_census::sweep();
        // Check per query as well as per frame, making cross-scene misuse safe.
        self.begin(world);
        let bits = [start.x.to_bits(),start.y.to_bits(),start.z.to_bits(),
            end.x.to_bits(),end.y.to_bits(),end.z.to_bits(),0,0,0];
        let hash = bits.iter().fold(2166136261u32, |h,b| (h ^ b).wrapping_mul(16777619));
        // Fold high bits: endpoint floats often share their low mantissa bits.
        let set = ((hash ^ (hash >> 16)) as usize) & (SETS-1);
        let entries = &mut self.entries[set];
        let mut evicted = false;
        let (trace,hit) = if let Some(entry) = entries.iter().find(|e| e.valid && e.bits == bits) {
            (entry.trace,true)
        } else {
            let trace = self.geometry.as_ref().unwrap().sweep(start,end);
            let at = entries.iter().position(|e| !e.valid).unwrap_or(self.next[set]);
            evicted = entries[at].valid;
            entries[at] = Entry {bits,trace,valid:true};
            self.next[set] = (at+1)%WAYS;
            (trace,false)
        };
        #[cfg(test)] { if hit {self.hits+=1;} else {self.misses+=1;} }
        crate::phase_census::cache_result(hit,evicted);
        crate::cache_qualification::result(hit,evicted);
        if hit && crate::cache_qualification::should_check() {
            let fresh=self.geometry.as_ref().unwrap().sweep(start,end);
            crate::cache_qualification::checked(crate::cache_qualification::trace_equal(trace,fresh));
        }
        world.audio_complete(trace,start,end)
    }
}

#[cfg(test)]
#[path = "static_cache_tests.rs"]
mod tests;
