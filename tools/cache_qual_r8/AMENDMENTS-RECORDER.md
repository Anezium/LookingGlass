# Recorder export

The recorder now exports at most once per new command id. An id that was already applied does not change qualification state and does not write cache-qual.json. That includes the staged inert command id 0, action stop. A new stop id still exports once. The single clean-exit export after the viewer loop is unchanged.

The new SELF hash supersedes 892d7680285ac9441f81fcb4321706e943dbe19c1fe4f0c349e8e09c949e8ef2.

Common SELF, both arms: 46e71379df8d1c0140e00bb5134376c81bda2096ae5a37f738e5a66487ebde22.

Every other rule is unchanged. Cache and recorder remain default-off. Console qualification is not executed.
