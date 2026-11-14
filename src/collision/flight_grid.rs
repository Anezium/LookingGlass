//! Conservative empty-space certificates for short axial flight edges.
//! Occupied cells never imply collision. Only an entirely empty cell range proves a static miss.
use super::*;
#[derive(Debug)]
pub(super) struct FlightGrid { origin: [i32;3], dims: [usize;3], bits: Vec<u64> }
impl FlightGrid {
    const CELL: f64 = 64.;
    const LIMIT: f32 = 1_000_000.;
    pub(super) fn build(hulls: &[Hull]) -> Option<Self> {
        if hulls.is_empty() || hulls.iter().any(|h| !h.bounds.min.is_finite() || !h.bounds.max.is_finite()
            || h.bounds.min.cmpgt(h.bounds.max).any() || h.bounds.min.abs().max_element()>Self::LIMIT || h.bounds.max.abs().max_element()>Self::LIMIT) { return None; }
        // Query half extents are at most 32, with SKIN=1/32. Padding 64 also covers float
        // rounding at the admitted coordinate limit (ulp <= 1/16). Quantization here is f64.
        let range=|h:&Hull| {
            let lo=h.bounds.min.to_array().map(|x|((f64::from(x)-64.)/Self::CELL).floor() as i32);
            let hi=h.bounds.max.to_array().map(|x|((f64::from(x)+64.)/Self::CELL).floor() as i32);
            (lo,hi)
        };
        let mut origin=[i32::MAX;3];let mut end=[i32::MIN;3];
        for h in hulls { let (lo,hi)=range(h); for axis in 0..3 {origin[axis]=origin[axis].min(lo[axis]);end[axis]=end[axis].max(hi[axis]);} }
        let dims=std::array::from_fn(|axis|(end[axis]-origin[axis]+1) as usize);
        let cells=dims.iter().try_fold(1usize,|n,&d|n.checked_mul(d))?;
        if cells>16*1024*1024 {return None;}
        let mut out=Self{origin,dims,bits:vec![0;cells.div_ceil(64)]};
        let mut work=0usize;
        for h in hulls {
            let (lo,hi)=range(h);
            let lo=std::array::from_fn::<_,3,_>(|axis|(lo[axis]-origin[axis]) as usize);
            let hi=std::array::from_fn::<_,3,_>(|axis|(hi[axis]-origin[axis]) as usize);
            for x in lo[0]..=hi[0] {for y in lo[1]..=hi[1] {
                let a=out.index(x,y,lo[2]);let b=out.index(x,y,hi[2]);
                work+=b/64-a/64+1;
                if work>2_000_000 {return None;}
                if a/64==b/64 {out.bits[a/64]|=(u64::MAX<<(a%64))&(u64::MAX>>(63-b%64));}
                else {out.bits[a/64]|=u64::MAX<<(a%64);out.bits[b/64]|=u64::MAX>>(63-b%64);
                    out.bits[a/64+1..b/64].fill(u64::MAX);}
            }}
        }
        #[cfg(not(test))]
        println!("[lg/flight-grid] bytes={} cells={cells} word_writes={work}",out.bytes());
        Some(out)
    }
    pub(super) fn bytes(&self)->usize {self.bits.capacity()*8}
    fn index(&self,x:usize,y:usize,z:usize)->usize {(x*self.dims[1]+y)*self.dims[2]+z}
    pub(super) fn proves_clear(&self,a:Vec3,b:Vec3,half:Vec3)->bool {
        if !a.is_finite() || !b.is_finite() || !half.is_finite() || half.cmplt(Vec3::ZERO).any()
            || half.cmpgt(Vec3::splat(32.)).any() || a.abs().max_element()>Self::LIMIT || b.abs().max_element()>Self::LIMIT
            || (a-b).length_squared()>160.*160.
            || (0..3).filter(|&axis|a[axis]!=b[axis]).count()>1 {return false;}
        // Power-of-two scaling is exact apart from tiny underflow near zero. Keep
        // the original sign for that case; avoid per-query double/libm on the Vita.
        let cell=|x:f32| {let scaled=x*(1./64.);let i=scaled as i32;
            i-i32::from(scaled<i as f32 || (i==0 && x<0.))};
        let mut lo=[0usize;3];let mut hi=[0usize;3];
        for axis in 0..3 {
            let first=cell(a[axis].min(b[axis]))-self.origin[axis];
            let last=cell(a[axis].max(b[axis]))-self.origin[axis];
            if last<0 || first>=self.dims[axis] as i32 {return true;}
            lo[axis]=first.max(0) as usize;hi[axis]=last.min(self.dims[axis] as i32-1) as usize;
        }
        for x in lo[0]..=hi[0] {for y in lo[1]..=hi[1] {for z in lo[2]..=hi[2] {
            let i=self.index(x,y,z);if self.bits[i/64]&(1u64<<(i%64))!=0 {return false;}
        }}}
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_cell_certificates_never_skip_a_full_static_hit() {
        let world=World::fixture(&[(vec3(-800.,-800.,-50.),vec3(800.,800.,0.)),
            (vec3(100.,-200.,0.),vec3(130.,200.,300.))]);
        let grid=FlightGrid::build(&world.static_geometry.hulls).unwrap();
        let mut seed=0x2948AB12u32;
        let mut random=||{seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;(seed%20001) as f32/10.-1000.};
        let mut free=0;
        for trial in 0..8000 {
            let a=vec3(random(),random(),random());let mut b=a;
            b[trial%3]+=random().clamp(-160.,160.);
            let half=[Vec3::ZERO,Vec3::splat(24.),Vec3::splat(32.)][trial%3];
            if grid.proves_clear(a,b,half) {
                let t=world.sweep(a,b,half);assert!(!t.start_solid && t.fraction==1.,"{a} {b} {half}");free+=1;
            }
        }
        assert!(free>3000,"exercise actual empty certificates: {free}");
        for x in [-128.,-64.,-0.0001,-f32::from_bits(1),0.,f32::from_bits(1),0.0001,64.,128.] {
            let a=vec3(x,0.,32.);let b=a+Vec3::X*80.;
            if grid.proves_clear(a,b,Vec3::splat(32.)) {let t=world.sweep(a,b,Vec3::splat(32.));assert!(!t.start_solid && t.fraction==1.);}
        }
        assert!(!grid.proves_clear(Vec3::ZERO,Vec3::X,Vec3::splat(33.)));
        assert!(!grid.proves_clear(Vec3::NAN,Vec3::ZERO,Vec3::ZERO));
        assert!(!grid.proves_clear(Vec3::ZERO,Vec3::ONE,Vec3::ZERO));
    }
}
