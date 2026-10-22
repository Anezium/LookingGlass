//! Exact scheduling certificates and adjacent static range coalescing.
use super::*;

pub(super) fn static_opaque(r: &Recipe) -> bool {
    r.opaque && r.stage.blend == Blend::Opaque && r.model.is_none()
        && r.motion.is_none() && r.deform.is_none()
}
#[derive(Clone, Copy, Debug)]
struct Rect { min: Vec2, max: Vec2 }
impl Rect {
    fn disjoint(self, b: Self) -> bool {
        self.max.x < b.min.x || b.max.x < self.min.x || self.max.y < b.min.y || b.max.y < self.min.y
    }
}
fn join_disjoint(a: Rect, b: Rect) -> Option<Rect> {
    a.disjoint(b).then_some(Rect {min:a.min.min(b.min),max:a.max.max(b.max)})
}
fn projected(g: &Group, matrix: Mat4) -> Option<Rect> {
    let mut rect = Rect { min: Vec2::splat(f32::INFINITY), max: Vec2::splat(f32::NEG_INFINITY) };
    for x in [g.min.x,g.max.x] { for y in [g.min.y,g.max.y] { for z in [g.min.z,g.max.z] {
        let p = matrix * vec4(x,y,z,1.);
        // A box crossing the eye plane cannot certify disjoint coverage.
        if !p.is_finite() || p.w <= 0.001 { return None; }
        let p = p.truncate().truncate()/p.w;
        rect.min = rect.min.min(p); rect.max = rect.max.max(p);
    } } }
    // Two full pixels on each side, including scaled/MSAA targets.
    let pad = vec2(4./screen_width().min(320.),4./screen_height().min(180.));
    rect.min -= pad; rect.max += pad;
    Some(rect)
}
#[derive(Clone,Copy)]
struct Item { rank: usize, key: (bool,usize,usize), rect: Option<Rect> }
fn reorder(order: &mut [usize], items: &[Item]) {
    for i in 1..order.len() {
        let mut j=i;
        while j>0 {
            let a=items[order[j-1]]; let b=items[order[j]];
            if a.rank!=b.rank || a.key<=b.key || !a.rect.zip(b.rect).is_some_and(|(a,b)|a.disjoint(b)) {break;}
            order.swap(j-1,j);j-=1;
        }
    }
}
#[derive(Default)]
pub(super) struct Schedule {
    generation: u64,
    input: Vec<usize>,
    order: Vec<usize>,
    scratch: Vec<(f32,usize)>,
}
impl Schedule {
    pub fn apply(&mut self, draws: &mut Vec<(f32,usize)>, groups: &[Group], matrix: Mat4, generation: u64) {
        if self.generation != generation || self.input.iter().copied().ne(draws.iter().map(|d|d.1)) {
            self.generation = generation;
            self.input.clear(); self.input.extend(draws.iter().map(|d|d.1));
            self.order.clear(); self.order.extend(0..draws.len());
            let items=draws.iter().map(|d| {
                let g=&groups[d.1];
                Item {rank:g.recipe.rank,key:(g.recipe.stage.alpha_test==0,g.recipe.cull,g.material_order),
                    rect:static_opaque(&g.recipe).then(||projected(g,matrix)).flatten()}
            }).collect::<Vec<_>>();
            // Every crossed pair has disjoint raster coverage. Rank and blended,
            // animated, deformed or overlapping draws remain ordering fences.
            reorder(&mut self.order,&items);
        }
        count("vita_world_schedule_moved", self.order.iter().enumerate().filter(|(a,b)|*a!=**b).count() as u64);
        self.scratch.clear(); self.scratch.extend_from_slice(draws);
        for (draw,&i) in draws.iter_mut().zip(&self.order) { *draw=self.scratch[i]; }
    }
}

pub(super) fn merge(draws: &mut Vec<Prepared>, groups: &[Group], parameters: &[Parameter], projection: Mat4) {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if std::env::var_os("LOOKING_GLASS_WORLD_PACK_ONLY").is_some() {return;}
    let mut write=0;
    let mut previous_rect=None;
    for read in 0..draws.len() {
        let next=&draws[read]; let g=&groups[next.group];
        let candidate=write>0 && {
            let previous=&draws[write-1]; let p=&groups[previous.group];
            static_opaque(&g.recipe) && static_opaque(&p.recipe)
                && p.recipe.rank==g.recipe.rank && p.recipe.cull==g.recipe.cull
                && p.bindings.index_buffer==g.bindings.index_buffer
                && p.base_element+previous.indices==g.base_element
                && p.recipe.stage.alpha_test==g.recipe.stage.alpha_test
                && previous.texture==next.texture
                && previous.uniforms.as_ref().map_or(parameters[p.parameter].bits,shader::Uniforms::bits)
                    == next.uniforms.as_ref().map_or(parameters[g.parameter].bits,shader::Uniforms::bits)
        };
        // A draw boundary can affect equal-depth coverage/fragment derivatives
        // on the host driver even with identical uniform and vertex bits. Keep
        // all overlapping raster coverage in separate commands as well as in
        // the original primitive order. The union covers EVERY coalesced range.
        let joined=if candidate {
            previous_rect.or_else(||projected(&groups[draws[write-1].group],projection))
                .zip(projected(g,projection)).and_then(|(a,b)|join_disjoint(a,b))
        } else {None};
        if let Some(rect)=joined {
            draws[write-1].indices+=draws[read].indices; previous_rect=Some(rect);
        } else { draws.swap(write,read); write+=1; previous_rect=None; }

    }
    count("vita_world_merged_draws", (draws.len()-write) as u64);
    draws.truncate(write);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coverage_certificates_reject_overlap_and_touching_edges() {
        let a=Rect{min:vec2(0.,0.),max:vec2(1.,1.)};
        assert!(!a.disjoint(a));
        assert!(!a.disjoint(Rect{min:vec2(1.,0.),max:vec2(2.,1.)}));
        assert!(a.disjoint(Rect{min:vec2(1.01,0.),max:vec2(2.,1.)}));
    }
    #[test]
    fn coalescing_keeps_overlap_and_every_previous_range_as_boundaries() {
        let a=Rect{min:vec2(0.,0.),max:vec2(1.,1.)};
        let b=Rect{min:vec2(2.,0.),max:vec2(3.,1.)};
        let c=Rect{min:vec2(0.5,0.),max:vec2(1.5,1.)};
        assert!(join_disjoint(a,a).is_none());
        let combined=join_disjoint(a,b).unwrap();
        assert!(b.disjoint(c));
        assert!(join_disjoint(combined,c).is_none());
        assert!(join_disjoint(combined,Rect{min:vec2(4.,0.),max:vec2(5.,1.)}).is_some());
    }
    #[test]
    fn schedules_preserve_pixels_overlap_ties_blending_and_layer_order() {
        let mut seed=12345u32;
        let mut rand=||{seed=seed.wrapping_mul(1664525).wrapping_add(1013904223);seed};
        let mut moved=0;
        for frame in 0..300 {
            let mut coverage=Vec::new();let mut items=Vec::new();
            for id in 0..80 {
                let x=(rand()%30) as f32;let y=(rand()%30) as f32;
                let rect=Rect{min:vec2(x,y),max:vec2(x+(rand()%5+1) as f32,y+(rand()%5+1) as f32)};
                coverage.push(rect);
                items.push(Item{rank:id/20,key:(false,(rand()%3) as usize,(rand()%10) as usize),
                    rect:if (id+frame)%11==0 {None} else {Some(rect)}});
            }
            let original=(0..items.len()).collect::<Vec<_>>();let mut order=original.clone();
            reorder(&mut order,&items);
            moved+=usize::from(order!=original);
            // Equal-depth opaque writes and alpha blends are deliberately order sensitive.
            let raster=|order:&[usize]| {
                let mut pixels=vec![0u32;40*40];
                for &id in order {
                    let r=coverage[id];
                    for y in r.min.y as usize..=r.max.y as usize {for x in r.min.x as usize..=r.max.x as usize {
                        let pixel=&mut pixels[y*40+x];
                        *pixel=if items[id].rect.is_none() {(*pixel+id as u32+1)/2} else {id as u32+1};
                    }}
                }
                pixels
            };
            assert_eq!(raster(&original),raster(&order));
            let positions=(0..items.len()).map(|id|order.iter().position(|&i|i==id).unwrap()).collect::<Vec<_>>();
            for a in 0..items.len() {for b in a+1..items.len() {
                if items[a].rank!=items[b].rank || !items[a].rect.zip(items[b].rect).is_some_and(|(a,b)|a.disjoint(b)) {
                    assert!(positions[a]<positions[b]);
                }
            }}
        }
        assert!(moved>250);
    }

}
