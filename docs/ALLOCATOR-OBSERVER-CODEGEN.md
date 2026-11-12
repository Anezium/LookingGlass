# Allocator observer code generation

The native allocator gate traverses the complete functions reachable from all
ten public/reentrant wrappers, including direct calls, tail calls and compiler
landing pads. Only a wrapper's direct call to its exact matching real allocator
is a payload boundary. Ordinary entry-CFG reachability cannot waive a panic
callee in an exception landing pad.

The r13d ELF (`2167beec266255997b43bd85ccefa4bd56389368c802bf23f41417a5c3a2c902`)
retains `panic_cannot_unwind` at `0x813f63e8` in `__wrap_calloc`. Its overflowing
multiply branch calls the generic Rust `peak::Guard::enter` with a saturated
request, zero alignment and allocation=true. The old passing r13b/c ELF calls a
specialized saturated-request version. These three allocator source files did
not change between those checkpoints; surrounding thin-LTO code generation did.
ELF evidence does not identify the precise internal LLVM pass responsible.

`#[inline(always)]` on `Guard::enter` keeps the same observer operations visible
at the C wrappers, including that overflow branch. This changes only the local
inlining boundary. The atomic owner protocol, same-thread nesting, nonwaiting
contention fallback, saturated growth, sampling, real allocator arguments and
RAII Drop remain intact. The `extern "C"` ABI and its prohibition on unwinding
through the boundary remain intact. No exception is made for a residual landing
pad: the unchanged strict validator must pass on the complete newly linked ELF.
Inlining is not by itself proof of a no-panic closure.

Run the host regression using the project's existing Python and pinned Rust
toolchain, with an absolute new output directory:

```sh
RUSTUP_TOOLCHAIN=nightly-2026-10-11 python tests/native_allocator_observer.py --output /absolute/new/test-output
```

The runner extracts actual native wrapper, owner, guard and sampling bodies.
Host adapters supply thread IDs, matching payload results and `_mallinfo_r`.
Seven tests cover all ten matching payloads, unchanged arguments/return/errno
sentinels, null and zero requests, calloc overflow, default OFF, owner nesting,
cross-thread contention, latched ceiling, alignment/growth, sample cadence and
RAII release. The intentional caught host panic tests a Rust-scope guard drop;
it does not unwind through an `extern "C"` wrapper. The generated module and
source hashes, compile log and full test log are retained in the output.

These tests use host-width usize and mocked libc/OS. They do not certify libc
errno internals, ARM EH, real allocator failure behavior, OOM safety, timing,
stack high-water or console execution. The established offline native recipe
and unchanged ten-wrapper gate supply the separate linked ARM check. The
payload still includes normal newlib TLS/reentrancy, nested wrapped entries,
recursive malloc locks and the existing exhaustion diagnostic. Observer proof
does not certify those payload internals absent.

The r13d survey clock fix, exact 60+120 window, strict census guards, default-OFF
configuration and all game behavior are outside this change. A source/static
checkpoint does not authorize a console session; fresh entry and review remain
root-owned.
