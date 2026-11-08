//! Pure ownership/pose rules used by the opt-in legacy sky bridge.
use glam::{Quat, Vec3};

/// Keep only geometry consumed by the authored sky and miniature portal view.
/// No clones or new geometry allocations. Remaining GPU/material owners stay in Scene.
pub(super) fn retain_geometry<T>(requested: bool, sky: bool, has_origin: bool,
    batches: &mut Vec<T>, props: &mut Vec<T>, classify: impl Fn(&T) -> (bool, bool)) -> bool {
    let active = requested && sky && (batches.iter().any(|b| classify(b).0)
        || (has_origin && (!props.is_empty() || batches.iter().any(|b| classify(b).1))));
    if active {
        batches.retain(|b| { let (sky, static_world) = classify(b); sky || (has_origin && static_world) });
    } else { batches.clear(); }
    if !active || !has_origin { props.clear(); }
    active
}

/// Same fixed-origin translation and inverse rotation as the legacy sky view.
/// Scripted camera motion supplies rotation only, matching that reference.
pub(super) fn portal_pose(camera: Vec3, origin: Vec3, rotation: Quat, motion: Quat)
    -> (Vec3, (usize, Vec3, Quat)) {
    let offset = camera - origin;
    let r = (rotation * motion).inverse();
    (offset, (0, origin - r * origin, r))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    struct Item { sky:bool, static_world:bool, id:u8, drops:Rc<Cell<usize>> }
    impl Drop for Item { fn drop(&mut self) { self.drops.set(self.drops.get()+1); } }
    fn item(id:u8, sky:bool, static_world:bool, drops:&Rc<Cell<usize>>) -> Item {
        Item { sky, static_world, id, drops:drops.clone() }
    }
    fn flags(b:&Item)->(bool,bool) { (b.sky,b.static_world) }
    #[test] fn off_releases_all_legacy_owners() {
        let d=Rc::new(Cell::new(0));let mut b=vec![item(1,true,true,&d),item(2,false,true,&d)];let mut p=vec![item(3,false,false,&d)];
        assert!(!retain_geometry(false,true,true,&mut b,&mut p,flags));
        assert!(b.is_empty()&&p.is_empty());assert_eq!(d.get(),3,"SKY_OFF_RELEASES_OWNERS");
    }
    #[test] fn sky_mask_off_keeps_baseline_release_policy() {
        let d=Rc::new(Cell::new(0));let mut b=vec![item(1,true,true,&d)];let mut p=vec![];
        assert!(!retain_geometry(true,false,true,&mut b,&mut p,flags));assert_eq!(d.get(),1);
    }
    #[test] fn portal_keeps_static_view_and_sky_props_but_not_world_movers() {
        let d=Rc::new(Cell::new(0));let mut b=vec![item(1,true,false,&d),item(2,false,true,&d),item(3,false,false,&d)];let mut p=vec![item(4,false,false,&d)];
        assert!(retain_geometry(true,true,true,&mut b,&mut p,flags));
        assert_eq!(b.iter().map(|i|i.id).collect::<Vec<_>>(),vec![1,2],"SKY_PORTAL_GEOMETRY_SELECTION");assert_eq!(p[0].id,4);assert_eq!(d.get(),1);
        drop(b);drop(p);assert_eq!(d.get(),4,"SKY_SCENE_OWNERS_RELEASE_ONCE");
    }
    #[test] fn no_portal_keeps_authored_surfaces_only() {
        let d=Rc::new(Cell::new(0));let mut b=vec![item(1,true,true,&d),item(2,false,true,&d)];let mut p=vec![item(3,false,false,&d)];
        assert!(retain_geometry(true,true,false,&mut b,&mut p,flags));assert_eq!(b.len(),1);assert_eq!(b[0].id,1);assert!(p.is_empty());assert_eq!(d.get(),2);
    }
    #[test] fn absent_authored_content_preserves_gradient_fallback() {
        let d=Rc::new(Cell::new(0));let mut b=vec![item(2,false,true,&d)];let mut p=vec![];
        assert!(!retain_geometry(true,true,false,&mut b,&mut p,flags));assert!(b.is_empty());assert_eq!(d.get(),1);
    }
    #[test] fn retained_storage_is_reused_without_geometry_clones() {
        let d=Rc::new(Cell::new(0));let mut b=vec![item(1,true,true,&d)];let mut p=vec![];let ptr=b.as_ptr();let cap=b.capacity();
        for _ in 0..8 {assert!(retain_geometry(true,true,false,&mut b,&mut p,flags));assert_eq!(b.as_ptr(),ptr);assert_eq!(b.capacity(),cap);}
        assert_eq!(d.get(),0);drop(b);assert_eq!(d.get(),1);
    }
    #[test] fn portal_origin_and_camera_translation_match_legacy() {
        let origin=Vec3::new(120.,-40.,32.);let q=Quat::from_rotation_z(0.7);let m=Quat::from_rotation_x(-0.3);let point=origin+Vec3::new(4.,7.,11.);
        for eye in [Vec3::ZERO,Vec3::new(800.,-900.,40.)] {
            let (offset,(_,translation,r))=portal_pose(eye,origin,q,m);
            let relative=translation+r*point+offset-eye;
            let expected=m.inverse()*(q.inverse()*(point-origin));
            assert!(relative.distance(expected)<0.0002,"SKY_ORIGIN_PARALLAX");
        }
    }
}
