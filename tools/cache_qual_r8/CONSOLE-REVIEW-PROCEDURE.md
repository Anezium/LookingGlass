# Adopted review procedure

Read AMENDMENTS-REVIEW.md with the sealed CONSOLE-BRIEF.md before admission.
M1–M5 and R1–R10 supersede conflicting brief wording. Engine and SELF are unchanged.
The retained images fix lateral motion on both maps; never adapt after results.
Historical Auto-load reporters are clean, so intervening load audio remains gated.

After each chapter entry or native Auto load, run `operate.py LABEL button start`.
Inspect the returned modal Main menu and its visible Circle Back/Resume control,
then run `ack.py --main-menu "Main menu inspected; Circle Back to Game visible"`.
Only then run `operate.py LABEL move WINDOW circle`, `check WINDOW circle`, or
`post WINDOW circle`. Cross is never a resume button in a measured window.
Every stopped window returns an image of the modal Main menu; inspect that image
and acknowledge it with `--main-menu` before starting the following check window.
Do not press Start again while already in that modal menu: it can resume the game.
The helpers consume a distinct inspected menu image for every window.
If Back/Resume on Circle is not visible, stop and follow the closed-title restoration path.

Chapter navigation uses Select → Chapters, D-pad with
`operate.py LABEL button down N` (1 ≤ N ≤ 8), inspecting and acknowledging every
returned image, then Cross to Begin. For Hedge3 inspect chapter index 32,
displayed row 33, Labyrinthine Revenge. Repeat bounded navigation actions as needed
from the inspected selection. There is no touch-helper dependency.

Before initial SELF staging, before each arm's staging/restoration, and before
final restoration, read AMENDMENTS-M4B.md and use `operate.py LABEL prove-closed`.
This records two `pilot.cmd('quit ALICELG01')` replies. The first must be `Quit.`
or `Error: cannot quit the app. Is the TITLEID correct?`; the second must be
exactly that cannot-quit error. This process kill writes no in-game autosave.
Inspect the returned HOME/LiveArea image showing no running Alice and run
`ack.py --home "HOME inspected; Alice closed"`.
Restoration helpers recheck with two quits immediately before writes; both must
report cannot-quit to retain the earlier image proof. A successful kill invalidates
that image and requires a fresh proof and HOME acknowledgement. Any other reply,
second `Quit.`, driver loss or missing HOME inspection leaves closure unproven:
stop, write no saves or SELF, and preserve the evidence.

On a stop or abort, release inputs and close the title with the authorized abort
action or have the human close it from LiveArea. Prove closure and inspect HOME
before any save restoration. Then run `session.py restore`. Once final or failure
restoration starts, gameplay inputs, launch, abort, and in-game Quit are blocked.
Never use in-game Quit after restoration has started: it would overwrite the restored Auto.
Normal pre-Auto save restoration while the inspected menu is open is retained;
any STOP/ABORT marker switches it to the mandatory closed-title failure path.

Post-load windows now use stride 64 in both arms. AUDIT.json includes descriptive
per-pulse motion estimates and keeps them on failed motion audits. They align the
first native row with the first host pulse request; there is no synchronized clock
or exact per-pulse claim. Native total moving time and path remain the gates.
Each movement B−A prepare_acoustics mean and p95 is reported. A default-on
recommendation requires both movement mean deltas ≤ +0.25 ms; otherwise written
human acceptance must weigh moving cost against the r6 idle gain. Helpers never
promote the default. A movement heap excess with differing retained runtime audio
memory lines produces UNKNOWN and requires a new matched pair under a new amendment.
Load and transition paired heap deltas remain strict. Crashes and OOM still fail.
