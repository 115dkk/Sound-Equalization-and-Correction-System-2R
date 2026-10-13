"""Sample Linux aggregate process-tree RSS/PSS every 10 ms.

Usage: /usr/bin/time -v python3 oracle/peak_memory.py -- COMMAND [ARG ...]
The sampler is excluded from the process tree. GNU time's high-water value
includes the sampler (normally smaller than the measured command); it is not
an aggregate. Sampling peaks are lower bounds, not kernel high-water marks.
"""
from __future__ import annotations

import argparse
import errno
import json
from pathlib import Path
import subprocess
import time


def descendants(pid):
    """Include children created by any thread, and deduplicate process IDs."""
    seen = set()
    pending = [pid]
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        try:
            tasks = list(Path(f'/proc/{current}/task').iterdir())
        except (FileNotFoundError, ProcessLookupError):
            continue
        for task in tasks:
            try:
                pending.extend(int(child) for child in (task / 'children').read_text().split())
            except (FileNotFoundError, ProcessLookupError):
                pass
    return seen


def kilobytes(path, key):
    try:
        for line in path.read_text().splitlines():
            if line.startswith(key + ':'):
                return int(line.split()[1])
    except OSError as error:
        # smaps_rollup may return ESRCH or EIO during process teardown.
        if error.errno not in (errno.ENOENT, errno.ESRCH, errno.EIO):
            raise
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command[:1] == ['--']:
        command = command[1:]
    if not command:
        parser.error('a command is required')
    child = subprocess.Popen(command)
    peak_rss = peak_pss = peak_processes = samples = 0
    started = time.monotonic()
    try:
        while True:
            tick = time.monotonic()
            pids = descendants(child.pid)
            rss = pss = processes = 0
            for pid in pids:
                root = Path(f'/proc/{pid}')
                rss += kilobytes(root / 'status', 'VmRSS')
                pss += kilobytes(root / 'smaps_rollup', 'Pss')
                processes += root.exists()
            peak_rss = max(peak_rss, rss)
            peak_pss = max(peak_pss, pss)
            peak_processes = max(peak_processes, processes)
            samples += 1
            if child.poll() is not None:
                break
            time.sleep(max(0.0, 0.010 - (time.monotonic() - tick)))
    except BaseException:
        child.terminate()
        child.wait()
        raise
    print(json.dumps(dict(peak_rss_kib=peak_rss, peak_pss_kib=peak_pss,
                          peak_processes=peak_processes, samples=samples,
                          elapsed_s=time.monotonic()-started, returncode=child.returncode)),
          flush=True)
    raise SystemExit(child.returncode)


if __name__ == '__main__':
    main()
