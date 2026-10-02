"""Build PicoRun's compact first-reading dictionary from MIT-licensed pinyin-data.

This is a build-time tool; the launcher never runs Python.
"""
import argparse
import hashlib
import re
import struct
import unicodedata
from pathlib import Path


def ascii_reading(value):
    value = value.lower().translate(str.maketrans({character: "v" for character in "üǖǘǚǜ"}))
    value = "".join(character for character in unicodedata.normalize("NFD", value)
                    if unicodedata.category(character) != "Mn")
    if not value or not value.isascii() or not value.isalpha():
        raise ValueError(f"Unexpected pronunciation: {value!r}")
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    raw = args.input.read_bytes()
    readings = {}
    for line in raw.decode("utf-8-sig").splitlines():
        match = re.match(r"^U\+([0-9A-Fa-f]+):\s*([^#\s]+)", line)
        if match:
            codepoint = int(match[1], 16)
            if codepoint in readings:
                raise ValueError(f"Duplicate character: {codepoint:X}")
            readings[codepoint] = ascii_reading(match[2].split(",")[0])
    if not readings:
        raise ValueError("No pinyin records found")
    pool = bytearray()
    offsets = {}
    for syllable in sorted(set(readings.values())):
        offsets[syllable] = len(pool)
        pool.extend(syllable.encode("ascii") + b"\0")
    if len(pool) > 65536:
        raise ValueError("Dictionary exceeds the format's 16-bit pool offset")
    output = bytearray(struct.pack("<4sII", b"PRPY", 1, len(readings)))
    for codepoint, syllable in sorted(readings.items()):
        output.extend(struct.pack("<IH", codepoint, offsets[syllable]))
    output.extend(pool)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(output)
    print(f"source_sha256={hashlib.sha256(raw).hexdigest()}")
    print(f"records={len(readings)} bytes={len(output)} sha256={hashlib.sha256(output).hexdigest()}")


if __name__ == "__main__":
    main()
