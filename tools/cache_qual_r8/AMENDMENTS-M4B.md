# M4B: title-closure proof

This amendment supersedes M4's proof mechanism only, before any data. All other brief and amendment requirements remain in effect.

Proof that ALICELG01 is closed:
1. send `quit ALICELG01` and record the reply (either `Quit.` or the cannot-quit error is acceptable);
2. send `quit ALICELG01` a second time; it MUST reply `Error: cannot quit the app. Is the TITLEID correct?`;
3. capture, inspect and acknowledge a HOME/LiveArea image showing no running Alice.
Any other reply, or a second `Quit.`, means the state is unproven: stop, write no saves or SELF, and preserve the evidence. Never use the in-game Quit after restoration has started.

The vitacompanion `quit <TITLEID>` command calls `sceAppMgrDestroyAppByName`: this process kill is equivalent to closing from LiveArea and writes no in-game autosave. Restoration rechecks must return cannot-quit twice; if a recheck kills an app, the earlier HOME image is invalid and a fresh proof and image acknowledgement are required before writes.
