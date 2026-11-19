//! Pickup selection and collection commands resolved before rendering.
use super::*;
use macroquad::prelude::*;
#[derive(Default)]
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
impl Art {
    pub(crate) fn extract_input(&mut self,items:&[crate::inventory::Pickup],stats:&crate::inventory::Stats,visit:&str,camera:Vec3,clock:f32,will_draw:bool,out:&mut Input) {
        if out.visit!=visit {self.snapshot_visit=Default::default();}
        out.generation=self.snapshot_visit.id();out.visit.clear();out.visit.push_str(visit);out.clock=clock;
        out.props.clear();out.essences.clear();out.events.clear();out.markers.clear();
        let drops=stats.loot.get(visit).map_or(0,|l|l.drops.len());
        out.overflow=items.len()+drops+self.collected.len()>16384;
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
        // Collection history is simulation-owned on the opt-in path. Prune only
        // on iterations that would have reached the legacy pickup draw.
        if will_draw {self.collected.retain(|(_,_,start,_)|(0. ..=4.).contains(&(clock-start)));}
        for (model,origin,start,id) in &self.collected {
            let slot=self.collection_fx.keys().position(|m|m==model).expect("resident collection effect");
            out.events.push((slot,*origin,*start,*id));
        }
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
