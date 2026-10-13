//! Exact payload decisions for buffers and the uninterrupted LG actor program.
use macroquad::prelude::Vertex;

pub(in crate::render_fx) fn vertices_equal(a: &[Vertex], b: &[Vertex]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| {
        a.position.to_array().map(f32::to_bits) == b.position.to_array().map(f32::to_bits)
            && a.uv.to_array().map(f32::to_bits) == b.uv.to_array().map(f32::to_bits)
            && a.color == b.color && a.normal.to_array().map(f32::to_bits) == b.normal.to_array().map(f32::to_bits)
    })
}

pub(super) struct UploadPlan { pub vertex: bool, pub index: bool, pub full_hit: bool }
impl UploadPlan {
    pub fn new(full_cache: bool, index_cache: bool, same_vertex: bool, same_index: bool) -> Self {
        let full_hit = full_cache && same_vertex && same_index;
        Self { vertex: !full_hit, index: !(full_hit || index_cache && same_index), full_hit }
    }
}

#[derive(Default)]
pub(super) struct UniformCache<const N: usize> { last: Option<[f32; N]> }
impl<const N: usize> UniformCache<N> {
    pub fn observe(&mut self, data: &[f32; N]) -> bool {
        let same = self.last.as_ref().is_some_and(|last|
            last.iter().zip(data).all(|(a,b)| a.to_bits() == b.to_bits()));
        self.last = Some(*data);
        same
    }
    pub fn interrupt(&mut self) { self.last = None; }
}

/// This cache owns an uninterrupted private program/pass. A pipeline change
/// also reapplies bindings, covering the program's vertex layout; all buffer and
/// image handles belong in B. No geometry/draw/order decisions are cached.
pub(super) struct SubmitCache<P, B, const N: usize> {
    pipeline: Option<P>,
    bindings: Option<B>,
    uniforms: UniformCache<N>,
}
impl<P, B, const N: usize> Default for SubmitCache<P, B, N> {
    fn default() -> Self {
        Self { pipeline: None, bindings: None, uniforms: UniformCache::default() }
    }
}
impl<P: PartialEq, B: PartialEq, const N: usize> SubmitCache<P, B, N> {
    pub fn observe(&mut self, pipeline: P, bindings: B, uniforms: &[f32; N]) -> [bool; 3] {
        let state = self.pipeline.as_ref() != Some(&pipeline);
        let binding = state || self.bindings.as_ref() != Some(&bindings);
        let uniform = !self.uniforms.observe(uniforms) || state;
        self.pipeline = Some(pipeline);
        self.bindings = Some(bindings);
        [state, binding, uniform]
    }
    pub fn interrupt(&mut self) {
        self.pipeline = None;
        self.bindings = None;
        self.uniforms.interrupt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::*;
    #[test]
    fn owned_submit_state_preserves_draw_output_and_invalidates_complete_keys() {
        let mut cache=SubmitCache::<(usize,bool,bool),(usize,usize,[usize;3]),147>::default();
        let mut actual=(None,None,None);
        let mut optimized=Vec::new();let mut reference=Vec::new();
        let mut calls=[0;3];
        for i in 0..300 {
            if [0,31,32,100,200].contains(&i) {
                cache.interrupt();actual=(None,None,None); // world, target, view/pass boundaries
            }
            let pipeline=(i/50,i/25%2==0,i<150);
            let binding=(i/7,i/9,[i/11,i/13,i/17]);
            let mut u=[0_f32;147];
            u[(i/2)%147]=f32::from_bits(0x80000000+((i/2)%3) as u32);
            let apply=cache.observe(pipeline,binding,&u);
            if apply[0] {actual.0=Some(pipeline);calls[0]+=1;}
            if apply[1] {actual.1=Some(binding);calls[1]+=1;}
            if apply[2] {actual.2=Some(u.map(f32::to_bits));calls[2]+=1;}
            optimized.push((i,actual));reference.push((i,(Some(pipeline),Some(binding),Some(u.map(f32::to_bits)))));
        }
        assert_eq!(optimized,reference);assert!(calls.iter().all(|&c|c<300));
        let pipeline=(0,false,true);let binding=(1,2,[3,4,5]);let u=[0.;147];
        cache.interrupt();assert_eq!(cache.observe(pipeline,binding,&u),[true;3]);
        assert_eq!(cache.observe(pipeline,binding,&u),[false;3]);
        for index in 0..147 {
            let mut changed=u;changed[index]=-0.;
            assert!(cache.observe(pipeline,binding,&changed)[2]);
            cache.observe(pipeline,binding,&u);
        }
        for binding in [(2,2,[3,4,5]),(1,3,[3,4,5]),(1,2,[4,4,5]),(1,2,[3,5,5]),(1,2,[3,4,6])] {
            cache.interrupt();cache.observe(pipeline,(1,2,[3,4,5]),&u);
            assert_eq!(cache.observe(pipeline,binding,&u),[false,true,false]);
        }
        assert_eq!(cache.observe((0,true,true),binding,&u),[true;3]);
        assert_eq!(cache.observe((0,true,false),binding,&u),[true;3]);
    }
    #[test]
    fn index_only_cache_never_skips_changed_payloads_or_dynamic_vertices() {
        for full in [false, true] { for index in [false, true] {
            for vertex_same in [false, true] { for index_same in [false, true] {
                let plan = UploadPlan::new(full, index, vertex_same, index_same);
                assert!(plan.vertex || full && vertex_same && index_same);
                assert!(plan.index || index_same && (index || full && vertex_same));
                if !full { assert!(plan.vertex); }
                if !full && !index { assert!(plan.index); }
            }}
        }}
        let old = vec![0_u16, 1, 2];
        for changed in [vec![0, 2, 1], vec![0, 1], vec![0, 1, 2, 0]] {
            assert!(UploadPlan::new(false, true, true, old == changed).index);
        }
        assert!(UploadPlan::new(false, true, true, Vec::<u16>::new() == old).index);
    }
    #[test]
    fn vertex_payload_comparison_preserves_signed_zero_and_nan_bits() {
        let a = Vertex { position: Vec3::ZERO, uv: Vec2::ZERO, color: [255;4], normal: Vec4::ZERO };
        let mut b = a;
        assert!(vertices_equal(&[a], &[b]));
        b.position.x = -0.; assert!(!vertices_equal(&[a], &[b]));
        b = a; b.uv.y = -0.; assert!(!vertices_equal(&[a], &[b]));
        b = a; b.normal.w = -0.; assert!(!vertices_equal(&[a], &[b]));
        b = a; b.color[0] = 254; assert!(!vertices_equal(&[a], &[b]));
        b = a; b.position.z = f32::from_bits(0x7fc00001);
        assert!(vertices_equal(&[b], &[b]));
        let mut c = b; c.position.z = f32::from_bits(0x7fc00002);
        assert!(!vertices_equal(&[b], &[c]));
        assert!(!vertices_equal(&[a], &[]));
    }
    #[test]
    fn uniform_cache_requires_exact_values_and_reapplies_after_interruption() {
        let mut cache = UniformCache::default();
        let mut u = [1., 0., 0.5, 3.];
        assert!(!cache.observe(&u)); assert!(cache.observe(&u));
        u[1] = -0.; assert!(!cache.observe(&u)); assert!(cache.observe(&u));
        u[0] = 2.; assert!(!cache.observe(&u));
        cache.interrupt(); assert!(!cache.observe(&u)); assert!(cache.observe(&u));
        // A world callback, new view or new frame is an explicit interruption.
        cache.interrupt(); assert!(!cache.observe(&u));
    }
}
