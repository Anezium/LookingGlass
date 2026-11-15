# Default-off army-ant GPU skinning experiment

`gpu_skin_ants=false` retains the original shaders, ordered CPU geometry packets,
pose, tags, audio and gameplay. No new shader/attribute/uniform queries run while
it is off. `gpu_skin_capture=false` independently gates three diagnostic PNGs at
replay frames 1050, 1200 and 1470. Frozen release configuration files are unchanged.

The opt-in recognizes only the exact army_ant/armyant_baseframe.skb and
army_ant_corp/armyantcorp_baseframe.skb NPC asset pairs. Both measured assets have
up to five influences. No weights are truncated: five independent bone-local
position/weight lanes, five palette indices, stored normal and UV make a 120-byte
immutable vertex. A shared surface uploads once for the renderer lifetime. Strong
ownership prevents recycled-address cache hits. Resources die with that renderer.

Every accepted surface occurrence builds a remapped 3x4 f32 palette and captures
its visual world transform. At most 28 bones fit the conservative 128-vector
budget alongside the existing uniform layout. Observed surface maxima are 26.
The backend queries GL limits at runtime; miniquad has 16 attribute slots and this
declaration uses nine. A limit/compiler failure keeps CPU skinning. Native program
binary GXP headers log PA/SA, temporaries and instructions for both stages.

Only simple opaque live-ant layers are admitted. Dead ants retain CPU triangle
culling and the existing corpse fragment specialization; actor_opaque_shader also
keeps its reference path. Changed topology or UVs, extra influences
or palette bones, mutable live-mesh consumers, normal-dependent layers, flat
lighting, small-packet cuts and unsupported overlap modes keep the reference path.
Single preparation admits the GPU surface before worker publication; it never
changes pose evaluation. GPU ant groups are conservative frustum keeps because the
CPU packet culler cannot classify their placeholder vertices. GPU clipping still
applies. GPU groups are merge barriers and preserve the ordinary vertex budget.

The retained fragment source is unchanged. Vertex lighting approximates the CPU
path: it blends transformed stored normals and quantizes the same ambient/directional
lighting to bytes. The CPU instead accumulates cross products of deformed triangle
edges. These are not equivalent, and GPU floating-point association can also move
positions slightly. No image equivalence is claimed. All-frame difference counts,
maximum channel deltas and heatmaps must be reviewed explicitly before adoption.
The simulation/ordered non-ant packet oracle remains strict. Geometry exceptions
are identified explicitly; diagnostic cost counters are reported separately.

Host/native compiler evidence, timing rounds and the root-operated hardened step
package are outside Git at `/Users/anezium/vita-j0/plan-2026-11-12/a5-gpuskin/`.
REPORT.md there distinguishes measured results from native slot questions. A host
speedup is not a Vita result; keep the key off pending the owner's picture and
frame-time decision. The target walk window is frames 1000..1480 at <=33.3 ms.
