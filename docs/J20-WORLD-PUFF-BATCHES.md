# Direct world-puff batches

`world_puff_batches` defaults on and is independently configurable. It applies only to retained world puffs with producer reuse and FX merging enabled. Attached producers and other renderer paths retain their existing publication path.

The numeric quad kernel and camera basis are unchanged. The world producer keeps raw four-vertex results instead of constructing a publication, pass and group for each puff. Descriptors retain their registration position relative to ordinary packets. The scheduler builds the same stable joint-alpha order, with world entries first on depth ties, then applies the existing first-fit material policy inside effect runs. World, actor, invalid and authored multi-pass packets remain barriers. Final batches retain complete pass metadata, EQUAL layers and the 60,000-vertex/180,000-index merge bounds.

Two phase banks retain CPU arrays, membership storage and immutable publications. Each final batch publishes once; held publications are never changed. Actor-layer and skinning implementations are untouched. Host samples now expose the existing producer/merge/finish/recycle timers; this does not supply Linux's missing deferred-tail hooks.

842 feature tests pass, including four new checks: comparison against the legacy merger with mixed materials/barriers/EQUAL layers and a nearly full vertex page; exact pass metadata; zero warmed producer/batch allocations; and immutable held publications. Initial fix-on host captures with the new switch on and off both match all 15 package-1 Linux reference images, including invalidation views, HUD and live steam. Warm world allocation maximum is zero in both arms.

The initial reference reduces 128 particle input groups to 9 final batches. Unpaired observed producer/merge/finish-prepare/recycle medians are 147/37/23/51 → 86/18/40/41 microseconds. These launches are preliminary observations, not an attributed speed gain. Final alternated advancing-clock pairs and all per-switch alpha/six-map captures are required for the package evidence.

The expected console saving remains 1.5–2.5 ms from the brief's producer/merge/finish budget. It is an estimate; host submission measurements do not prove it. The historical Mac depth-fix discrepancy remains **UNRESOLVED**, the fix stays on, and no package readiness or console acceptance is claimed. Evidence is under `/srv/fast/projects/alice/jalons/jalon-20/pkg-2/`; final notes and verification logs govern its measured status.
