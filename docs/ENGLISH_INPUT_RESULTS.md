# 英文输入开关与验证

2026-10-03：已有托盘菜单新增“呼出时使用英文输入”，默认关闭，勾选表示开启，重启保留。暂不增加设置窗口。原生菜单只在打开期间创建，消息循环继续阻塞等待，没有新增线程、轮询或第三方 Rust crate。

最终 `target/release/picorun.exe`：638,464 字节，SHA256 `55056038F3D204791C604C4B18572775961DC446EB8504C42FA8FE07FB9AAF16`。

交付时已用默认数据目录启动新版，确认窗口可见、Shell 能查询到托盘图标。`--theme light` 只临时展示亮色，没有替换已保存的主题或英文输入选择；启动证据在 `runtime/english-input-final-startup.json`。

## 行为与参考

Flow Launcher 的 General 设置确实使用 ToggleSwitch，绑定 `Settings.AlwaysStartEn`，默认值为 false，中文标签“以英文模式开始输入”。参考固定提交 `5a7519a03327d0eef926d1d8703aa5dbfa88305a`，只读研究，没有复制代码。[设置界面](https://github.com/Flow-Launcher/Flow.Launcher/blob/5a7519a03327d0eef926d1d8703aa5dbfa88305a/Flow.Launcher/SettingPages/Views/SettingsPaneGeneral.xaml#L507)、[默认值](https://github.com/Flow-Launcher/Flow.Launcher/blob/5a7519a03327d0eef926d1d8703aa5dbfa88305a/Flow.Launcher.Infrastructure/UserSettings/Settings.cs#L447)

PicoRun 开启后每次呼出激活已加载的英文布局，仅影响自身 UI 线程。先记录呼出前的 HKL，输入框取得焦点后激活英文；隐藏前恢复，关闭开关或正常退出也恢复。窗口内可以手动切换语言，下次呼出重新使用英文。此前回归检查发现焦点会恢复控件上次的布局，因此不能在取得焦点后才备份原布局。Win32 调用可能重入；备份使用独立 Cell，恢复前取走值，没有 Runtime 可变借用跨 FFI。

使用 `GetKeyboardLayoutList` 与 `ActivateKeyboardLayout(hkl, 0)`，不调用安装/卸载布局的 API，也不改变其他程序的输入模式。找不到可用英文布局时保留现有布局，并在窗口状态区提示失败；不会自动安装。[布局列表](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getkeyboardlayoutlist)、[当前线程激活与返回值](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-activatekeyboardlayout)

这与 Flow 的细节不完全相同：Flow 还通过 WPF 设置中文 IME 的首选 Off/Alphanumeric 状态；PicoRun 直接选择英文 HKL，中文组合/候选仍由原生 Edit/IMM 处理。[Flow 转换器](https://github.com/Flow-Launcher/Flow.Launcher/blob/5a7519a03327d0eef926d1d8703aa5dbfa88305a/Flow.Launcher/Converters/BoolToIMEConversionModeConverter.cs)、[布局辅助逻辑](https://github.com/Flow-Launcher/Flow.Launcher/blob/5a7519a03327d0eef926d1d8703aa5dbfa88305a/Flow.Launcher.Infrastructure/Win32Helper.cs#L482)

数据目录的 `english-input.txt` 保存 `on` / `off`，缺失、乱码、超长或未知值按关闭处理。最多读取 65 字节；临时文件加原子替换保存，主题 `theme.txt` 保持独立。保存失败提示当前选择可能在重启后恢复。

![托盘菜单中已勾选英文输入](images/english-input-menu.png)

## 功能验证

15 项单元/集成测试通过；`cargo fmt --check`、`cargo clippy --offline --all-targets -- -D warnings`、`cargo build --release --offline` 与 `git diff --check` 通过。

最终 release 的 `native_probe --ime-real` 完成 77 项 PASS 检查：实际原生菜单的 E 快捷字母打开并保存选项；显示切换、隐藏恢复、关闭/开启时保留已提交查询；手动切回非英文、下次呼出恢复英文；关闭后继续保留原布局；重启读取开启状态并恢复布局；验证器自身线程布局保持不变。截图确认菜单勾选。

真实按键 `weixin` 在英文布局形成实际 Edit 查询；关闭选项后本机小狼毫/Rime 的确认键长按、空格提交中文后立即 Enter、Esc 取消、隐藏/重开继续通过。Delete/粘贴、受控快捷方式参数和工作目录、F5、失效入口、缓存恢复、单实例、热键冲突与释放、主题/托盘、1000 次查询回归通过。只打开合成受控程序，没有打开个人应用。

## 完整进程内存对照

i7-11700K（8 核/16 线程）、32 GiB、Windows 11 Enterprise 25H2 build 26200 x64；Rust 1.95.0 MSVC，release opt-level=3、fat LTO、codegen-units=1、panic=abort、strip、静态 CRT、离线构建。MiB = 1,048,576 字节。本机已加载中文 HKL `0x8040804`，英文 HKL `0x4090409`，中文输入使用小狼毫/Rime。

`tools/measure_input_memory.ps1 -English -Runs 5` 与不带 `-English` 的相同命令，各 5 个独立进程，500 个合成快捷方式。每个进程先隐藏启动，设自己的原布局为中文，再呼出；英文组由开关切为英文，中文组开启自己的 IME 组合模式。真实键入相同六字母 `weixin`，间隔 50 ms，等 1 秒后取样；中文组保留组合，英文组形成直接查询。没有 Enter、Space 或 Shell 启动。没有输入预热，用新进程的首次真实输入观察组件加载成本。各组首次使用独立测试缓存，后四次复用；没有清空文件缓存，不称为磁盘冷启动。隐藏取消组合，再等 1 秒，记录 5 秒空闲 CPU。

采样为整个 PicoRun 进程的 PrivateUsage、WorkingSetSize、PeakPagefileUsage、PeakWorkingSetSize，包含其已加载的系统/输入法组件，排除测量器、Explorer、外部输入法服务及子进程，不是全系统增量。没有清空工作集。P50/P95 分别为五个独立进程的中位数/最近秩第 95 百分位；n=5 时 P95 就是最大值，不能据此预测低配尾延迟。

| 阶段 | 英文私有提交 P50/P95 MiB | 中文私有提交 P50/P95 MiB | 英文工作集 P50/P95 MiB | 中文工作集 P50/P95 MiB |
| --- | ---: | ---: | ---: | ---: |
| 隐藏启动 | 2.11 / 2.13 | 2.12 / 2.12 | 15.68 / 15.70 | 15.68 / 15.69 |
| 显示，尚未输入 | 3.09 / 3.11 | 3.11 / 3.16 | 21.23 / 21.23 | 21.20 / 21.21 |
| 首次真实输入 | 3.09 / 3.11 | 63.41 / 64.70 | 21.33 / 21.35 | 54.91 / 55.01 |
| 输入后隐藏 | 3.09 / 3.11 | 63.39 / 64.70 | 21.35 / 21.37 | 54.93 / 55.03 |
| 隐藏后空闲结束 | 2.99 / 3.01 | 63.35 / 64.60 | 21.38 / 21.39 | 54.93 / 55.04 |

到最后一次采样为止的累计峰值：英文组最大私有提交 3.11 MiB、工作集 21.40 MiB；中文组 65.77 MiB、55.99 MiB。英文组五次 5 秒空闲 CPU 均为 0 ms；中文组为 0、15.625、0、0、15.625 ms，保留全部观测，不承诺普遍零 CPU。

本机新进程的直接拼音输入，私有提交中位数减少约 60.32 MiB。若用户已在同一进程实际使用中文 IME，随后开启英文选项不会卸载已加载的 IME/GPU 组件；此对照不代表即时回收效果。继续使用中文组合时仍保留此前成本，也没有解决 Shell 启动峰值。完整功能探针含受控 Shell 启动和真实 IME 的累计峰值仍为私有提交 107.29 MiB、工作集 116.55 MiB，不与上述查询对照混合。

## 窗口响应与纯搜索

窗口指标单独来自 `native_probe --ime-real` 的 500 项合成索引：5 个独立进程，首次应用缓存缺失、第二次损坏、后三次有效，每次仍发现入口；每进程先预热 20 次，再测 120 次六种查询。它们是同步设置 Edit、处理查询并绘制的探针时间，不是物理键盘到屏幕显示延迟。

启动观察 P50/P95 285.14 / 405.39 ms；呼出加绘制 51.64 / 77.09 ms。输入每进程 P50 的中位数 3.95 ms、P95 的中位数 7.21 ms，各进程 P95 范围 6.40–10.55 ms。普通阶段（实际 IME 测试前）完整进程私有提交/工作集中位数：隐藏 2.12/15.70 MiB、显示 3.09/20.89、查询 3.11/21.26、刷新 3.21/21.42、空闲结束 3.11/21.43；五次 5 秒空闲 CPU 均为 0 ms。

纯搜索为一次独立基准进程，固定一个 CPU 核；每个规模先构建索引和测一次首次 search，再预热 20×12 查询，计时 100×12=1200 次，最近秩 P50/P95，包含中文、全拼、首字母、中英混合、无命中。没有更改排名算法，不把一次基准的波动解释为优化效果，也不等于完整窗口成本。

| 项数 | 构建 ms | 首次搜索 µs | 预热搜索 P50/P95 ms | 索引自有容量字节 |
| --- | ---: | ---: | ---: | ---: |
| 500 | 0.9551 | 44.8 | 0.0245 / 0.0474 | 121,074 |
| 2,000 | 2.5380 | 109.5 | 0.0969 / 0.2028 | 488,874 |
| 10,000 | 13.9945 | 858.2 | 0.6105 / 1.3719 | 2,461,674 |

原始证据位于忽略目录 `runtime/probe-input-english`、`runtime/probe-input-chinese`、`runtime/probe-controlled` 与 `runtime/search-bench-english-input.txt`。此前构建保存在 `runtime/english-input-baseline`。Windows 10、x86、其他输入法及低内存换页实机仍未验证；本次没有新增这些兼容性结论。
