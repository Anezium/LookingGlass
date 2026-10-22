//! Observed GL calls only. Raster state starts unknown at each owned submission;
//! sampler values persist per program and are invalidated on link/delete.
use std::collections::BTreeMap;
#[derive(Default)]
pub(crate) struct Cache {
    pub bindings: bool,
    pub raster: bool,
    program: u32,
    track: bool,
    samplers: BTreeMap<(u32,i32),i32>,
    state: [Option<[u32;4]>;16],
}
impl Cache {
    pub fn begin(&mut self, bindings: bool, raster: bool) {
        self.bindings=bindings; self.track=bindings; self.raster=raster; self.state.fill(None);
    }
    pub fn end(&mut self) { self.bindings=false; self.raster=false; self.state.fill(None); }
    pub fn program(&mut self, id:u32) -> bool {
        self.program=id;
        self.raster(0,0,[id,0,0,0])
    }
    pub fn invalidate(&mut self, id:u32) { self.samplers.retain(|(p,_),_|*p!=id); self.state.fill(None); }
    pub fn sampler(&mut self, location:i32, value:i32) -> bool {
        // Observe external program writes too. No assumed global binding state.
        if !self.track {return true;}
        let previous=self.samplers.insert((self.program,location),value);
        !self.bindings || previous!=Some(value)
    }
    pub fn raster(&mut self, kind:u8, key:u32, value:[u32;4]) -> bool {
        if !self.raster { return true; }
        let slot=match kind {
            0=>0, 1=>1, 3=>2, 4=>3, 5=>4, 6=>5, 7=>6,
            2=>match key {0x0B71=>7,0x0C11=>8,0x0B44=>9,0x0BE2=>10,0x0B90=>11,_=>return true},
            _=>return true,
        };
        self.state[slot].replace(value)!=Some(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samplers_survive_frames_but_follow_every_program_write_and_lifetime() {
        let mut c=Cache::default();c.begin(true,true);
        assert!(c.program(7));assert!(!c.program(7));
        assert!(c.sampler(1,0));assert!(!c.sampler(1,0));
        c.end();assert!(c.program(8));assert!(c.sampler(1,1));
        c.begin(true,true);assert!(c.program(7));assert!(!c.sampler(1,0));
        c.end();assert!(c.sampler(1,3));c.begin(true,true);assert!(c.sampler(1,0));
        c.invalidate(7);assert!(c.sampler(1,0));
        assert!(c.raster(1,0,[1,0,0,0]));assert!(!c.raster(1,0,[1,0,0,0]));
        c.end();c.begin(true,true);assert!(c.raster(1,0,[1,0,0,0]));
    }
}
