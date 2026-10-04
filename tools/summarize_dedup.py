"""Offline report tooling; never used by the launcher. Personal paths stay in runtime/."""
import csv
import math
import sys
from collections import defaultdict
from pathlib import Path


def percentile(values, percent):
    values = sorted(float(v) for v in values)
    return values[math.ceil(len(values) * percent / 100) - 1]


def read_groups(path, stage=None):
    groups = defaultdict(list)
    with path.open(encoding="utf-8-sig", newline="") as stream:
        for row in csv.DictReader(stream):
            if stage is None or row["stage"] == stage:
                key = (int(row["entries"]), row["duplicate_percent"], row["version"])
                groups[key].append(row)
    return groups


def report(root):
    times = read_groups(root / "times.csv")
    memory = read_groups(root / "memory.csv", "idle_end")
    lines = [
        "| 原始入口 | 重复比例 | 版本 | 索引项 | 启动 P50/P95 ms | F5 P50/P95 ms | 窗口消息 P50/P95 ms |",
        "|---:|---:|---|---:|---:|---:|---:|",
    ]
    keys = sorted(times, key=lambda k: (k[0], k[1], k[2] != "before"))
    for key in keys:
        rows = times[key]
        if len(rows) != 5:
            raise ValueError(f"expected 5 processes per group: {key}, got {len(rows)}")
        spans = []
        for metric in ("startup_ms", "refresh_ms"):
            spans.append("/".join(f"{percentile([r[metric] for r in rows], p):.2f}" for p in (50, 95)))
        # Median per-process quantiles, not a pooled percentile over all 500 messages.
        spans.append("/".join(f"{percentile([r[metric] for r in rows], 50):.2f}" for metric in ("query_p50_ms", "query_p95_ms")))
        label = "实机" if key[1] == "real" else f"{key[1]}%"
        lines.append(f"| {key[0]} | {label} | {key[2]} | {rows[0]['catalog_entries']} | " + " | ".join(spans) + " |")
    lines += ["", "| 原始入口 | 重复比例 | 版本 | 私有提交 P50/P95 MiB | 工作集 P50/P95 MiB | 进程峰值提交最大 MiB | 进程峰值工作集最大 MiB |", "|---:|---:|---|---:|---:|---:|---:|"]
    for key in keys:
        rows = memory[key]
        if len(rows) != 5:
            raise ValueError(f"missing memory processes: {key}")
        spans = []
        for metric in ("private", "ws"):
            spans.append("/".join(f"{percentile([r[metric] for r in rows], p) / 1048576:.2f}" for p in (50, 95)))
        for metric in ("peak_commit", "peak_ws"):
            spans.append(f"{max(int(r[metric]) for r in rows) / 1048576:.2f}")
        label = "实机" if key[1] == "real" else f"{key[1]}%"
        lines.append(f"| {key[0]} | {label} | {key[2]} | " + " | ".join(spans) + " |")
    starts = read_groups(root / "memory.csv", "idle_start")
    deltas = [int(end["cpu_100ns"]) - int(start["cpu_100ns"]) for key in keys for start, end in zip(starts[key], memory[key])]
    lines += ["", f"Idle: {len(deltas)} intervals x 0.5 s; nonzero CPU intervals={sum(v != 0 for v in deltas)}, total_cpu_100ns={sum(deltas)}"]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("runtime/dedup")
    output = report(root)
    (root / "summary.md").write_text(output, encoding="utf-8")
    print(output)
