//! Bounded deterministic flight routing using swept body clearance.
use crate::collision::World;
use macroquad::prelude::*;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
};

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Navigator {
    pub path: Vec<Vec3>,
    goal: Vec3,
    retry: f32,
}
fn memo_enabled() -> bool {
    #[cfg(feature="vita_renderer")]
    { crate::render::vita_renderer::config().sim_flight_memo }
    #[cfg(not(feature="vita_renderer"))]
    { false }
}
fn clear(world: &World, a: Vec3, b: Vec3, half: Vec3) -> bool {
    !(if memo_enabled() { world.flight_blocked(a, b, half) } else { world.sweep_blocked(a, b, half) })
        && (0..=4).all(|i| world.liquid_at(a.lerp(b, i as f32 / 4.) - Vec3::Z * half.z) == 0)
}
impl Navigator {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.path.len() <= 128
                && self.path.iter().all(|p| p.is_finite())
                && self.goal.is_finite()
                && (0.0..=1.1).contains(&self.retry),
            "Invalid saved flight route"
        );
        Ok(())
    }
    pub fn reset(&mut self) {
        self.path.clear();
        self.retry = 0.;
    }
    pub fn advance(
        &mut self,
        world: &World,
        start: Vec3,
        goal: Vec3,
        half: Vec3,
        speed: f32,
        dt: f32,
    ) -> Vec3 {
        if dt <= 0. {
            return start;
        }
        self.retry = (self.retry - dt).max(0.);
        let desired = if clear(world, start, goal, half) {
            self.path.clear();
            goal
        } else {
            if (self.path.is_empty() || self.goal.distance(goal) > 80.) && self.retry == 0. {
                self.path = route(world, start, goal, half);
                self.goal = goal;
                self.retry = 1.;
            }
            while self.path.first().is_some_and(|p| p.distance(start) < 4.) {
                self.path.remove(0);
            }
            // Shorten a checked route as soon as a later waypoint is directly reachable.
            if let Some(index) = self
                .path
                .iter()
                .rposition(|p| clear(world, start, *p, half))
            {
                self.path.drain(..index);
            }
            self.path.first().copied().unwrap_or(start)
        };
        let end = start + (desired - start).clamp_length_max(speed * dt);
        if clear(world, start, end, half) {
            end
        } else {
            self.path.clear();
            start
        }
    }
}
fn route(world: &World, start: Vec3, goal: Vec3, half: Vec3) -> Vec<Vec3> {
    let timer = crate::spike_log::on().then(std::time::Instant::now);
    let before = world.flight_counts();
    let mut tests = 0u32;
    let mut calls = 0u32;
    let mut clear = |a: Vec3, b: Vec3| {
        calls += 1;
        tests += 1;
        let answer = clear(world, a, b, half);
        answer
    };
    let (path, expanded) = search(start, goal, &mut clear);
    #[cfg(not(target_os="vita"))]
    if std::env::var_os("LOOKING_GLASS_ROUTE_TRACE").is_some() { println!("[lg/flight-work] calls={calls} tests={tests} expanded={expanded}"); }
    if let Some(t) = timer {
        let after=world.flight_counts();
        crate::spike_log::flight_work([calls,tests,after[0].wrapping_sub(before[0]),after[1].wrapping_sub(before[1]),after[2].wrapping_sub(before[2]),after[3].wrapping_sub(before[3])]);
        crate::spike_log::planner(crate::spike_log::FLIGHT_ROUTE, t.elapsed().as_secs_f64() * 1000., expanded, tests);
    }
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    if std::env::var_os("LOOKING_GLASS_FLIGHT_BENCH").is_some() {
        // Host-only repetitions of this immutable snapshot, excluded from console builds.
        for trial in 0..24 {
            let now = std::time::Instant::now();
            let answer = search(start, goal, &mut |a, b| crate::flight::clear(world, a, b, half));
            let ms = now.elapsed().as_secs_f64() * 1000.;
            assert_eq!(answer, (path.clone(), expanded));
            println!("[lg/flight-bench] trial={trial} any={} blocked={} ms={ms:.4}",
                crate::sim_maps::sweep_any(), crate::sim_maps::sweep_blocked());
        }
    }
    // Host diagnostics: every search with its inputs, to see which ones repeat.
    #[cfg(not(target_os = "vita"))]
    if std::env::var_os("LOOKING_GLASS_ROUTE_TRACE").is_some() {
        println!("[lg/route-trace] flight start={:?} goal={:?} half={:?} expanded={expanded} tests={tests} path={}",
            start.to_array().map(f32::to_bits), goal.to_array().map(f32::to_bits), half.to_array(), path.len());
    }
    path
}
enum Nodes {
    Sparse(BTreeMap<[i16;3], (u32,[i16;3])>),
    Dense(Vec<Option<(u32,[i16;3])>>),
}
impl Nodes {
    fn index(k: [i16;3]) -> usize { ((k[0]+8) as usize*17+(k[1]+8) as usize)*13+(k[2]+6) as usize }
    fn get(&self, k: &[i16;3]) -> Option<&(u32,[i16;3])> {
        match self { Self::Sparse(n)=>n.get(k), Self::Dense(n)=>n[Self::index(*k)].as_ref() }
    }
    fn insert(&mut self, k: [i16;3], v: (u32,[i16;3])) {
        match self { Self::Sparse(n)=>{n.insert(k,v);}, Self::Dense(n)=>n[Self::index(k)]=Some(v) }
    }
}
/// The route search, with `clear` answering every clearance test. Returns the route and the
/// number of nodes expanded.
fn search(start: Vec3, goal: Vec3, clear: &mut impl FnMut(Vec3, Vec3) -> bool) -> (Vec<Vec3>, u32) {
    search_impl(start, goal, clear, memo_enabled())
}
fn search_impl(start: Vec3, goal: Vec3, clear: &mut impl FnMut(Vec3, Vec3) -> bool, dense: bool) -> (Vec<Vec3>, u32) {
    const CELL: f32 = 80.;
    type Key = [i16; 3];
    let point = |k: Key| start + vec3(k[0] as f32, k[1] as f32, k[2] as f32) * CELL;
    let estimate = |k: Key| (point(k).distance(goal) * 10.) as u32;
    let root = [0; 3];
    let mut nodes = if dense { Nodes::Dense(vec![None;17*17*13]) } else { Nodes::Sparse(BTreeMap::new()) };
    nodes.insert(root,(0u32,root));
    let mut queue = BinaryHeap::from([Reverse((estimate(root), 0u32, root))]);
    let mut best = root;
    let mut reached = false;
    let mut sequence = 0;
    let mut expanded = 0u32;
    while let Some(Reverse((_, _, key))) = queue.pop() {
        expanded += 1;
        if expanded > 1536 {
            break;
        }
        let at = point(key);
        if estimate(key) < estimate(best) {
            best = key;
        }
        if clear(at, goal) {
            best = key;
            reached = true;
            break;
        }
        let cost = nodes.get(&key).unwrap().0;
        for axis in 0..3 {
            for direction in [-1, 1] {
                let mut next = key;
                next[axis] += direction;
                if next[0].abs() > 8 || next[1].abs() > 8 || next[2].abs() > 6 {
                    continue;
                }
                let new_cost = cost + 800;
                if nodes.get(&next).is_some_and(|v| v.0 <= new_cost)
                    || !clear(at, point(next))
                {
                    continue;
                }
                nodes.insert(next, (new_cost, key));
                sequence += 1;
                queue.push(Reverse((new_cost + estimate(next), sequence, next)));
            }
        }
    }
    let mut result = Vec::new();
    if reached {
        result.push(goal);
    }
    let mut at = best;
    while at != root && result.len() < 128 {
        result.push(point(at));
        at = nodes.get(&at).unwrap().1;
    }
    result.reverse();
    (result, expanded)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_nodes_preserve_queue_cap_and_partial_paths() {
        let world=World::fixture(&[(vec3(100.,-160.,-100.),vec3(130.,160.,300.)),
            (vec3(-700.,-700.,200.),vec3(700.,700.,240.))]);
        for goal in [vec3(300.,0.,60.),vec3(300.,0.,220.),vec3(1000.,1000.,60.)] {
            let start=vec3(0.,0.,60.);
            let half=Vec3::splat(24.);
            let baseline=search_impl(start,goal,&mut |a,b| clear(&world,a,b,half),false);
            let mut calls=Vec::new();
            let actual=search_impl(start,goal,&mut |a,b| { calls.push((a,b)); clear(&world,a,b,half) },true);
            assert_eq!(baseline,actual);
            let mut reference_calls=Vec::new();
            search_impl(start,goal,&mut |a,b| { reference_calls.push((a,b)); clear(&world,a,b,half) },false);
            assert_eq!(calls,reference_calls);
        }
        let capped=search_impl(Vec3::ZERO,vec3(5000.,5000.,5000.),&mut |a,b| a.distance(b)<81.,false);
        assert_eq!(capped.1,1537);
        assert!(!capped.0.is_empty(), "cap exhaustion returns a partial path");
        assert_eq!(capped,search_impl(Vec3::ZERO,vec3(5000.,5000.,5000.),&mut |a,b| a.distance(b)<81.,true));
    }
    #[test]
    fn route_goes_around_a_wall_under_a_ceiling_without_clipping() {
        let world = World::fixture(&[
            (vec3(130., -140., -40.), vec3(170., 140., 260.)),
            (vec3(-500., -500., 220.), vec3(700., 500., 250.)),
            (vec3(-500., -500., -80.), vec3(700., 500., 0.)),
        ]);
        for hz in [30, 60, 144] {
            let mut nav = Navigator::default();
            let mut at = vec3(0., 0., 90.);
            let goal = vec3(320., 0., 90.);
            let half = Vec3::splat(24.);
            let mut detoured = false;
            for _ in 0..hz * 8 {
                let next = nav.advance(&world, at, goal, half, 150., 1. / hz as f32);
                assert!(clear(&world, at, next, half));
                at = next;
                detoured |= at.y.abs() > 150.;
            }
            assert!(detoured && at.distance(goal) < 1., "{hz}: {at:?}");
        }
    }
    #[test]
    fn route_avoids_water_and_freezes_when_paused() {
        let mut world = World::fixture(&[]);
        world.add_liquid(vec3(100., -100., -100.), vec3(200., 100., 150.), 32, None);
        let mut nav = Navigator::default();
        let start = vec3(0., 0., 50.);
        assert_eq!(
            nav.advance(&world, start, Vec3::X * 300., Vec3::splat(24.), 240., 0.),
            start
        );
        let mut at = start;
        for _ in 0..600 {
            at = nav.advance(
                &world,
                at,
                vec3(300., 0., 50.),
                Vec3::splat(24.),
                240.,
                1. / 120.,
            );
            assert_eq!(world.liquid_at(at - Vec3::Z * 24.), 0);
        }
        assert!(at.distance(vec3(300., 0., 50.)) < 1.);
    }
}
