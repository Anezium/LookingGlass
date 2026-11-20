//! Pickup selection and collection commands resolved before rendering.
use super::*;
use macroquad::prelude::*;
/// Model, origin, collection clock and occurrence id.
pub(crate) type CollectionEvent = (String, Vec3, f32, u64);

#[derive(Default)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct Input {
    generation:u64, visit:String, clock:f32, overflow:bool,
    props:Vec<(bool,usize,Vec3)>,
    essences:Vec<(crate::loot::Grade,Vec3,String)>,
    events:Vec<(usize,Vec3,f32,u64)>,
    pub(crate) markers:Vec<(usize,Vec3)>,
}
impl Input {
    pub(crate) fn bytes(&self)->usize {
        std::mem::size_of::<Self>()+self.visit.len()+self.props.len()*std::mem::size_of::<(bool,usize,Vec3)>()
            +self.essences.len()*std::mem::size_of::<(crate::loot::Grade,Vec3,String)>()+self.essences.iter().map(|e|e.2.len()).sum::<usize>()
            +self.events.len()*std::mem::size_of::<(usize,Vec3,f32,u64)>()+self.markers.len()*std::mem::size_of::<(usize,Vec3)>()
    }
    pub(crate) fn overflow(&self)->bool {self.overflow}
}
impl Input {
    /// Capture from plain pickup and collection state. The visit generation is
    /// sampled on the owner, and the history is read without pruning it.
    pub(crate) fn capture(
        &mut self,
        generation: u64,
        items: &[crate::inventory::Pickup],
        stats: &crate::inventory::Stats,
        visit: &str,
        camera: Vec3,
        clock: f32,
        will_draw: bool,
        collected: &[CollectionEvent],
    ) {
        self.capture_with_slots(generation, items, stats, visit, camera, clock, will_draw,
            collected, |model| {
                // Owner resources are a BTreeMap populated from these same names.
                let mut keys = collection_models();
                assert!(keys.any(|name| name == model), "resident collection effect");
                collection_models().filter(|name| *name < model).count()
            });
    }

    fn capture_with_slots(
        &mut self,
        generation: u64,
        items: &[crate::inventory::Pickup],
        stats: &crate::inventory::Stats,
        visit: &str,
        camera: Vec3,
        clock: f32,
        will_draw: bool,
        collected: &[CollectionEvent],
        slot: impl Fn(&str) -> usize,
    ) {
        let out = self;
        out.generation = generation;
        out.visit.clear();
        out.visit.push_str(visit);
        out.clock = clock;
        out.props.clear();out.essences.clear();out.events.clear();out.markers.clear();
        let drops=stats.loot.get(visit).map_or(0,|l|l.drops.len());
        out.overflow=items.len()+drops+collected.len()>16384;
        if out.overflow {return;}
        for item in items {
            if stats.collected.contains(&item.id) {continue;}
            if let crate::inventory::PickupKind::Weapon(i)=item.kind {
                if camera.distance_squared(item.origin)<=1500.*1500. {out.markers.push((i,item.origin+Vec3::Z*(44.+(clock*2.).sin()*2.)));}
            }
            if camera.distance_squared(item.origin)>2500.*2500. {continue;}
            match item.kind {
                crate::inventory::PickupKind::Power(kind)=>out.props.push((true,match kind {Kind::Rage=>0,Kind::Tea=>1,Kind::Glass=>2},item.origin)),
                crate::inventory::PickupKind::Sanity|crate::inventory::PickupKind::Will=>{
                    if let Some(i)=["p_h1","p_h2","p_m1","p_m2"].iter().position(|name|*name==item.model){out.props.push((false,i,item.origin));}
                },
                crate::inventory::PickupKind::Essence=>{
                    if let Some(grade)=crate::loot_art::GRADES.iter().find(|g|g.model()==item.model){out.essences.push((*grade,item.origin,format!("placed:{}",item.id)));}
                },
                crate::inventory::PickupKind::Weapon(_)=>{},
            }
        }
        if let Some(loot)=stats.loot.get(visit){out.essences.extend(loot.drops.iter().map(|d|(d.grade,d.origin,format!("drop:{}",d.id))));}
        out.essences.sort_by(|a,b|camera.distance_squared(b.1).total_cmp(&camera.distance_squared(a.1)));
        for (model, origin, start, id) in collected {
            if !will_draw || collection_active(clock, *start) {
                out.events.push((slot(model), *origin, *start, *id));
            }
        }
    }
}

fn collection_active(clock: f32, start: f32) -> bool {
    (0. ..=4.).contains(&(clock - start))
}

impl Art {
    pub(crate) fn capture_history(&self) -> (u64, &[CollectionEvent]) {
        (self.snapshot_visit.id(), &self.collected)
    }

    pub(crate) fn prepare_visit(&mut self, previous: &Input, visit: &str) {
        if previous.visit != visit {
            self.snapshot_visit = Default::default();
        }
    }

    pub(crate) fn publish_input(&mut self, input: &Input, will_draw: bool) {
        // Preserve the owner pruning point and the pre-prune overflow test.
        if will_draw && self.valid_input(input) {
            self.collected.retain(|(_, _, start, _)| collection_active(input.clock, *start));
        }
    }

    pub(crate) fn extract_input(
        &mut self,
        items: &[crate::inventory::Pickup],
        stats: &crate::inventory::Stats,
        visit: &str,
        camera: Vec3,
        clock: f32,
        will_draw: bool,
        out: &mut Input,
    ) {
        self.prepare_visit(out, visit);
        let (generation, history) = self.capture_history();
        out.capture(generation, items, stats, visit, camera, clock, will_draw, history);
        self.publish_input(out, will_draw);
    }
    pub(crate) fn valid_input(&self,input:&Input)->bool {
        !input.overflow && input.generation==self.snapshot_visit.id()
            && input.props.iter().all(|&(power,slot,_)|slot<if power{self.props.len()}else{self.resources.len()})
            && input.events.iter().all(|&(slot,_,_,_)|slot<self.collection_fx.len())
            && input.markers.iter().all(|&(slot,_)|slot<crate::inventory::WEAPONS.len())
    }
    pub(crate) fn draw_input(&mut self,input:&Input,atmosphere:&crate::environment::Atmosphere,camera:Vec3) {
        assert!(self.valid_input(input));
        if self.effect_visit!=input.visit || input.clock<self.effect_clock {
            self.essence.clear_effects();for e in self.collection_fx.values(){e.clear();}
        }
        self.effect_visit.clone_from(&input.visit);self.effect_clock=input.clock;
        self.essence.begin_frame();for e in self.collection_fx.values(){e.begin_frame();}
        self.material.atmosphere(atmosphere,camera);self.material.bind();
        for &(power,slot,origin) in &input.props {
            let prop=if power{&mut self.props[slot]}else{&mut self.resources[slot]};
            prop.draw_frame(crate::skeletal::Transform{translation:origin,rotation:Quat::IDENTITY},1.,false,input.clock,true);
        }
        for (grade,origin,identity) in &input.essences {self.essence.draw_item(identity,*grade,*origin,atmosphere,camera,input.clock);}
        crate::render::depth_read_only(||{
            for &(slot,origin,start,id) in &input.events {
                self.collection_fx.values().nth(slot).unwrap().draw(Some(&format!("collection:{id}")),input.clock-start,origin,0.,camera,atmosphere);
            }
        });
        self.essence.end_frame();for e in self.collection_fx.values(){e.end_frame();}gl_use_default_material();
    }
}

#[cfg(all(test, target_os = "linux"))]
pub(crate) fn test_pickup_capture(assets: &mut crate::assets::Assets) {
    use crate::inventory::{Pickup, PickupKind, Stats};
    use crate::loot::{Drop, Grade, Loot};

    let mut art = Art::load(assets).unwrap();
    let item = |model: &str, kind, x| Pickup {
        id: model.into(), model: model.into(), origin: Vec3::X * x, kind, amount: 1.,
    };
    let items = [
        item("p_h1", PickupKind::Sanity, 4.),
        item("p_m2", PickupKind::Will, 40.),
        item("w_watch", PickupKind::Weapon(9), 1500.),
        item("w_lookingglass", PickupKind::Power(Kind::Glass), 2500.),
        item("w_me_large", PickupKind::Essence, 90.),
        item("w_me_small", PickupKind::Essence, 2501.),
    ];
    let mut stats = Stats::default();
    stats.collected.insert(items[1].id.clone());
    stats.loot.insert("fixture".into(), Loot {
        drops: vec![Drop { id: 17, origin: Vec3::X * 100., grade: Grade::Super, remaining: 2. }],
        ..Default::default()
    });
    let mut owner = Input::default();
    art.extract_input(&items, &stats, "fixture", Vec3::ZERO, 0., false, &mut owner);
    for (pickup, time) in [(&items[0], 0.), (&items[4], 1.), (&items[1], 4.)] {
        art.collected(pickup, time);
    }
    let (generation, _) = art.capture_history();
    let mut pure = Input::default();
    for (clock, will_draw) in [(2., false), (4., true), (5., true)] {
        let (_, history) = art.capture_history();
        pure.capture(generation, &items, &stats, "fixture", Vec3::ZERO, clock, will_draw, history);
        art.extract_input(&items, &stats, "fixture", Vec3::ZERO, clock, will_draw, &mut owner);
        assert_eq!(pure, owner);
        assert!(art.valid_input(&pure));
    }
    assert_eq!(pure.events.iter().map(|event| event.3).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(pure.markers.len(), 1);
    assert_eq!(pure.props.len(), 2);
    assert_eq!(pure.essences[0].2, "drop:17");
}

#[cfg(test)]
#[test]
fn capture_checks_overflow_before_history_age_filter() {
    let events = vec![("p_h1".into(), Vec3::ZERO, 0., 0); 16385];
    let mut input = Input::default();
    input.capture(7, &[], &crate::inventory::Stats::default(), "fixture", Vec3::ZERO, 20., true, &events);
    assert!(input.overflow());
    assert!(input.events.is_empty());
}
