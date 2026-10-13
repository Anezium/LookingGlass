//! Game-thread presentation reuse for one overlap frame, in traversal order.
//! Owning handles keep a result alive across nested attachment draws and unwinds.
use super::{fill_palette, overlap};
use crate::skeletal::Transform;
use macroquad::prelude::*;
use std::{cell::RefCell, rc::Rc};
const LIMIT: usize = 128;
struct Cache<T> {
    entries: Vec<Option<Rc<T>>>,
    count: usize,
    cursor: usize,
    reused: usize,
}
impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self {
            entries: (0..LIMIT).map(|_| None).collect(),
            count: 0,
            cursor: 0,
            reused: 0,
        }
    }
}
impl<T: Default> Cache<T> {
    fn get(&mut self, matches: impl FnOnce(&T) -> bool, fill: impl FnOnce(&mut T)) -> Rc<T> {
        if overlap::gathering() && self.count < LIMIT {
            let index = self.count;
            self.count += 1;
            let mut value = self.entries[index].take().unwrap_or_default();
            // An outstanding handle may survive a caller's unwind: never mutate it.
            if Rc::get_mut(&mut value).is_none() {
                value = Rc::default();
            }
            fill(Rc::get_mut(&mut value).unwrap());
            self.entries[index] = Some(value.clone());
            return value;
        }
        if overlap::ready() {
            let index = self.cursor;
            self.cursor += 1;
            if index < self.count {
                if let Some(value) = &self.entries[index] {
                    if matches(value) {
                        self.reused += 1;
                        return value.clone();
                    }
                }
            }
        }
        let mut value = T::default();
        fill(&mut value);
        Rc::new(value)
    }
    fn begin(&mut self) {
        self.count = 0;
        self.cursor = 0;
        self.reused = 0;
    }
    fn assert_consumed(&self) {
        assert_eq!(self.cursor, self.count, "unconsumed presentation entries");
        assert_eq!(
            self.reused, self.count,
            "presentation recomputed during submission"
        );
    }
}
#[derive(Default)]
pub(crate) struct Pose {
    key: (usize, usize),
    local: Vec<Transform>,
    mouth: u32,
    roots: Vec<(usize, f32)>,
    pub pose: Vec<Transform>,
    pub scales: Vec<f32>,
}
pub(crate) enum PoseHandle {
    Owned(Pose),
    Shared(Rc<Pose>),
}
impl std::ops::Deref for PoseHandle {
    type Target = Pose;
    fn deref(&self) -> &Pose {
        match self {
            Self::Owned(p) => p,
            Self::Shared(p) => p,
        }
    }
}
#[derive(Default)]
struct Palette {
    pose: Vec<Transform>,
    scales: Vec<f32>,
    values: Vec<(Mat3, Vec3)>,
}
thread_local! {
    static POSES: RefCell<Cache<Pose>> = RefCell::default();
    static PALETTES: RefCell<Cache<Palette>> = RefCell::default();
}
pub(super) fn begin() {
    POSES.with(|c| c.borrow_mut().begin());
    PALETTES.with(|c| c.borrow_mut().begin());
}
pub(super) fn assert_consumed() {
    POSES.with(|c| c.borrow().assert_consumed());
    PALETTES.with(|c| c.borrow().assert_consumed());
}
pub(crate) fn pose(
    key: (usize, usize),
    local: &[Transform],
    mouth: f32,
    roots: &[(usize, f32)],
    prepare: impl FnOnce() -> (Vec<Transform>, Vec<f32>),
) -> PoseHandle {
    if !overlap::gathering() && !overlap::ready() {
        let (pose, scales) = prepare();
        return PoseHandle::Owned(Pose {
            pose,
            scales,
            ..Default::default()
        });
    }
    PoseHandle::Shared(POSES.with(|c| {
        c.borrow_mut().get(
            |v| {
                v.key == key
                    && v.mouth == mouth.to_bits()
                    && overlap::same_pose(&v.local, local)
                    && v.roots.len() == roots.len()
                    && v.roots
                        .iter()
                        .zip(roots)
                        .all(|(a, b)| a.0 == b.0 && a.1.to_bits() == b.1.to_bits())
            },
            |v| {
                v.key = key;
                v.mouth = mouth.to_bits();
                v.local.clear();
                v.local.extend_from_slice(local);
                v.roots.clear();
                v.roots.extend_from_slice(roots);
                (v.pose, v.scales) = prepare();
            },
        )
    }))
}
pub(super) fn palette<R>(
    pose: &[Transform],
    scales: &[f32],
    draw: impl FnOnce(&[(Mat3, Vec3)]) -> R,
) -> R {
    let value = PALETTES.with(|c| {
        c.borrow_mut().get(
            |v| overlap::same_pose(&v.pose, pose) && overlap::same_floats(&v.scales, scales),
            |v| {
                v.pose.clear();
                v.pose.extend_from_slice(pose);
                v.scales.clear();
                v.scales.extend_from_slice(scales);
                fill_palette(&mut v.values, pose, scales);
            },
        )
    });
    draw(&value.values)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn local(x: f32) -> Vec<Transform> {
        vec![Transform {
            translation: Vec3::X * x,
            rotation: Quat::IDENTITY,
        }]
    }
    #[test]
    fn duplicate_models_reuse_each_occurrence_pose_scales_and_palette_once() {
        overlap::startup(2, false);
        let mut frame = overlap::Frame::begin();
        let first = local(1.);
        let second = local(2.);
        let roots = [(0, 1.4)];
        let a = pose((7, 9), &first, 0.1, &[], || (first.clone(), vec![]));
        let b = pose((7, 9), &second, 0.2, &roots, || (second.clone(), vec![1.4]));
        let ptr_a = palette(&a.pose, &a.scales, |p| p.as_ptr());
        let ptr_b = palette(&b.pose, &b.scales, |p| p.as_ptr());
        frame.dispatch();
        frame.collect();
        let aa = pose((7, 9), &first, 0.1, &[], || panic!("pose recomputed"));
        let bb = pose((7, 9), &second, 0.2, &roots, || {
            panic!("scaled pose recomputed")
        });
        assert!(std::ptr::eq(&*a, &*aa));
        assert!(std::ptr::eq(&*b, &*bb));
        palette(&aa.pose, &aa.scales, |p| assert_eq!(p.as_ptr(), ptr_a));
        palette(&bb.pose, &bb.scales, |p| assert_eq!(p.as_ptr(), ptr_b));
        assert_ne!(aa.pose[0].translation, bb.pose[0].translation);
        assert_eq!(bb.scales, [1.4]);
        frame.assert_consumed();
        drop(frame);
    }
    #[test]
    fn changed_inputs_and_new_frames_cannot_reuse_stale_preparation() {
        overlap::startup(1, false);
        let input = local(1.);
        for changed in 0..5 {
            let mut frame = overlap::Frame::begin();
            let a = pose((7, 9), &input, 0.1, &[(0, 1.)], || {
                (input.clone(), vec![1.])
            });
            frame.dispatch();
            frame.collect();
            let different = local(2.);
            let result = pose(
                if changed == 0 { (8, 9) } else { (7, 9) },
                if changed == 1 { &different } else { &input },
                if changed == 2 { 0.2 } else { 0.1 },
                if changed == 3 { &[(0, 2.)] } else { &[(0, 1.)] },
                || (different.clone(), vec![2.]),
            );
            if changed < 4 {
                assert_eq!(result.pose[0].translation, Vec3::X * 2.);
                assert!(!std::ptr::eq(&*a, &*result));
            } else {
                assert!(std::ptr::eq(&*a, &*result));
            }
            drop(frame);
        }
        // Same inputs in a later frame require a new evaluation.
        let _frame = overlap::Frame::begin();
        let result = pose((7, 9), &input, 0.1, &[(0, 1.)], || (local(3.), vec![3.]));
        assert_eq!(result.pose[0].translation, Vec3::X * 3.);
    }
    #[test]
    fn serial_presentation_does_not_allocate_cache_handles_or_clone_inputs() {
        let input = local(1.);
        let prepared = pose((7, 9), &input, 0.1, &[], || (input.clone(), vec![]));
        match prepared {
            PoseHandle::Owned(p) => {
                assert!(p.local.is_empty());
                assert!(p.roots.is_empty());
            }
            PoseHandle::Shared(_) => panic!("serial preparation was cached"),
        }
    }
}
