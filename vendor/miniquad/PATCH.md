# Local Miniquad 0.4.8 Vita fork patch

Source: Anezium/miniquad 6f891804784bf4a5b39313ee6499344cb0d7e8b6,
the exact fork previously locked by this application. Library sources, build
script, licenses and the fork's Vita change record are retained; examples and
their dev dependencies are excluded. No game data is included.

The OpenGL backend deleted texture handles but retained every Texture record in
a growing Vec forever. Managed texture metadata now uses the same ResourceManager
as shaders and buffers, with removal on deletion. IDs still increase monotonically
and are never reused, preserving weak surface/probe identity safety. GPU deletion
and VitaGL's deferred-free behavior are unchanged.

A pure metadata test executes 10,000 create/delete cycles with bounded storage
and verifies that old IDs never alias their replacements.
