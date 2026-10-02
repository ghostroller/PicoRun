# PicoRun

面向低配 Windows 10 机器的轻量原生应用启动器。Pico 表示小，Run 表示搜索后直接打开应用。

Rust + Win32，目标只包含应用搜索和启动，必须有全拼、拼音首字母和中文名称搜索。界面样式与搜索逻辑独立，方便以后调整外观。

## 当前状态

首个无图标原生版本已可运行，包含：

- 预计算名称、全拼、首字母别名的应用模型。
- 44,435 条单字读音的紧凑只读拼音字典，以及少量明确的多音词覆盖。
- 独立的 exact / prefix / substring / subsequence 排名，固定前 12 项、稳定同分顺序。
- 用户/公共开始菜单与桌面的应用发现，保留 `.lnk` 的 Shell 启动。
- 原生 Edit 输入与 GDI 结果列表、全局热键、单实例、方向键选择；IME 组合/候选保护、确认键自动重复保护与取消恢复。
- 版本化校验缓存、损坏恢复、目录读取失败时保留可用入口、F5 手动刷新。
- Windows 托盘：单击呼出，右键打开、刷新、切换亮/暗主题、设置呼出时使用英文输入或退出；任务栏重建时恢复图标。
- 独立 Theme/renderer/controller，亮/暗两套主题、保存主题选择、查询/结果/绘制快照复用、阻塞消息循环。
- 可控启动程序与真实窗口探针、搜索流程测试、500/2000/10000 项合成搜索基准。

应用结果图标与 Store/UWP 枚举尚未实现；索引仅接受 `.exe` 和指向 `.exe` 的快捷方式，文档、目录、网页快捷方式不纳入。拼音使用字典首读音与少量词组覆盖，不能解决所有多音字。主题只提供亮色和暗色，初始默认暗色。

最新开关与内存对比见 [英文输入验证](docs/ENGLISH_INPUT_RESULTS.md)，IME 与 Flow Launcher 参考结论见 [原生 IME 验证](docs/IME_RESULTS.md)，此前见 [托盘、主题与优化验证](docs/TRAY_THEME_RESULTS.md) 和 [原生版本验证](docs/NATIVE_RESULTS.md)。已测 Windows 11 x64 的本机小狼毫/Rime；Windows 10、32 位、其他输入法、极低配和低内存换页尚未验证。真实 IME 输入后的常驻与完整启动峰值需要继续优化，不能据搜索核心速度承诺最低配体验。

## 运行与退出

构建后直接运行 `target\release\picorun.exe`，首次打开搜索窗口：

- Alt+Space 呼出/隐藏；↑↓ 选择；Enter 打开；Esc 或失去焦点隐藏。
- 输入法组合/候选期间，Enter、Esc 和上下键先交给输入法；确认键松开后再按 Enter 才启动。空格选词后可直接 Enter 启动；隐藏取消尚未提交的组合。
- F5 刷新应用索引；Ctrl+Q 退出并释放热键；重开 exe 会呼出已有实例。
- 热键冲突会显示错误并退出。可运行 `picorun.exe --hotkey Ctrl+Alt+P` 换键。
- `--hidden` 启动时隐藏；托盘图标继续存在。右键托盘可退出或切换主题。
- 主题选择保存到数据目录的 `theme.txt`；`--theme light` / `--theme dark` 只覆盖本次启动，之后在托盘中选择主题会保存。
- 托盘右键的“呼出时使用英文输入”是持久化开关，默认关闭，勾选表示开启。开启后每次呼出使用已安装的英文布局，隐藏/关闭选项时恢复呼出前的布局；窗口内仍可手动切换语言。只切本窗口的线程，不安装输入布局。设置存放在 `english-input.txt`；无可用英文布局或保存失败时显示状态提示。

缓存位于系统 LocalAppData 已知目录中的 `PicoRun\apps-v1.bin`，通常为 `%LOCALAPPDATA%\PicoRun\apps-v1.bin`；不写 exe 旁边。每次启动重新发现入口，缓存用于失败恢复与保存预计算键；缓存损坏会重建。F5 同步扫描，扫描期间有刷新状态提示。应用安装/删除后需要 F5 或重启，没有周期扫描。

验证选项 `--source <目录>` 可重复，替代系统应用目录；`--data-dir <目录>` 指定测试缓存目录。使用不同来源时应使用独立缓存目录。`--demo <查询>` 只查合成名称，不打开应用。

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

原生验收探针 `target\release\native_probe.exe --ime-real` 会在自己的前台窗口操作本机已安装的简体中文输入法，合成快捷方式只打开受控程序；`--real` 仅查询本机目录。测量边界和本次通过范围见 IME 报告。

核心模块可在其他系统编译；完整产品目标是 Windows。基准只计预热后的搜索调用，详见输出说明。

## 结构与下一步

```text
src/model.rs             应用与启动目标，索引时生成别名
src/catalog.rs           应用来源与稳定索引快照
src/pinyin.rs            紧凑只读字典与别名生成
src/search.rs            搜索与固定少量结果
src/ui/controller.rs     查询、结果与键盘选择
src/theme.rs             字体、颜色与几何参数
src/cache.rs             有校验与版本的紧凑缓存
src/platform/windows/    系统目录、Shell、热键、窗口及集中 FFI
src/ui/native.rs         独立 GDI renderer
src/bin/search_bench.rs  纯搜索基准
src/bin/native_probe.rs  完整进程、窗口与可控启动探针
assets/pinyin.bin        小字典的二进制数据
tools/build_pinyin.py    构建时字典生成器
```

范围与验收见 [实施说明](docs/IMPLEMENTATION.md)，最新结果见 [英文输入验证](docs/ENGLISH_INPUT_RESULTS.md)，输入法按键见 [原生 IME 验证](docs/IME_RESULTS.md)，首版基线见 [原生版本验证](docs/NATIVE_RESULTS.md)。此前实验与边界见 [研究记录](docs/RESEARCH.md)。字典来源、二进制格式和重建方法见 [拼音数据说明](assets/README.md)。

目前未选择整个项目的发布许可证。拼音数据来自 MIT 许可的 pinyin-data，许可文本见 [third_party/pinyin-data/LICENSE](third_party/pinyin-data/LICENSE)，来源说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。没有导入 WindMenu 或 ALTRun 的代码。

