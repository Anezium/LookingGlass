# Milestone 8 renderer experiments

All switches are strict fields in `vita_renderer.json`, under
`ux0:data/alice/private/` on Vita or `private/` in the host working directory.
Missing fields use defaults; an invalid file falls back to the complete defaults.
Build with `--features vita_renderer`. Without that feature the original paths remain.

`bone_palette` (default **false**) expands bone rotations and scales once per
skinned draw, including the model rotation. Weight order is retained but float
evaluation changes, so this is a tolerance-tested visual option. Animation,
attachment tags and gameplay retain their original transforms. This switch does
not change actor lighting.

`--skin-bench --data /path/to/base` runs without a window using the real Alice
SKL and sixteen run animation poses. Eight alternating rounds each skin 5,000
draws, including palette setup, non-unit bone scales and negative model scale.
The log gives median ns/vertex, all samples and maximum coordinate error.
Host x86 results are only a relative indication; Cortex-A9 timings require Vita.

`LOOKING_GLASS_FRAME_PROFILE=/path/to/output` now exports host `skinning`,
`frame_characters` and its `characters_*` decomposition, including positions,
normals, shading, layers and submission. Shared helpers outside the character
block contribute only to `skinning`. Native spans retain their existing owner
scopes. Profile medians are per invocation, not GPU timings.

Adapted MIT engine code: NextOs-Ports/Looking-Glass-NextOS 15779e4 (felc18-blip).
No NextOS platform bridge, Android implementation or ETC2 code is used.
