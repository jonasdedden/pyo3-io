"""One fresh-process binary bulk-read RSS measurement for bench.py --memory."""

import argparse
import io
import json
from pathlib import Path
import sys
import tempfile


def peak_rss():
    """Return the measured process high-water RSS in bytes, not allocator usage."""
    if sys.platform == "linux":
        # getrusage can retain a parent's pre-exec high-water mark. VmHWM belongs to
        # this executable's address space, so a large benchmark driver does not set
        # an artificial floor on every fresh measurement process.
        for line in Path("/proc/self/status").read_text().splitlines():
            if line.startswith("VmHWM:"):
                _, count, unit = line.split()
                if unit != "kB":
                    raise RuntimeError(f"unexpected VmHWM unit: {unit}")
                return int(count) * 1024
        raise RuntimeError("VmHWM is unavailable")
    import resource
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release"), default="release")
    parser.add_argument("--size", type=int, required=True)
    parser.add_argument("--source", choices=("BytesIO", "temporary-file"), required=True)
    parser.add_argument("--implementation", choices=("typed-detached", "typed-bound", "legacy", "filelike"), required=True)
    args = parser.parse_args()
    if args.size <= 0:
        parser.error("size must be positive")
    # macOS reports ru_maxrss in bytes. Do not guess metrics/units on other platforms.
    if sys.platform not in ("linux", "darwin"):
        print(json.dumps({"unavailable": "peak RSS measurement supports Linux and macOS"}))
        return
    sys.path.insert(0, str(Path(__file__).resolve().parent / "python"))
    from _extension import install
    install(args.profile)
    import pyo3_file_typed_tests as ext

    if args.source == "BytesIO":
        stream = io.BytesIO(b"x" * args.size)
    else:
        stream = tempfile.TemporaryFile("w+b")
        # Do not retain a whole Python input allocation for the real-file case.
        block = b"x" * min(args.size, 65536)
        remaining = args.size
        while remaining:
            remaining -= stream.write(block[:remaining])
        stream.flush()
        stream.seek(0)
    with stream:
        before = peak_rss()
        count, data = ext.bench_read(stream, args.implementation, 0, False)
        if count != args.size or data is not None:
            raise RuntimeError("incorrect benchmark result")
        after = peak_rss()
    print(json.dumps({"before_peak_bytes": before, "after_peak_bytes": after}))


if __name__ == "__main__":
    main()
