# PicoRun

面向低配 Windows 10 机器的轻量原生应用启动器。Pico 表示小，Run 表示搜索后直接打开应用。

Rust + Win32，目标只包含应用搜索和启动，必须有全拼、拼音首字母和中文名称搜索。界面样式与搜索逻辑独立，方便以后调整外观。

## 当前状态

这是独立项目的基础框架，已经包含：

- 预计算名称、全拼、首字母别名的应用模型。
- 44,435 条单字读音的紧凑只读拼音字典，以及少量明确的多音词覆盖。
- 独立的 exact / prefix / substring / subsequence 排名，固定前 12 项、稳定同分顺序。
- 应用发现和启动接口、可测试的窗口控制状态、独立主题参数。
- 不打开任何应用的命令行演示、搜索流程测试、合成搜索基准。

**全局热键、原生窗口、系统应用发现、实际启动、缓存和图标尚未实现。** 默认启动会说明这一状态。拼音使用字典首读音，不能处理所有多音字；这版搜索未声称等价于之前测试过的 ALTRun/WindMenu 核心，也尚未测完整 GUI 的内存。

## 构建与验证

需要 Rust MSVC 工具链和 Windows SDK。项目目前无第三方 Rust crate 或网络运行时依赖，使用静态 CRT。

```powershell
cargo fmt --check
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo build --release --offline
cargo run --release --offline --bin picorun -- --demo weixin
cargo run --release --offline --bin picorun -- --demo wx
cargo run --release --offline --bin search_bench
```

核心模块可在其他系统编译；完整产品目标是 Windows。基准只计预热后的搜索调用，详见输出说明。

## 结构与下一步

```text
src/model.rs             应用与启动目标，索引时生成别名
src/catalog.rs           应用来源与稳定索引快照
src/pinyin.rs            紧凑只读字典与别名生成
src/search.rs            搜索与固定少量结果
src/ui/controller.rs     查询、结果与键盘选择
src/theme.rs             字体、颜色与几何参数
src/platform/windows.rs  Windows 系统边界，当前占位
src/bin/search_bench.rs  纯搜索基准
assets/pinyin.bin        小字典的二进制数据
tools/build_pinyin.py    构建时字典生成器
```

下一步任务和验收见 [实施说明](docs/IMPLEMENTATION.md)。此前实验与边界见 [研究记录](docs/RESEARCH.md)。字典来源、二进制格式和重建方法见 [拼音数据说明](assets/README.md)。

目前未选择整个项目的发布许可证。拼音数据来自 MIT 许可的 pinyin-data，许可文本见 [third_party/pinyin-data/LICENSE](third_party/pinyin-data/LICENSE)，来源说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。没有导入 WindMenu 或 ALTRun 的代码。

