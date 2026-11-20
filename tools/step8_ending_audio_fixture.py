#!/usr/bin/env python3
"""Exercise the viewer's ending-film barrier on a private Linux/Xvfb display.

Run with the locked host probe binary and an unmodified release config:
    ending_audio_fixture.py BINARY DATA CONFIG OUTPUT
"""
import argparse
import ctypes
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time


def wait_for(text, log, process, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if text in log.read_text(errors="replace"):
            return
        if process.poll() is not None:
            break
        time.sleep(0.05)
    raise RuntimeError(f"Missing {text!r} in {log}")


def skip_film(log, process):
    x11 = ctypes.CDLL("libX11.so.6")
    xtest = ctypes.CDLL("libXtst.so.6")
    display = ctypes.c_void_p
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = display
    x11.XKeysymToKeycode.argtypes = [display, ctypes.c_ulong]
    x11.XKeysymToKeycode.restype = ctypes.c_ubyte
    x11.XFlush.argtypes = [display]
    x11.XCloseDisplay.argtypes = [display]
    xtest.XTestFakeKeyEvent.argtypes = [display, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    connection = x11.XOpenDisplay(None)
    if not connection:
        raise RuntimeError("Could not open the fixture's X display")
    try:
        # Wait until the real film loop has observed released controls and armed skip.
        wait_for("film=ending owner=roq state active=true down=false armed=true held_s=0", log, process)
        enter = x11.XKeysymToKeycode(connection, 0xff0d)
        if not xtest.XTestFakeKeyEvent(connection, enter, 1, 0):
            raise RuntimeError("Could not hold Enter for the ending film")
        x11.XFlush(connection)
        try:
            wait_for("film=ending owner=roq decoder_exit reason=skipped", log, process)
        finally:
            xtest.XTestFakeKeyEvent(connection, enter, 0, 0)
            x11.XFlush(connection)
    finally:
        x11.XCloseDisplay(connection)


def run(binary, data, config, output):
    private = output / "private"
    private.mkdir()
    settings = json.loads(config.read_text())
    settings.update(diag_snapshot=False, sky_live=False, replay_play="", replay_record="",
                    story_film_bypass=False, audio_music_thread=False)
    (private / "vita_renderer.json").write_text(json.dumps(settings))
    env = dict(os.environ, LIBGL_ALWAYS_SOFTWARE="1", GALLIUM_DRIVER="llvmpipe", LP_NUM_THREADS="1")
    launch = [str(binary), "--data", str(data)]
    with (output / "seed.log").open("w") as log:
        subprocess.run(launch + ["--qlair-save-write"], cwd=output, env=env,
                       stdout=log, stderr=subprocess.STDOUT, timeout=120, check=True)
    saves = private / "qlair-work/save-fixtures/ending"
    assert (saves / "slot1.json").is_file(), "The finale writer did not emit the ending save"
    env.update(LOOKING_GLASS_SIM_MAPS_PROBE="1", LOOKING_GLASS_SIM_MAPS_DT="0.05",
               LOOKING_GLASS_SIM_MAPS_RESUME_LOAD="1", LOOKING_GLASS_PROBE_KEEP_WINDOW="1",
               LOOKING_GLASS_AUDIO_PROOF="1", LOOKING_GLASS_SAH_SILENT_AUDIO="1")
    path = output / "viewer.log"
    with path.open("w") as log, subprocess.Popen(
        launch + ["--save-dir", str(saves), "--load", "slot1", "--frames", "4"],
        cwd=output, env=env, stdout=log, stderr=subprocess.STDOUT,
    ) as process:
        try:
            wait_for("[lg/film] id=ending owner=roq start", path, process)
            skip_film(path, process)
            if process.wait(timeout=45):
                raise RuntimeError(f"Viewer failed. See {path}")
        finally:
            if process.poll() is None:
                process.kill()
    log = path.read_text(errors="replace")
    assert "Viewer failed:" not in log and "Ending film failed:" not in log
    barrier, = re.finditer(r"^AUDIO_STEP_BARRIER frame=(\d+) barrier=ending-film ops=([1-9]\d*)$", log, re.M)
    frame, ops = map(int, barrier.groups())
    assert "barrier=ending-continue" not in log, "Audio remained queued when the film returned"
    start = log.index("[lg/film] id=ending owner=roq start")
    handoff = log.index("[lg/film] id=ending owner=roq end=skipped")
    resumed = log.index(f"AUDIO_MIX frame={frame} ")
    assert barrier.start() < start < handoff < resumed, "Ending barrier order changed"
    assert "World interactions:" in log, "Viewer did not exit cleanly after the resumed frame"
    mix = re.findall(r"^AUDIO_MIX .*$", log, re.M)
    decode = re.findall(r"^AUDIO_DECODE .*$", log, re.M)
    assert mix and decode, "The audio proof stream was not armed"
    receipt = dict(frame=frame, prefix_ops=ops, film="skipped", continuation_ops=0,
                   mix=mix, decode=decode)
    (output / "proof.json").write_text(json.dumps(receipt, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "data", "config", "output"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--private-display", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    paths = [path.resolve() for path in (args.binary, args.data, args.config, args.output)]
    if args.private_display:
        run(*paths)
    else:
        args.output.mkdir(parents=True)
        subprocess.run(
            ["xvfb-run", "-a", "-e", str(paths[3] / "display.log"), "-s",
             "-screen 0 1280x720x24 -nolisten tcp", sys.executable,
             str(Path(__file__).resolve()), *map(str, paths), "--private-display"],
            timeout=180, check=True,
        )


if __name__ == "__main__":
    main()
