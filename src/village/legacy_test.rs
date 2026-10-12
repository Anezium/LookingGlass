// Frozen 8de8940 village advance oracle; compiled only by host tests.
use super::*;
impl Village {
    pub(super) fn advance_legacy(
        &mut self,
        dt: f32,
        _map: &Bsp,
        world: &mut World,
        player: &mut Player,
        fixed: &[Collider],
    ) -> Result<()> {
        if dt <= 0. {
            return Ok(());
        }
        {
            #[cfg(target_os = "vita")]
            let _hotspot = crate::frame_profile::span("village_cinema");
            self.cinema.advance(dt.min(0.1), player, world)?;
        }
        #[cfg(target_os = "vita")]
        crate::frame_profile::counts(&[
            ("village_objects", self.objects.len() as u64),
            ("village_solid_objects", self.colliders().count() as u64),
        ]);
        #[cfg(target_os = "vita")]
        let (
            mut substeps,
            mut visited,
            mut unchanged,
            mut changed,
            mut changed_solid,
            mut local_contacts,
            mut publishes,
            mut published,
            mut riders,
            mut rejected,
        ) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
        let mut remaining = dt.min(0.1);
        while remaining > 0.000001 {
            #[cfg(target_os = "vita")]
            {
                substeps += 1;
            }
            let step = remaining.min(1. / 120.);
            remaining -= step;
            if self.hatch_open {
                self.hatch_time = (self.hatch_time + step).min(1.);
            }
            for i in 0..self.objects.len() {
                let o = &self.objects[i];
                #[cfg(target_os = "vita")]
                {
                    visited += 1;
                }
                #[cfg(target_os = "vita")]
                let _sample_profile = crate::frame_profile::span("village_sample");
                let mut next = sample(&o.name, o.base, o.time + step, self.hatch_time);
                if o.name == "shrink_door1" {
                    next.angles.z = self.cinema.door_angle();
                }
                #[cfg(target_os = "vita")]
                drop(_sample_profile);
                if next.origin == o.pose.origin && next.angles == o.pose.angles {
                    #[cfg(target_os = "vita")]
                    {
                        unchanged += 1;
                    }
                    self.objects[i].time += step;
                    continue;
                }
                #[cfg(target_os = "vita")]
                {
                    changed += 1;
                    changed_solid +=
                        u64::from(o.solid && !(o.name == "openthis" && self.hatch_open));
                }
                let collider = {
                    #[cfg(target_os = "vita")]
                    let _hotspot = crate::frame_profile::span("village_collider_build");
                    o.shape.at_eager(next.origin, next.rotation())
                };
                let solid = o.solid && !(o.name == "openthis" && self.hatch_open);
                #[cfg(target_os = "vita")]
                let _ground_profile = crate::frame_profile::span("village_ground_probe");
                #[cfg(target_os = "vita")]
                {
                    local_contacts += 1;
                }
                let ground = o.collider.trace(
                    player.feet + PLAYER_CENTER,
                    player.feet + PLAYER_CENTER - Vec3::Z * 3.,
                    PLAYER_HALF,
                );
                #[cfg(target_os = "vita")]
                drop(_ground_profile);
                let riding = solid
                    && player.velocity.z <= 1.
                    && !ground.start_solid
                    && ground.fraction < 1.
                    && ground.normal.z > 0.65;
                #[cfg(target_os = "vita")]
                let _publish_profile = crate::frame_profile::span("village_dynamic_publish");
                let dynamic = fixed
                    .iter()
                    .cloned()
                    .chain(
                        self.objects
                            .iter()
                            .enumerate()
                            .filter(|(j, o)| {
                                *j != i && o.solid && !(o.name == "openthis" && self.hatch_open)
                            })
                            .map(|(_, o)| o.collider.clone()),
                    )
                    .collect::<Vec<_>>();
                #[cfg(target_os = "vita")]
                {
                    publishes += 1;
                    published += dynamic.len() as u64;
                }
                world.set_dynamic(dynamic);
                #[cfg(target_os = "vita")]
                drop(_publish_profile);
                #[cfg(target_os = "vita")]
                let _rider_profile = crate::frame_profile::span("village_rider_contact");
                let mut carried = next.origin
                    + next.rotation() * o.pose.rotation().inverse() * (player.feet - o.pose.origin);
                if solid {
                    if riding {
                        #[cfg(target_os = "vita")]
                        {
                            riders += 1;
                        }
                        let Some((feet, normal)) = collider.rider_feet(carried) else {
                            #[cfg(target_os = "vita")]
                            {
                                rejected += 1;
                            }

                            continue;
                        };
                        carried = feet;
                        let trace = world.body_trace(player.feet, carried);
                        if trace.start_solid || trace.fraction < 1. || !world.body_clear(carried) {
                            #[cfg(target_os = "vita")]
                            {
                                rejected += 1;
                            }

                            continue;
                        }
                        player.ground_normal = normal;
                    } else if {
                        #[cfg(target_os = "vita")]
                        {
                            local_contacts += 1;
                        }
                        collider.touches(
                            player.feet + PLAYER_CENTER,
                            player.feet + PLAYER_CENTER,
                            PLAYER_HALF,
                        )
                    } {
                        #[cfg(target_os = "vita")]
                        {
                            rejected += 1;
                        }
                        continue;
                    }
                }
                #[cfg(target_os = "vita")]
                drop(_rider_profile);
                if riding {
                    player.feet = carried;
                    player.grounded = true;
                }
                let o = &mut self.objects[i];
                o.pose = next;
                o.collider = collider;
                o.time += step;
            }
        }
        {
            #[cfg(target_os = "vita")]
            let _hotspot = crate::frame_profile::span("village_final_publish");
            world.set_dynamic(fixed.iter().cloned().chain(self.colliders()).collect());
        }
        #[cfg(target_os = "vita")]
        crate::frame_profile::counts(&[
            ("village_substeps", substeps),
            ("village_object_visits", visited),
            ("village_poses_unchanged", unchanged),
            ("village_poses_changed", changed),
            ("village_poses_changed_solid", changed_solid),
            ("village_poses_changed_nonsolid", changed - changed_solid),
            ("village_local_contacts", local_contacts),
            ("village_set_dynamic_calls", publishes + 1),
            (
                "village_elements_published",
                published + fixed.len() as u64 + self.colliders().count() as u64,
            ),
            ("village_riders", riders),
            ("village_moves_rejected", rejected),
        ]);

        Ok(())
    }
}
