#!/usr/bin/env python3
"""Compare immutable native UI profiler binaries; build all variants first.

Each binary must contain the same macos-resource-profile driver. CPU percentage
is process user+system CPU time / elapsed wall time (100% = one CPU core).
The fixture uses native CoreText/Metal offscreen rendering, not the app engine.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
parser.add_argument("binaries", nargs=3, help="ellipsis, whole-glyph, per-pixel binaries")
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
frames = args.output.resolve() / "empty-frames.json"
frames.write_text("[]\n")
names = ["ellipsis", "whole-glyph", "per-pixel"]
binaries = [Path(p).resolve() for p in args.binaries]
metadata = {
    "binaries": {
        name: {"path": str(path), "sha256": hashlib.file_digest(path.open("rb"), "sha256").hexdigest()}
        for name, path in zip(names, binaries)
    },
    "cpu": subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip(),
    "os": subprocess.check_output(["sw_vers"], text=True).strip(),
    "note": "Three balanced-order trials; dev profile; 3s warmup, 10s idle, 10s forced redraw at 60 Hz; 25 sessions.",
}
(args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
results = []
# Rotate each variant through first/middle/last position to limit order bias.
for trial, order in enumerate([[0, 1, 2], [2, 0, 1], [1, 2, 0]], 1):
    for index in order:
        name = names[index]
        run_dir = args.output.resolve() / f"{name}-{trial}"
        run_dir.mkdir()
        env = os.environ.copy()
        env.update(ZERON_PROFILE_SIDEBAR_FADE="1", ZERON_PROFILE_BACKGROUND_CHATS="24", ZERON_VERIFY_SIDEBAR_ROWS="1")
        for key in ["ZERON_VERIFY_CACHE", "ZERON_VERIFY_INTERACTIONS", "ZERON_FRAME_STATS", "COMET_GPU_STATS"]:
            env.pop(key, None)
        with (run_dir / "stderr.log").open("w") as err:
            command = [str(binaries[index]), str(frames), str(run_dir)]
            with subprocess.Popen(command, env=env, text=True, stdout=subprocess.PIPE, stderr=err) as process:
                (run_dir / "process.json").write_text(json.dumps({"pid": process.pid, "command": command}, indent=2) + "\n")
                stdout, _ = process.communicate()
                returncode = process.returncode
        (run_dir / "stdout.log").write_text(stdout)
        if returncode:
            raise subprocess.CalledProcessError(returncode, command, stdout)
        phases = [json.loads(line) for line in stdout.splitlines() if line.startswith('{') and '"cpu_percent"' in line]
        assert {row["phase"] for row in phases} == {"warmup", "idle", "redraw"}, stdout
        results.extend(dict(variant=name, trial=trial, **row) for row in phases)
        (args.output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
        print(name, trial, [(row["phase"], round(row["cpu_percent"], 3)) for row in phases], flush=True)
