# Vita handheld controls

The default preset uses miniquad `6f891804` button/look events and a VitaSDK
analog read for movement magnitude.
Fresh Vita preferences and untouched older desktop defaults get this layout.
Custom bindings are preserved. To reset them, back up and remove only
`ux0:data/alice/private/preferences.json` before launch, or use the supplied preset.
No game data is changed.

| Physical control | Gameplay | Backend event / host emulation |
|---|---|---|
| Left stick | Move; partial stick walks, full stick runs | Vita analog; host W/A/S/D or XInput stick |
| Right stick | Look | Mouse motion |
| R trigger | Primary attack (hold when needed) | Ctrl |
| L trigger | Alternate attack (hold when needed) | Shift |
| Cross | Jump / climb / swim up; confirm / resume | Enter + left click |
| Square | Interact / use | Space |
| Triangle | Retry / skip scene | E |
| Circle | Dive / descend; back in menus | Backspace |
| D-pad left / right | Previous / next available toy | Left / Right |
| D-pad up | Cheshire hint | Up |
| D-pad down | Inventory (all toys reachable by cycling) | Down |
| Select | Chapter / level chooser | Tab |
| Start | Pause/menu; resume/back | Escape |

Menus use D-pad navigation, Cross confirm and Circle/Start back. Saves and loads
are available through Start, including Continue and all save slots. There is no
separate crouch, map or journal action in the current engine; Ctrl's stock action
is dive/descend. Inventory and the chapter chooser are available as above.
The optional desktop run toggle, camera toggle, debug/help/recovery and direct
numbered toy shortcuts are unbound; all ten available toys can be cycled.

On the host, launch with `LOOKING_GLASS_VITA_CONTROLS=1` and use the backend-event
column. This applies the preset in memory without saving it automatically.
Gameplay bindings remain editable through preferences. Menu keys remain fixed.
Attack inputs must be released after menu/load boundaries, as on desktop.
Console reachability (including swim/climb, all toys and cinematic skip) remains
subject to the human protocol; a host check is not console acceptance.

Vita gameplay reads the analog stick through VitaSDK without changing miniquad's
pin. A radial deadzone of 0.24 is remapped to 0..1; below 0.65 walks, at/above
0.65 runs (about 73.4% raw deflection). Direction and magnitude are retained.
Returning from a modal screen requires the stick to return to neutral first.
Host keyboard emulation walks by default; an XInput stick exercises both speeds.
Vita action prompts use physical button names, including D-pad directions.

D-pad Down opens inventory without changing the selected toy on that frame.
Inside the chapter chooser Down scrolls; it cannot open inventory. Cross confirms
the highlighted chapter and ignores its duplicate pointer event. Inventory
confirmation uses the same guard, so Cross cannot select the stale cursor toy.
Touch still works.
The original preset is replaced only by resetting preferences or installing the
supplied r2 preset after backing up custom preferences.
