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
/// The five liquid probes the original predicate tests, with exactly its `lerp` and
/// subtraction. Rounding can put the last one outside `[a, b]`, so certificates must
/// bound these computed points, not the endpoints.
fn liquid_probes(a: Vec3, b: Vec3, half: Vec3) -> [Vec3; 5] {
    let shift = Vec3::Z * half.z;
    std::array::from_fn(|i| a.lerp(b, i as f32 / 4.) - shift)
}
/// The memoised liquid test. A liquid AABB that misses the box around the five computed
/// probes proves every probe is 0 exactly; otherwise the original probes answer.
pub(crate) fn liquid_clear(world: &World, a: Vec3, b: Vec3, half: Vec3) -> bool {
    let probes = liquid_probes(a, b, half);
    let (mut min, mut max) = (probes[0], probes[0]);
    for p in &probes[1..] {
        min = min.min(*p);
        max = max.max(*p);
    }
    world.liquid_box_clear(min, max) || probes.iter().all(|p| world.liquid_at(*p) == 0)
}
fn clear(world: &World, a: Vec3, b: Vec3, half: Vec3, memo: bool) -> bool {
    if !memo { return !world.sweep_blocked(a, b, half)
        && (0..=4).all(|i| world.liquid_at(a.lerp(b, i as f32 / 4.) - Vec3::Z * half.z) == 0); }
    if world.flight_blocked(a, b, half) { return false; }
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    let split = crate::collision::flight_split_on();
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if split { world.count_flight_probe(9, 1); }
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    let mark = split.then(std::time::Instant::now);
    let liquid = liquid_clear(world, a, b, half);
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if let Some(mark) = mark {
        world.add_flight_probe(5, mark.elapsed().as_nanos() as u64);
        world.count_flight_probe(10, 1);
        if !liquid { world.count_flight_probe(8, 1); }
    }
    liquid
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
        let memo = memo_enabled();
        let desired = if clear(world, start, goal, half, memo) {
            self.path.clear();
            goal
        } else {
            if (self.path.is_empty() || self.goal.distance(goal) > 80.) && self.retry == 0. {
                self.path = route(world, start, goal, half, memo);
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
                .rposition(|p| clear(world, start, *p, half, memo))
            {
                self.path.drain(..index);
            }
            self.path.first().copied().unwrap_or(start)
        };
        let end = start + (desired - start).clamp_length_max(speed * dt);
        if clear(world, start, end, half, memo) {
            end
        } else {
            self.path.clear();
            start
        }
    }
}
fn route(world: &World, start: Vec3, goal: Vec3, half: Vec3, memo: bool) -> Vec<Vec3> {
    let timer = crate::spike_log::on().then(std::time::Instant::now);
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    world.reset_flight_probe();
    let before = world.flight_counts();
    let mut tests = 0u32;
    let mut calls = 0u32;
    let mut clear = |a: Vec3, b: Vec3| {
        calls += 1;
        tests += 1;
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let whole = crate::collision::flight_split_on().then(std::time::Instant::now);
        let answer = clear(world, a, b, half, memo);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if let Some(whole) = whole { world.add_flight_probe(6, whole.elapsed().as_nanos() as u64); }
        answer
    };
    let (path, expanded) = search(start, goal, &mut clear, memo);
    // Host-only class split of this search. Printed before the spike line so the
    // two can be read together; the shipped path does not compile this block.
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if crate::collision::flight_split_on() {
        let p = world.flight_probe();
        let ms = timer.map_or(f64::NAN, |t| t.elapsed().as_secs_f64() * 1000.);
        let n = |i: usize| p.ns[i] as f64 / 1e6;
        println!("[lg/flight-split] total_ms={ms:.3} grid={:.3} short={:.3} witness={:.3} long={:.3} mutable={:.3} liquid={:.3} bookkeeping={:.3} | calls={} blocked={} grid_n={} short_n={} short_blocked={} witness_n={} long_n={} liquid_sets={} liquid_blocked={}",
            n(0), n(1), n(2), n(3), n(4), n(5), ms - (0..6).map(n).sum::<f64>(),
            p.count[0], p.count[1], p.count[2], p.count[3], p.count[4], p.count[5], p.count[6], p.count[10], p.count[8]);
        println!("[lg/flight-split] memo_clear_calls={} static_clear={} liquid_sets={}", p.count[9], p.count[0]-p.count[1], p.count[10]);
        println!("[lg/flight-split] short_witness={} cert32p33={} cert64p3206={} unused={} of_short={} liquids={}", p.count[14], p.count[11], p.count[12], p.count[13], p.count[3], world.liquid_count());
        let clear_total = p.ns[6] as f64 / 1e6;
        println!("[lg/flight-split] clear_total={clear_total:.3} search_loop={:.3} inside_clear={:.3}", ms - clear_total,
            (0..6).map(n).sum::<f64>());
    }
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
        // The answer sequence is captured once, so the same A* can be timed with world
        // queries (real) and with stored answers (bookkeeping only).
        let mut answers = Vec::new();
        let replay = search_impl(start, goal, &mut |a, b| {
            let answer = crate::flight::clear(world, a, b, half, memo);
            answers.push(answer);
            answer
        }, memo_enabled());
        assert_eq!(replay, (path.clone(), expanded), "captured answer sequence changed the route");
        for trial in 0..24 {
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            world.reset_flight_probe();
            let now = std::time::Instant::now();
            let answer = search(start, goal, &mut |a, b| crate::flight::clear(world, a, b, half, memo), memo);
            let ms = now.elapsed().as_secs_f64() * 1000.;
            assert_eq!(answer, (path.clone(), expanded));
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            {
                let p = world.flight_probe();
                print!("[lg/flight-bench] trial={trial} kind=real ms={ms:.4} grid={:.4} short={:.4} witness={:.4} mutable={:.4} liquid={:.4}",
                    p.ns[0] as f64 / 1e6, p.ns[1] as f64 / 1e6, p.ns[2] as f64 / 1e6, p.ns[4] as f64 / 1e6, p.ns[5] as f64 / 1e6);
                println!(" any={} blocked={}", crate::sim_maps::sweep_any(), crate::sim_maps::sweep_blocked());
            }
            #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
            println!("[lg/flight-bench] trial={trial} kind=real ms={ms:.4}");
        }
        for trial in 0..24 {
            let now = std::time::Instant::now();
            let mut at = 0;
            let answer = search_impl(start, goal, &mut |_, _| { let r = answers[at]; at += 1; r }, memo);
            let ms = now.elapsed().as_secs_f64() * 1000.;
            assert_eq!(answer, (path.clone(), expanded));
            println!("[lg/flight-bench] trial={trial} kind=book ms={ms:.4} answers={at}");
        }
        let mut empty = std::time::Instant::now();
        for _ in 0..200_000 { std::hint::black_box(empty.elapsed()); empty = std::time::Instant::now(); }
        println!("[lg/flight-clock] pair_ms={:.3}", empty.elapsed().as_secs_f64() * 1000.);
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
fn search(start: Vec3, goal: Vec3, clear: &mut impl FnMut(Vec3, Vec3) -> bool, memo: bool) -> (Vec<Vec3>, u32) {
    search_impl(start, goal, clear, memo)
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
            let baseline=search_impl(start,goal,&mut |a,b| clear(&world,a,b,half,false),false);
            let mut calls=Vec::new();
            let actual=search_impl(start,goal,&mut |a,b| { calls.push((a,b)); clear(&world,a,b,half,false) },true);
            assert_eq!(baseline,actual);
            let mut reference_calls=Vec::new();
            search_impl(start,goal,&mut |a,b| { reference_calls.push((a,b)); clear(&world,a,b,half,false) },false);
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
                assert!(clear(&world, at, next, half, false));
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
    #[test]
    fn liquid_certificate_bounds_the_computed_probes() {
        let mut world = World::fixture(&[]);
        world.add_liquid(vec3(0., -10., -10.), vec3(10., 10., 10.), 8, None);
        let a = vec3(-8192., 0., 24.);
        let b = vec3(-0.0001, 0., 24.);
        let half = Vec3::splat(24.);
        let shift = Vec3::Z * half.z;
        // f32 cancellation puts the t=1 probe at the origin, outside the endpoint box.
        let probe = a.lerp(b, 1.) - shift;
        assert_eq!(probe, Vec3::ZERO);
        assert_ne!(world.liquid_at(probe), 0);
        assert!(world.liquid_box_clear(a.min(b) - shift, a.max(b) - shift));
        assert!(!liquid_clear(&world, a, b, half));
        assert!(!clear(&world, a, b, half, true));
        assert!(!clear(&world, a, b, half, false));
    }
    #[test]
    fn liquid_certificate_cases_on_shipped_maps() {
        // These are the review's targeted real-map counterexamples. The host copy is used
        // when present; the suite still runs without the private game data.
        let dir = std::env::var_os("LOOKING_GLASS_TEST_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(
                "/Users/anezium/vita-work/alice-data/Alice1/bin/base"));
        if !dir.is_dir() {
            println!("skipping shipped map liquid cases: {} absent", dir.display());
            return;
        }
        let mut assets = crate::assets::Assets::open(&dir).unwrap();
        for (name, a, b, hz) in [
            ("hatter1", vec3(-7552., 3996., 88.), vec3(-7552., 3996., 8.000001), 24.),
            ("hedge3", vec3(3612., -2048., 224.), vec3(3612., -2048., 64.00001), 32.),
        ] {
            let map = crate::bsp::Bsp::parse(&assets.read(&format!("maps/{name}.bsp")).unwrap()).unwrap();
            let world = World::actor_world(&map).unwrap();
            let half = Vec3::splat(hz);
            let trace = world.sweep_except(a, b, half, None);
            assert!(!trace.start_solid && trace.fraction >= 1.,
                "{name}: the solid sweep must be clear");
            let shift = Vec3::Z * half.z;
            assert!(world.liquid_box_clear(a.min(b) - shift, a.max(b) - shift),
                "{name}: the endpoint box must still be the counterexample");
            assert!(!clear(&world, a, b, half, false),
                "{name}: the original probes must reject");
            assert!(!clear(&world, a, b, half, true),
                "{name}: the liquid certificate must reject");
            println!("liquid certificate case {name} rejected by the computed probes");
        }
    }
}
