"""Argument filling by pointing only: mode/flags from `tar --help`, the file from a directory
listing. Nothing is generated; every value is an option the code offered."""
import json, subprocess, time
from router import jev, save_cache, TOKENS
HELP = subprocess.run(["tar", "--help"], capture_output=True, text=True).stdout
FILES = ["backup.tar", "photos/", "site-2024.tar.gz", "notes.md", "logs.zip", "README"]
MODES = {"-c": "Create an archive", "-r": "Add/Replace entries", "-t": "List contents", "-u": "Update", "-x": "Extract"}
FLAGS = {"-v": "Verbose: print each file as it is processed", "-z": "gzip compression", "-k": "Keep existing files",
         "-p": "Restore permissions", "-j": "bzip2 compression", "-O": "write entries to stdout"}
REQS = ["extract the gzipped archive and show each file as it goes",
        "list what's in the plain tar file without extracting anything",
        "make a gzip-compressed archive of the photos folder",
        "unpack the zip file"]
for req in REQS:
    state = {"request": req, "tar_help": HELP, "directory_listing": FILES}
    qs = {"mode": {"type": "choice", "instructions": "Which tar mode does `request` need, per `tar_help`?", "criteria": MODES},
          "file": {"type": "choice", "instructions": "Which entry of `directory_listing` is the archive or folder `request` refers to?",
                   "criteria": {**{f: None for f in FILES}, "NONE": "no entry in the listing matches"}},
          "tar_fits": {"type": "noul", "instructions": "Is tar (as described in `tar_help`) the right tool for `request`?",
                       "criteria": {"true": "tar can do this directly", "false": "this needs a different tool"}}}
    for f, d in FLAGS.items():
        qs[f"flag{f}"] = {"type": "noul", "instructions": f"Does `request` call for tar option `{f}` ({d})?"}
    t = time.perf_counter(); a = jev(state, qs)["answers"]; dt = time.perf_counter() - t
    flags = [f for f in FLAGS if a[f"flag{f}"]["noul"] > 0.5]
    print(f'"{req}"  ({dt:.2f}s)')
    print(f"   tar_fits={a['tar_fits']['noul']:.2f}  mode={a['mode']['choice']} ({a['mode']['confidence']:.2f})  "
          f"flags={flags}  file={a['file']['choice']} ({a['file']['probabilities'][a['file']['choice']]:.2f})")
    print("   flag probs:", {f: round(a[f'flag{f}']['noul'], 2) for f in FLAGS})
save_cache(); print(f"tokens {TOKENS['n']} (${TOKENS['n']*0.042e-6:.5f})")
