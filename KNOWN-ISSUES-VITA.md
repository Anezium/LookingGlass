# Known issues on PS Vita

This is a release candidate. Full-map play, fresh VitaShell installation,
films, sound by ear, save/load and clean exit still need release qualification.
The 20 fps playing floor has not been established across every map.

- Large fights and explosions can stall. Recent route and dead-ant fixes passed
  bounded replay checks; they do not prove every encounter is smooth.
- Rendering uses 0.75 scale. Water normals are off; water can look static and
  reflections or lighting can differ from desktop. Authored sky is off by default.
- Earlier play found geometry and smoke visible through walls and a missing
  death/reload interface once. Broad visual rechecking is still needed.
- Fortress2 has a reported camera problem; its current status is unknown.
- Startup checks pack names and sizes only, not their contents. Cold-start and
  first-map times on this package are unknown. Shader compilation can add load time.
- Original PC saves are not compatible. Game assets are supplied by the player.
