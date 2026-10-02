# 前期证据与适用范围

2026-10-03，从 ALTRun 研究会话建立独立 PicoRun 项目。用户要求在“最差能带得动 Windows 10”的机器上运行，内存极宝贵，必须拼音搜索并打开应用；不需要插件、计算器、文件搜索。ZeroLaunch-rs 已因 Web 界面排除，不再测试。用户已决定独立重写，以便补齐拼音并改样式。

## 已测结果

此前环境：i7-11700K、32 GiB、Windows 11 build 26200、x64。纯核心实验固定一个 CPU 核，不能模拟最低配 CPU、慢磁盘或换页。

| 对象与状态 | 本机实测 | 限制 |
| --- | --- | --- |
| ALTRun apps-only，345 项 | 搜索 P95 2.653 ms；私有提交约 11.18 MiB，工作集约 22.18 MiB | 旧实验口径，完整配置与报告应回查 |
| 同算法 Rust 全拼核心，345 项 | P95 0.0328 ms；私有提交 1.371 MiB，工作集 7.320 MiB | 小核心原型，带隐藏 Win32 控件桩；不是完整启动器 |
| 同算法 C++ 全拼核心，345 项 | P95 0.0304 ms；私有提交 1.523 MiB，工作集约 7.44 MiB | 运行库和构建不同，不能全部归因语言 |
| WindMenu 官方 0.7，隐藏守护进程 | 私有提交约 1.246 MiB，工作集 7.863 MiB | 未打开其搜索窗口，没有自动拼音 |
| WindMenu 等价评分优化，345 项 | 同场 P95 0.5492 → 0.0311 ms | 原评分、完整结果；独立核心实验，没集成 GUI |
| WindMenu 等价评分优化，10000 项 | 同场 P95 13.9046 → 0.725 ms | 没有新增拼音；不与 PicoRun 简单排名等价 |

核心语言不是已知瓶颈。WindMenu 主要浪费在候选字符/词边界重复预处理、每 DP 行分配、内层重复 Unicode 小写转换和无匹配项完整评分。缓存不变量、复用行缓冲、先排除不可能命中，真实样本预热 alloc/realloc 可降为 0，核心保留缓存约 70 KiB。这些原则可以独立实现，不能直接拷贝 GPL 代码。

界面源码还发现重复计算分页、只靠 WM_CHAR 刷新而遗漏其他文本变化、呼出重建窗口和字符串等成本；当时没有实际窗口延迟数字。首版应采用真实文本变化通知并限制绘制/图标成本，不把核心微秒测量冒充按键到屏幕时间。

## 可读的原始证据

以下在同一机器上，只读参考，不属于 PicoRun 构建依赖。禁止复制其中的 GPL 实现；允许查报告、结果和测试方法。真实应用路径不进入这个仓库。

- `D:\Workspace\ALTRun\Tests\Performance\README.md`
- `D:\Workspace\ALTRun\Tests\Performance\RustAndAlternatives.md`
- `D:\Workspace\ALTRun\Tests\Performance\ThirdPartyResults.md`
- `D:\Workspace\ALTRun\Tests\Performance\WindMenuAudit.md`
- `D:\Workspace\ALTRun\Tests\Performance\results\rust-summary.json`
- `D:\Workspace\ALTRun\Tests\Performance\results\windmenu-audit.json`

PicoRun 当前核心是新的简单排名，不能继承旧原型的速度或等价性结论。需要测试本项目自己的完整实现；当前可运行的合成基准只测预热后 search 调用。

## 2026-10-03：Flow Launcher 与 IME

只读研究官方 Flow Launcher 提交 `5a7519a03327d0eef926d1d8703aa5dbfa88305a` 的 TextBox、TextChanged/KeyUp 同步、WPF KeyBinding 与可选 AlwaysStartEn 设置；借鉴标准控件处理文字、正确区分输入法按键的原则，没有导入 WPF 或 Flow 代码。PicoRun 用原生 Edit/IMM 管理组合、候选和按键直到释放的归属，真实小狼毫输入已验收。实现、固定来源链接与完整进程成本见 [IME_RESULTS.md](IME_RESULTS.md)。

随后核实 AlwaysStartEn 在 Flow 通用设置中是默认关闭的 ToggleSwitch。PicoRun 按用户选择加入托盘勾选开关，保存到小文本文件，临时激活自身 UI 线程的已加载英文 HKL，隐藏时恢复。没有增加设置窗口；本机 500 项索引、各 5 个独立进程的英文/小狼毫六字母输入对照见 [ENGLISH_INPUT_RESULTS.md](ENGLISH_INPUT_RESULTS.md)。收益主要来自避开本机中文组合输入路径，不能把已加载的 IME/GPU 组件常驻或 Shell 启动峰值说成已解决。

