# Local Macroquad 0.4.14 patch

Source: crates.io macroquad 0.4.14, registry checksum preserved in the prior
Cargo.lock. Only library sources, their required built-in UI resources and
licenses are vendored. Examples and game data are excluded.

Material destruction previously removed only the Macroquad pipeline slot.
The backend pipeline and shader ResourceManager entries accumulated at every
reload. PipelineExt now remembers its shader, and the last Material owner
flushes queued draws and deletes both backend resources exactly once.
The four built-in shared-shader pipelines are unaffected.

The application lock keeps every dependency version unchanged; Macroquad now
resolves from this directory so the fix applies identically to host and Vita.
