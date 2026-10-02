# 基础框架验证

2026-10-03，本机 Rust MSVC release 构建，opt-level=3、fat LTO、codegen-units=1、panic=abort、strip、静态 CRT。当前环境为 Windows 11 x64；没有在低配 Windows 10 验证。

- `cargo fmt --check` 通过。
- `cargo test --offline` 通过：2 个拼音/数据完整性测试、5 个搜索/控制器/目录失败测试。
- `cargo clippy --offline --all-targets -- -D warnings` 通过。
- `cargo build --release --offline` 通过，无第三方 Rust crate。
- `weixin`、`wx`、`jsb`、`wyyyy`、`cqyh` 演示分别返回预期的合成应用名称；没有打开任何用户应用。
- 演示 exe 519,680 字节，搜索基准 exe 548,352 字节。这是磁盘大小，不是 RAM 占用。

`search_bench` 的一次探索运行，每组 1,200 次搜索调用，之前按 12 种输入预热 20 轮；计时只包围搜索函数，统计样本按 nearest-rank P95。合成标题由 5 类应用名追加序号生成；没有固定 CPU 亲和性、没有多次独立进程取中位数。这是基础基线，不能与之前不同算法和统计口径直接比较。

| 合成项数 | P50 ms | P95 ms |
| --- | ---: | ---: |
| 500 | 0.0304 | 0.0548 |
| 2000 | 0.1136 | 0.1960 |
| 10000 | 0.6173 | 1.1005 |

不含索引扫描、拼音生成、Edit 文本读取、绘制、完整进程占用、应用启动；当前还没有 GUI，因此没有“启动器已达到低内存目标”的结论。后续性能任务要同时测完整产品和纯核心，并保留忽略目录中的原始结果。

拼音原始文本 SHA256：`621f8ca9eff8519f47e2b17b564fd318161e13bca07eea8c8e04993cd5d3b52e`。
生成的 PRPY v1 数据 SHA256：`9a3fc49d2a28e476239029b25960259937df4a63aad0a01178ee2bef7ee06a0a`；44,435 条记录，268,376 字节。

