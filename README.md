# PicoRun

面向低配 Windows 10 机器的轻量原生应用启动器。Pico 表示小，Run 表示搜索后直接打开应用。

Rust + Win32，目标只包含应用搜索和启动，必须有全拼、拼音首字母和中文名称搜索。界面样式与搜索逻辑独立，方便以后调整外观。

## 当前状态

首个原生版本已可运行，包含：

- 预计算名称、全拼、首字母别名的应用模型。
- 44,435 条单字读音的紧凑只读拼音字典，以及少量明确的多音词覆盖。
- 独立的 exact / prefix / substring / subsequence 排名，固定前 12 项、稳定同分顺序。
- 用户/公共开始菜单与桌面的应用发现，保留 `.lnk` 的 Shell 启动。
- 原生 Edit 输入与 GDI 结果列表、全局热键、单实例、方向键选择；IME 组合/候选保护、确认键自动重复保护与取消恢复。
- 版本化校验缓存、损坏恢复、目录读取失败时保留可用入口、F5 手动刷新。
- Windows 托盘：单击呼出，右键打开、刷新、切换亮/暗主题、英文输入、应用图标、登录自启动或退出；任务栏重建时恢复图标。
- 独立 Theme/renderer/controller，亮/暗两套主题、保存主题选择、查询/结果/绘制快照复用、阻塞消息循环。
- 可选应用图标：托盘勾选开关、默认关闭、保存选择；只加载可见项，后台直接提取、共享资源与负缓存、有上限的内存缓存、局部重绘。
- 可选登录自启动：默认关闭，只在托盘明确启用时注册到当前用户的 Run 项，登录后隐藏到托盘；取消勾选删除注册。
- 可控启动程序与真实窗口探针、搜索流程测试、500/2000/10000 项合成搜索基准。

Store/UWP 枚举尚未实现；索引仅接受 `.exe` 和指向 `.exe` 的快捷方式，文档、目录、网页快捷方式不纳入。拼音使用字典首读音与少量词组覆盖，不能解决所有多音字。主题只提供亮色和暗色，初始默认暗色。

应用结果图标此前完成独立原型的 [成本评估](docs/ICON_EVALUATION.md) 和 [缓存优化对照](docs/ICON_CACHE_RESULTS.md)，现已加入正式版本，默认关闭。正式开关、关闭后资源释放与同场性能对照见 [图标开关验证](docs/ICON_SWITCH_RESULTS.md)。

随后完成 [图标缓存优化对照](docs/ICON_CACHE_RESULTS.md)：真实样本活动 CPU 中位数减少 14.2%、查询绘制中位数减少 15.0%；解析结果和同资源图标共享，并在图标完成时局部重绘。常驻变化很小，资源提取峰值仍需优化。

最新开关与内存对比见 [英文输入验证](docs/ENGLISH_INPUT_RESULTS.md)，IME 与 Flow Launcher 参考结论见 [原生 IME 验证](docs/IME_RESULTS.md)，此前见 [托盘、主题与优化验证](docs/TRAY_THEME_RESULTS.md) 和 [原生版本验证](docs/NATIVE_RESULTS.md)。已测 Windows 11 x64 的本机小狼毫/Rime；Windows 10、32 位、其他输入法、极低配和低内存换页尚未验证。真实 IME 输入后的常驻与完整启动峰值需要继续优化，不能据搜索核心速度承诺最低配体验。

登录自启动的注册、托盘菜单、跨进程保存及隐藏启动验收见 [自启动验证](docs/STARTUP_RESULTS.md)。已验证注册与保存命令的直接执行，尚未实际注销/重启电脑验证登录流程。

上下键选择采用两行局部重绘和可复用的单行缓冲，避免先清空整张列表再画文字；选中项不变时不重绘。缓冲在隐藏时释放。首次打开、亮暗主题、图标开关和窗口重开的验证及完整进程成本见 [选择重绘验证](docs/SELECTION_RENDER_RESULTS.md)。

## 运行与退出

构建后直接运行 `target\release\picorun.exe`，首次打开搜索窗口：

- Alt+Space 呼出/隐藏；↑↓ 选择；Enter 打开；Esc 或失去焦点隐藏。
- 隐藏或打开应用后保留输入内容；再次呼出时全选上次文本，直接输入即可替换，也可直接 Enter 再次打开当前结果。内容仅在本次进程运行期间保留，退出程序后不保存到磁盘。
- 呼出时按所在屏幕工作区和最大结果面板定位，输入框高度不随保留查询的结果数量变化；列表向下伸缩。文本选区使用亮/暗主题各自的柔和灰色，保持原生编辑与输入法行为。
- 鼠标左键点击其他结果行先选中，点击已选中的行打开应用；键盘选中或默认选中的首行也可直接点击打开。输入法组字/候选期间不打开，空白、边距和页脚不响应打开。
- 输入法组合/候选期间，Enter、Esc 和上下键先交给输入法；确认键松开后再按 Enter 才启动。空格选词后可直接 Enter 启动；隐藏取消尚未提交的组合。
- F5 刷新应用索引；Ctrl+Q 退出并释放热键；重开 exe 会呼出已有实例。
- 热键冲突会显示错误并退出。可运行 `picorun.exe --hotkey Ctrl+Alt+P` 换键。
- `--hidden` 启动时隐藏；托盘图标继续存在。右键托盘可退出或切换主题。
- 主题选择保存到数据目录的 `theme.txt`；`--theme light` / `--theme dark` 只覆盖本次启动，之后在托盘中选择主题会保存。
- 托盘右键的“呼出时使用英文输入”是持久化开关，默认关闭，勾选表示开启。在输入框取得焦点时，优先临时关闭当前输入法的组合输入，使用英文模式并保留原布局；不支持关闭时才尝试已安装的英文布局。呼出前保存原前台输入框的布局和可读取的输入法状态，隐藏、失焦、关闭选项或正常退出时恢复。窗口内仍可手动切换语言。不安装输入布局或更改系统键位。设置存放在 `english-input.txt`；切换、恢复请求或保存失败时显示状态提示。详见 [输入模式恢复验证](docs/INPUT_SESSION_RESULTS.md)。
- 托盘右键的“显示应用图标”是持久化开关，默认关闭，保存到 `icons.txt`。切换立即生效，保留查询和选择；`--icons on` / `--icons off` 只覆盖本次启动，在托盘切换才保存。关闭时停止并回收图标线程、缓存与显示快照；正在执行的系统资源提取需先返回。开启但未显示结果时不创建加载线程；F5 和 DPI 变化使缓存失效。

- 托盘右键的“登录时启动 PicoRun”默认关闭，勾选时在 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 注册 `PicoRun`，取消时只删除这个值，不需要管理员权限。启动命令保存当前 exe 的绝对路径、`--hidden`、热键和显式指定的数据/来源目录；主题、英文输入和图标选择仍从各自设置读取。移动 exe 或更改这些路径/热键后，需要重新勾选更新注册。菜单勾选表示当前命令已注册；Windows 自身禁用或策略可能影响实际执行。

缓存位于系统 LocalAppData 已知目录中的 `PicoRun\apps-v1.bin`，通常为 `%LOCALAPPDATA%\PicoRun\apps-v1.bin`；不写 exe 旁边。每次启动重新发现入口，缓存用于失败恢复与保存预计算键；缓存损坏会重建。F5 同步扫描，扫描期间有刷新状态提示。应用安装/删除后需要 F5 或重启，没有周期扫描。

默认来源是系统已知目录中的用户开始菜单 Programs、公共开始菜单 Programs、用户桌面和公共桌面，支持 Windows 重定向后的实际路径。递归扫描最多 16 层，跳过符号链接和重解析点；接受 `.exe` 与目标扩展名为 `.exe` 的 `.lnk`，显示名取入口文件名。相同入口路径先去重；同名（去首尾空白、忽略大小写）的普通本地快捷方式再比较目标路径、完整原始参数、工作目录、窗口模式、权限标志和扩展元数据。完全相同才合并，优先用户开始菜单、公共开始菜单、用户桌面、公共桌面，同一来源用路径稳定选择；显式 `--source` 按传入顺序优先。不同名称、启动差异、特殊/不完整快捷方式及裸 `.exe` 保留。去重只在启动/F5 扫描时进行，先合并再生成拼音键；打开仍交给 Shell 执行选中的原 `.lnk`。验证与成本见 [DEDUP_RESULTS.md](docs/DEDUP_RESULTS.md)。

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

`target\release\native_probe.exe --mouse` 通过实际鼠标按键、原生窗口消息和受控快捷方式，检查先选中再打开、键盘选中后点击、无结果/边距/页脚、输入法组字保护以及快速输入后点击。覆盖亮/暗主题、图标开关和 96/120/144/192 DPI 的窗口布局；验证范围见 [鼠标打开验证](docs/MOUSE_LAUNCH_RESULTS.md)，原始证据位于忽略的 `runtime/probe-mouse/`。

`target\release\native_probe.exe --input-session` 使用两个受控原生进程，验证原输入框处于中文和英文模式时的呼出、Esc、热键、失焦、受控 Enter 启动、关闭选项及正常退出，并在原输入框实际键入确认恢复效果。专用验证热键为 Ctrl+Alt+F10 / F11，退出时释放；仅操作自己的前台窗口，前台被其他程序夺走时停止按键注入。证据位于忽略的 `runtime/probe-input-session/`。

`target\release\native_probe.exe --startup` 在固定的非 Run 注册表命名空间中验证实际托盘菜单、保存、取消和重启，并直接执行受控启动命令；实际用户 Run 值仅作前后只读对照。临时验证项会清理，证据写入忽略的 `runtime/probe-startup/`。

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

