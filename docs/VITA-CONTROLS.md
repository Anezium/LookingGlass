# Vita handheld controls

The default preset uses the existing miniquad `6f891804` keyboard/mouse events.
Fresh Vita preferences and untouched older desktop defaults get this layout.
Custom bindings are preserved. To reset them, back up and remove only
`ux0:data/alice/private/preferences.json` before launch, or use the supplied preset.
No game data is changed.

| Physical control | Gameplay | Backend event / host emulation |
|---|---|---|
| Left stick | Move; always run | W/A/S/D |
| Right stick | Look | Mouse motion |
| R trigger | Primary attack (hold when needed) | Ctrl |
| L trigger | Alternate attack (hold when needed) | Shift |
| Square | Jump / climb / swim up | Space |
| Triangle | Interact / use | E |
| Circle | Dive / descend; back in menus | Backspace |
| Cross | Retry / skip scene; confirm / resume | Enter + left click |
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
