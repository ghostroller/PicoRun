# 商店应用发现与搜索修复

2026-10-09：安装版只扫描开始菜单 Programs 和桌面的 `.lnk` / `.exe`，没有读取 Windows AppsFolder；模型虽有 AUMID 类型，扫描、缓存和打开均未接通。原安装版三轮均找不到用户报告的 ChatGPT 与 Microsoft Store。修复版三轮真实窗口均找到两者，启动扫描和手动刷新均通过，总入口从 308 增至 330，其中 22 项为打包应用。

## 实现与边界

- 默认来源在启动/F5 时用 `SHCreateItemFromParsingName`、`BHID_EnumItems` 和 `IEnumShellItems` 枚举 AppsFolder，不执行 PowerShell/Python，不扫描包安装目录。显式 `--source` 仍只读取指定文件夹。
- 用 `IShellItem2::GetProperty(PKEY_AppUserModel_ID)` 读取 AUMID，再以 `GetPackagesByPackageFamily` 的当前用户注册查询确认打包身份，避免重复引入普通 Win32 应用。采用 Shell 的本地化显示名，按 AUMID 去重；同名不同身份保留。未按名称合并普通快捷方式与打包入口，避免丢失自定义快捷方式启动语义。
- 只有成功读取的 `VT_EMPTY` 视作无属性；HRESULT、类型、名称、枚举或预算错误标记整个打包来源失败并回补旧打包入口。成功刷新可移除卸载项，文件夹失败仍只回补所属路径。状态栏改为报告失败“来源”。
- COM 引用、任务字符串与 PROPVARIANT 使用所有权清理；全部系统调用在 Runtime/控制器可变借用之外进行。记录最多 100000 项、临时记录容量预算 16 MiB；Shell 枚举对象在生成拼音前释放。正常搜索仍只消费预计算键，没有新增常驻线程、轮询或图标资产缓存。
- 打包应用通过 `IApplicationActivationManager::ActivateApplication` 激活，并在现有状态栏报告 HRESULT；普通 `.lnk` 继续原入口 Shell 打开。开启图标时打包应用目前使用通用图标。
- 缓存头升级为 v2，记录增加目标类型标签；继续读取旧 v1。为保留既有路径及验证工具，文件名仍为 `apps-v1.bin`，其内部版本决定格式。AUMID 按 SDK 上限最多 129 个 UTF-16 单元（不含 NUL），拒绝空值、NUL、非法编码及未知标签。没有新增 Rust 依赖。

接口与 ABI 依据为本机 Windows SDK 10.0.26100.0 头文件及微软文档：[AppsFolder/AUMID](https://learn.microsoft.com/en-us/windows/configuration/store/find-aumid)、[Shell 项创建](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-shcreateitemfromparsingname)、[枚举所有权](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ienumshellitems-next)、[当前用户包注册](https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-getpackagesbypackagefamily)、[应用激活](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-iapplicationactivationmanager-activateapplication)。

## 正确性与真实窗口

`cargo fmt --check`、`cargo test --offline`（54 单元 + 5 集成）、`cargo clippy --offline --all-targets -- -D warnings`、`cargo build --release --offline` 全部通过。回归覆盖缓存 v1 迁移/v2 混合入口、损坏与非法标签、PROPVARIANT 布局和错误分类、严格 UTF-16 边界、当前用户注册过滤、AUMID 去重、中文/全拼/首字母键、同名稳定顺序及失败来源回补。既有文件夹测试继续检验隔离扫描和目录失败恢复。

新增 `native_probe.exe --packaged`。使用独立数据目录，实际运行 release 主程序；给原生 Edit 设置查询，通过同步刷新、GDI 绘制和仅测量模式可用的标量目标类型/身份哈希核对所选入口。两项查询均有实际窗口截图；每轮手动刷新、缓存重新读取、再次查询及正常退出码 0 均通过。另用不存在的 AUMID 检验实际激活 COM 返回失败。没有打开 ChatGPT、Microsoft Store 或其他个人应用，故本轮没有验证成功的打包应用启动；快捷方式受控程序的实际参数/工作目录验证见下述记录。

窗口响应是跨进程 `WM_SETTEXT → 查询同步 → WM_PAINT` 的发送调用总时间，不包含真实键盘调度、显示扫描输出或完整 IME 会话；不能等同用户按键到屏幕延迟。测量开关保持窗口可见以避免失焦隐藏影响截图，不改变文本、选择、搜索和绘制逻辑。

既有 `native_probe.exe --dedup` 的 15 项断言全部写入，包括原 `.lnk` 启动受控程序并核对 Unicode 参数和工作目录，但探针随后在本机 `windows.storage.dll` 10.0.19041.2788 发生 `0xC0000005`、偏移 `0x46668C` 原生异常。本轮不能将该探针完整退出记为通过。旧安装版与 Explorer 也有相同 DLL/偏移的事件；无转储调用栈，不能确认原因，未为此修改生产清理或跳过 COM 释放。最终两组商店探针共六个主程序均正常退出。

早期测试使用的 Ctrl+Alt+F11 在本机冲突（Windows 1409），其提前退出后的内存/响应数据无效，不计入下表；最终改用 Ctrl+Alt+Shift+F9，并验证进程存活与退出码。

## 完整进程测量

本轮实际机器：Intel Core i9-11900K、16 GiB、Windows 10 专业版 22H2 / 10.0.19045、x64。此前文档中的 Windows 11 / 32 GiB 属于另一轮环境。Rust 1.95.0，`x86_64-pc-windows-msvc`；release 为 opt-level=3、fat LTO、单 codegen unit、panic=abort、strip=true。以下性能来自修复完成时的 0.1.1 未发布工作副本；随后只将包版本升为 0.1.2，性能代码没有再改动。Windows 10 本轮功能通过不代表所有 Windows 10 版本、低配/换页机器、32 位或 ARM64 已验收。

安装版对照与修复版各 3 个独立进程，先对照后修复；首次无磁盘索引缓存，其余两次有缓存，均为新进程且仍重新扫描。没有清空操作系统文件缓存或工作集，不能称物理冷启动。图标关闭、深色、英文输入开关打开，每轮查询预热 100 次后计时 100 次（5 个固定查询循环 20 次），随后一次 F5 与 2 秒隐藏空闲。完整进程计数来自 `GetProcessMemoryInfo` 和 `GetProcessTimes`，包含加载的 Shell/UI/输入组件；不含外部应用或测量工具。

| 指标 | 安装版（308 项） | 修复版（330 项） |
| --- | ---: | ---: |
| 首次无索引缓存启动 ms | 188.146 | 290.172 |
| 两次有缓存的新进程启动 ms | 175.747 / 168.795 | 270.035 / 285.458 |
| 三次启动 P50 / P95 ms | 175.747 / 188.146 | 285.458 / 290.172 |
| 三轮查询各自 P50 的中位数 ms | 1.670 | 1.660 |
| 三轮查询各自 P95 的中位数 ms | 3.063 | 3.154 |
| 三次 F5 P50 / P95 ms | 137.126 / 138.917 | 235.615 / 241.567 |
| 最大累计私有提交峰值 MiB | 4.961 | 6.680 |
| 最大累计工作集峰值 MiB | 19.891 | 26.750 |
| 各轮 2 秒隐藏空闲 CPU 时间增量 ms | 0 / 0 / 0 | 0 / 0 / 0 |

内存表为各阶段三个进程样本的 P50 / P95，单位 MiB；P50 取排序中位项，P95 用 nearest-rank（n=3 时为最大项）。CPU 受本机计数粒度限制，零增量不代表任意时段绝对零 CPU。时间中三轮查询分位数是各轮分位数的中位数，不是合并 300 个样本的分位数。

| 阶段 | 安装版私有提交 | 修复版私有提交 | 安装版工作集 | 修复版工作集 |
| --- | ---: | ---: | ---: | ---: |
| 初始隐藏 | 3.938 / 3.988 | 4.965 / 5.102 | 17.234 / 17.250 | 23.035 / 23.105 |
| 显示 | 3.977 / 4.043 | 5.402 / 5.508 | 19.246 / 19.289 | 25.379 / 25.426 |
| 连续查询后 | 3.977 / 4.043 | 5.293 / 5.402 | 19.441 / 19.477 | 25.387 / 25.453 |
| F5 后 | 4.160 / 4.449 | 5.312 / 5.512 | 19.660 / 19.879 | 25.383 / 25.645 |
| 隐藏空闲结束 | 4.160 / 4.449 | 5.312 / 5.512 | 19.668 / 19.887 | 25.387 / 25.648 |

此批隐藏私有提交中位数增加约 1.03 MiB、工作集增加约 5.80 MiB，扫描/F5 也有额外成本。包含新增来源与 Shell 加载成本，不能按 22 个入口简单推算单项开销。样本小、顺序未随机化，原安装版缺少两项查询结果，不能据响应中位数接近宣称所有负载无回归。对照第一轮与合成核心采样短暂并行；其启动数值保留为观察，预热后的中位查询与后两轮/修复轮才用于比较。未达到完整低内存机器的产品验收。

## 纯搜索与索引生成

最终 release `search_bench.exe` 的独立记录，固定一个可用 CPU 核，12 个查询，预热 20 轮（240 次）后重复 100 轮、每个规模 1200 样本。每个规模构建一次，首搜索是该索引第一次调用，并非新进程/磁盘冷启动。此项排除 AppsFolder、Shell、原生窗口、应用启动及完整进程内存。

| 合成入口数 | 构建 ms | 自有堆容量 bytes | 首搜索 μs | 预热 P50 / P95 ms |
| ---: | ---: | ---: | ---: | ---: |
| 500 | 0.5331 | 121074 | 22.6 | 0.0213 / 0.0283 |
| 2000 | 2.2606 | 488874 | 92.9 | 0.0848 / 0.1158 |
| 10000 | 9.2487 | 2461674 | 467.9 | 0.4299 / 0.5492 |

## 交付与原始证据

测量副本：`target/release/picorun.exe`，801280 bytes，0.1.1 未发布工作副本，SHA-256 为 `908296AF450956D7B3C9A81C882D134B15C3D1E62F65EEFA975AECBC75340402`。本次发布版本升为 0.1.2，以 tag 对应的 release 构建与校验和为准。退出安装版后运行修复版 EXE，默认启动扫描即可找到两项应用，F5 可刷新；托盘“退出 PicoRun”或 Ctrl+Q 正常退出。测试结束已恢复原安装版，安装包由 GitHub 手动工作流生成。

以下均在忽略目录，不提交个人应用名称清单、AUMID 清单或真实入口路径：

- `runtime/probe-packaged/`、`runtime/probe-packaged-reference/`：三轮断言、计时、完整进程 CSV、隔离缓存与两项截图。
- `runtime/packaged-summary.json`、`runtime/search-bench-final.txt`：无个人入口的汇总及合成记录。
- `runtime/dedup/checks-5976/`：15 项受控快捷方式断言；原生退出异常不能忽略。
- `runtime/probe-packaged/data/host-error.txt`：早期测试热键冲突诊断；最终备用热键结果以上述 checks 为准。

复验前正常退出现有 PicoRun，先执行规定的离线构建检查，再运行 `target/release/native_probe.exe --packaged`。对照命令为 `--packaged --reference`，须先在 `runtime/packaged-baseline/picorun.exe` 保存原安装版只读副本。探针专门验证本机报告的两项名称，不是任意语言/任意机器的通用应用存在性断言。
