# 2026-10-05 项目审查修复

本轮修复项目审查发现的七项问题，没有新增运行时依赖。Windows 10、32 位、ARM64 和低配实机没有新增验证；Windows 10 和低配实机测试按用户要求暂缓。用户原有的 `SELECTION_RENDER_RESULTS.md` 修改和 `LAUNCHER_COMPARISON.md` 保持原样。

## 修复内容

| 问题 | 最终行为与回归证据 |
| --- | --- |
| 图标线程退出丢失唤醒 | 停止标志的发布与等待条件使用同一互斥锁。关闭接收通道后停止，再在锁释放后 join。受控交错测试能检出旧逻辑，修复版本通过，所有等待有界。 |
| 已知来源暂不可访问时丢失缓存项 | 仅应用来源查询使用 `KF_FLAG_DONT_VERIFY`，离线配置路径由实际扫描记录为失败目录，再保留该目录的旧入口。LocalAppData 保留验证路径的原行为；Shell 返回分配在失败时也释放。 |
| Unicode 路径去重误合并 | 扫描、失败回补和快捷方式启动键使用 UTF-16 路径键，只折叠 ASCII 大小写并统一斜杠。保守保留非 ASCII 大小写变体，不声称实现所有 Windows 文件系统的路径等价。两个实际 Unicode 文件、重叠来源、无效代理项以及目标/工作目录差异均有回归。目录回补检查组件边界。 |
| 初始化期间第二实例丢失呼出 | 保留安装器使用的原命名 mutex，并在主线程持有 ownership。就绪事件在热键和托盘初始化成功后发布；第二实例阻塞等待事件或首进程退出，最长十秒。超时/首进程退出给出双语错误，不再静默成功。主进程空闲仍为阻塞消息循环，无新增轮询线程或定时器。 |
| 第二实例无法转交前台焦点 | 第二实例获取原窗口 PID，调用 `AllowSetForegroundWindow` 后投递显示请求。Windows 仍可拒绝激活；此时恢复输入备份、提示点击输入框，不继续切换英文模式。 |
| UNC 安装卸载残留自启动 | 安装器的归属函数支持正常盘符绝对路径和完整 UNC 路径，规范化后与本次安装 exe 比较。其他 server/share/副本、相对路径、不完整路径及 device namespace 保守保留。 |
| 包验证启动失败遗留进程 | `Start-Launcher` 内部持有创建的确切 Process 对象，异常时核对 exe 路径、停止该进程并释放对象，不按名称结束其他进程。 |

新路径键和去重结构只在扫描期间使用，没有增加应用模型或磁盘缓存的常驻元数据。新就绪事件按 RAII 关闭；原 mutex 在普通返回时释放，崩溃时等待方能处理 abandoned 状态。没有调用清空工作集 API，也没有改真正的登录自启动注册或个人应用清单。

## 验证

最终源码的 `cargo test --offline` 通过 42 项单元测试及 5 项集成测试，共 47 项；`cargo fmt --check`、`cargo clippy --offline --all-targets -- -D warnings`、`cargo build --release --offline` 均通过。默认沙箱阻止过路径查找、文件替换、CIM 和隔离注册表读取；这些检查在自动批准的沙箱外执行后通过，没有把环境拒绝计为代码缺陷。

`native_probe --instance` 最终通过 14 项真实窗口断言：三轮各 10,000 个不执行的受控 exe、新建数据目录，在 mutex 已存在且窗口尚未创建时启动第二实例，确认原隐藏进程显示、缓存包含全部条目、正常退出释放窗口和 mutex。另一不同 PID 的受控原生前台窗口实际启动第二实例，确认原 PicoRun 成为 `GetForegroundWindow`。最初沙箱内尝试失败，最终沙箱外约 3.65 秒的成功运行单独保留；这不是低配或冷磁盘耗时结论。

PowerShell 7 与 Windows PowerShell 5.1 分别运行 `verify_packaging_edge_cases.ps1`，各通过 21 个实际编译执行的生产 Pascal 归属样本及 5 个进程失败清理断言，包含同路径的另一个进程继续存活。UNC 验证执行的是生产归属函数，没有建立 SMB 共享或进行真实网络安装。

旧英文探针直接跨进程投递 SHOW，没有确认前台资格；因此激活被拒绝时，其英文断言没有有效输入会话。本轮调整测试驱动：每次正向输入检查必须确认 `GetForegroundWindow`；必要时结束失败会话，安全点击验证进程所持有的窗口空白处，确认父窗口焦点，再以成功呼出实际备份的布局检验恢复。没有修改生产英文逻辑或跳过断言。`native_probe --english` 定向验证通过 23 项断言，包含实际走到激活重试分支，证据 `runtime/probe-english-review/checks.txt`、`runtime/review-fixes/english-targeted-output.txt`。

调整后的完整 `native_probe` 通过 106 项断言、0 失败，exit=0，约 60 秒。覆盖真实 Edit 删除与剪贴板、立即 Enter 同步、中文及混合查询、受控 `.lnk` 的参数和工作目录、查询回忆、托盘与热键、主题、英文输入恢复、注入的 IME 状态、损坏缓存恢复、第二实例、1,000 次查询及正常退出释放资源。没有新增真实安装输入法组字验证，也未运行 `--real` 读取个人清单。正常退出后无探针或 PicoRun 遗留进程。新记录复制到 `runtime/review-fixes/controlled/`，与本轮 `runtime/probe-controlled/` 的三份文件哈希一致。

最终分发包通过 `verify_windows_package.ps1` 的 53 项隔离生命周期断言，覆盖安装、升级、运行时拒绝覆盖、卸载归属、ZIP 窗口及 manifest/哈希核对。真实用户 Run、设置与清单的哈希前后相同。证据：`runtime/probe-packaging/d6c09c36a8a54d5887d075349cfe23e7/checks.txt`。在英文测试驱动调整后重新执行了全部 Cargo 检查，并从冻结源码重新构建包；包内 exe 哈希精确匹配最终显式 x64 构建。此前包的验证另存于 `runtime/probe-packaging/32c370eb3ed04d38bd33abdda055dae4/`。

原始证据放在忽略目录：`runtime/probe-instance/`、`runtime/instance-probe-final-output.txt`、`runtime/review-icon-stop/`、`runtime/probe-packaging-edge/6375437ee48d42f083e5f66988109a74/`、`runtime/probe-packaging-edge/a3bd3ece15754f5b8361db131ed45ca4/`。本轮首次综合窗口尝试的英文输入检查失败保留在 `runtime/review-fixes/controlled-first-error.txt`；此前的成功记录另存于 `runtime/review-fixes/before-controlled/`，不计作本轮成功证据。

## 测量环境与搜索核心

机器为 i7-11700K @ 3.60 GHz、32 GiB RAM（系统可见 31.84 GiB），Windows 11 企业版 x64 build 26200。构建参数见下方产物说明；机器元数据在 `runtime/review-fixes/metadata.json`。以下是当前版本观察值，没有配对前后对照，不声称性能提升。

`search_bench` 顺序执行五个独立 release 进程，每个固定到一个 CPU。每档合成 500/2,000/10,000 项，12 种中英文、拼音、首字母、混合及空/无匹配查询；先预热 20 × 12 次，再测量 100 × 12 = 1,200 次。每进程内部样本排序，P50 取下中位数，P95 取 nearest-rank；下表是五次进程分位值的中位数，括号为最小至最大值，没有将各进程分位数称为合并样本分位数。

| 合成入口数 | 搜索 P50（ms） | 搜索 P95（ms） |
| ---: | ---: | ---: |
| 500 | 0.0236（0.0232–0.0246） | 0.0428（0.0410–0.0547） |
| 2,000 | 0.0973（0.0940–0.1008） | 0.1899（0.1626–0.2029） |
| 10,000 | 0.5122（0.4907–0.5186） | 0.8836（0.7967–0.9784） |

这是预计算入口后的搜索核心计时，不含发现、拼音生成、窗口绘制或应用打开。首个搜索和索引构建的原始耗时另行保留，不计入预热分位数；未清理系统文件缓存，不称为冷磁盘启动。原始文件 `runtime/review-fixes/search-bench-{1..5}.txt` 及 `search-summary.json`；其中的 benchmark 进程内存不能代替 PicoRun 完整进程指标。

## 完整进程内存

上述综合窗口回归使用 500 个受控应用快捷方式、默认关闭图标、暗色主题，顺序启动五个独立 PicoRun 进程。按每个 PicoRun PID 调用 `GetProcessMemoryInfo`；私有提交为 `PrivateUsage`，工作集为 `WorkingSetSize`，提交峰值为 `PeakPagefileUsage`，工作集峰值为 `PeakWorkingSetSize`。MiB = 1,048,576 字节；表内为五进程阶段快照中位数（范围），不含探针和被打开的受控子进程。

| 阶段 | 私有提交 MiB | 工作集 MiB |
| --- | ---: | ---: |
| 隐藏启动 | 2.398（2.137–2.520） | 16.543（16.434–16.578） |
| 显示窗口 | 3.293（2.359–3.344） | 21.891（17.543–21.969） |
| 120 次查询后 | 3.305（2.359–3.355） | 22.238（17.578–22.305） |
| 手动刷新后 | 3.496（2.504–3.512） | 22.445（17.766–22.477） |
| 隐藏空闲五秒后 | 3.395（2.402–3.410） | 22.453（17.770–22.484） |

首个进程还运行完整的主题菜单、Shell 打开和输入法状态回归；全部记录中最大的阶段快照为私有提交 11.789 MiB、工作集 47.566 MiB。Windows 报告的进程生命周期提交峰值为 **46.812 MiB**、工作集峰值为 **86.926 MiB**，不能用隐藏阶段的低值代替。五次空闲区间各五秒，进程 CPU 计数增量均为 0 ms（受系统计时精度限制），没有调用工作集清空 API。该结果只适用于当前机器和上述受控流程。

## 真实窗口响应

每进程在首次显示后预热 20 次 `wx` 查询，再测 120 次：六种查询各 20 次，经跨进程原生 Edit 修改、同步查询消息和 `WM_PAINT`。每进程排序后 P50 为第 60 个、P95 为第 114 个值；五次进程分位值的中位数分别为 **3.0995 ms / 7.2114 ms**，范围分别为 2.8560–4.0898 ms / 5.4580–9.1779 ms。它包含原生消息往返与绘制，不代表物理按键到屏幕呈现延迟，也不使用搜索核心时间替代。

| 次序 | 应用缓存状态 | 启动至找到窗口 ms | SHOW + 同步绘制 ms | 输入 P50 / P95 ms |
| ---: | --- | ---: | ---: | ---: |
| 0 | 无缓存，扫描 | 300.1752 | 19.5053 | 2.8560 / 7.2114 |
| 1 | 损坏缓存，扫描恢复 | 272.9977 | 43.8984 | 4.0898 / 9.1779 |
| 2 | 有效缓存 | 278.2210 | 49.3466 | 4.0329 / 8.0468 |
| 3 | 有效缓存 | 268.8781 | 56.8613 | 3.0995 / 6.2543 |
| 4 | 有效缓存 | 279.0505 | 42.8189 | 3.0064 / 5.4580 |

启动计时包括创建进程与每五毫秒查询一次窗口的测试驱动开销；这些查询仅存在于验证程序。每次是新进程，系统文件缓存未清理，无应用缓存的启动仅测一次；不同缓存状态不合并成冷启动分位数。SHOW + 同步绘制五次的中位数为 43.8984 ms、范围 19.5053–56.8613 ms。原始 `checks.txt`、`memory.csv`、`timings.txt` 在 `runtime/review-fixes/controlled/`，解析的窗口统计在 `runtime/review-fixes/window-summary.json`。

## 可运行产物

主程序：`D:\Workspace\PicoRun\target\release\picorun.exe`，787,456 字节；Rust 1.95.0、MSVC x64，release 参数为 opt-level=3、fat LTO、codegen-units=1、panic=abort、strip、静态 CRT。该体积是磁盘大小，不是内存占用。

直接运行显示窗口，Alt+Space 呼出/隐藏；Ctrl+Q 或托盘退出。最终安装包与 ZIP 保持 0.1.0，仅生成本地分发文件，没有发布到 GitHub。

| 分发文件 | 字节 | SHA256 |
| --- | ---: | --- |
| `dist/PicoRun-0.1.0-windows-x64.zip` | 409,970 | `6d7080d36e05bd2c03affa84bd264a0c9449b24a733f48ced174f505ba4945de` |
| `dist/PicoRun-0.1.0-windows-x64-setup.exe` | 2,418,927 | `d37450bfaad983f6a660b8c6991fdf738afeee1a4289d04d996982df29397302` |

包内程序来自显式 `x86_64-pc-windows-msvc` 构建，同为 787,456 字节，SHA256 为 `122177b35cf82c355c618d43dd99c40bc617044ec257ba25bbac39c581cb880e`。完整构建元数据在 `dist/PicoRun-0.1.0-build.json`。

## v0.1.1 发版前验证

上述 0.1.0 表格保留审查修复阶段的原始产物记录。用户随后授权提交、推送与发版，因 v0.1.0 已发布，本次将 Cargo 清单及锁文件升级为 0.1.1；生产逻辑沿用上述七项修复。版本升级后重新执行 47 项 Rust 测试、格式检查、全部目标 Clippy 和离线 release 构建，全部通过。

发布使用仓库既有的手动 `Package Windows` workflow，检出匹配 Cargo 版本的 `v0.1.1` 标签，发布安装包、ZIP、校验和及构建元数据。Actions 构建与本地构建分开记录，公开构建的哈希以 Release 下载的 `SHA256SUMS.txt` 与 `build.json` 为准。[v0.1.1 发布说明](releases/v0.1.1.md)记录修复内容和验证边界。

本地 0.1.1 候选包单独输出至 `runtime/release-v0.1.1/`，通过 53 项隔离安装生命周期断言；真实 Run、用户设置与清单哈希不变。证据 `runtime/probe-packaging/ba1172755b984552961568d8798b8ae0/checks.txt`。包内 exe 为 787,456 字节，SHA256 `8f101e03f08a5cd413805ce9e491eb9224efb6732b43ced582e4d09258d3704f`；ZIP 为 409,976 字节，SHA256 `f8afba74881625632faae12c2f2f4bebb5bb6a05c656b8cdc973f0f8727ad592`；安装包为 2,418,986 字节，SHA256 `b630586c775181fe5e1990cb70bc7f6b8bc08ba80a6d1ea286b14997445d6c3b`。这些是本机候选产物校验值，公开 Release 产物由 Actions 独立构建。
