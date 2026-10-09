# 终端关闭与应用启动生命周期

2026-10-09：用户在终端 cargo run --bin picorun 后打开桌面应用，强制关闭终端时应用也结束。本轮检查发现，原 Shell 打开方式在调用者属于 Windows Job 时可能让新进程继承同一 Job；PicoRun 没有主动枚举或结束用户应用。

## 复现及原因

使用受控 GUI 程序和自有 Job 验证，未打开 dbx、Edge 或其他个人应用。Job 配置 KILL_ON_JOB_CLOSE，禁止 breakaway；先暂停创建原版 PicoRun、加入该 Job，再恢复运行，通过它的原生 Edit 打开受控 .lnk／EXE：

- 两种目标均属于自有 Job，目标控制台窗口为 0。
- 关闭最后一个 Job 句柄，PicoRun 和目标都结束。
- 只正常关闭 PicoRun、保留 Job 句柄，目标继续存活。

这证明了与用户描述一致的 Job 继承问题，且本次受控复现不依赖控制台关闭事件。没有读取已关闭的 dbx／Edge 的 Job，不能声称确认那几个已退出进程的实际成员身份。Windows Terminal 的控制台关闭和进程组管理是相关的宿主行为，普通父进程退出本身不会自动结束所有子进程。

微软 [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects) 说明默认子进程继承 Job；[Process Creation Flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags) 中 CREATE_BREAKAWAY_FROM_JOB 需要 Job 允许脱离。CREATE_NO_WINDOW／DETACHED_PROCESS 只处理控制台，不能保证脱离 Job。

## 实现

普通无 Job、无控制台的桌面启动继续使用原 Shell／AUMID 路径。处于 Job 或附着控制台时，仅在打开应用那一刻创建短命自身 helper：

1. 从 GetShellWindow 取得当前桌面 Shell，要求同用户 SID、同会话、完整性级别不高于调用者，且 Shell 不属于 Job。只请求 PROCESS_CREATE_PROCESS／QUERY_LIMITED_INFORMATION 和 TOKEN_QUERY，不启用 SeDebug，不修改终端 Job、Shell token 或其他进程内存。
2. 用 PROC_THREAD_ATTRIBUTE_PARENT_PROCESS 创建可信自身 helper。精确指定自身 EXE、固定私有参数模式、正确引用 UTF-16 参数，不用 cmd 或 PowerShell 拼接命令，不搜索 helper EXE。
3. 显式传调用者当前 UTF-16 环境，保留隐式驱动器变量和原始单位；注册 Path 只加到这个私有环境的 PATH 前面。父进程和 Explorer 的环境不变。相对入口不 canonicalize，helper 的 current directory 按 CreateProcess 默认继承调用者，原 .lnk 的参数／工作目录仍由 Shell 处理。
4. 不继承终端／父管道句柄，不给 helper 创建控制台；best-effort 转交前台权限。helper 自身在 Shell／ActivationManager 之前检查不属于任何 Job；错误通过退出码回传，主进程只等待 helper，不等待应用整个生命周期。
5. 不暂停创建 helper：避免终端在创建／恢复的间隙死亡而留下 Job 外永久暂停的孤儿。父进程仍核验 helper 的 Job／身份；快速已退出的 helper 用可信 typed 退出结果处理查询竞态。所有属性列表、环境、token、进程、线程句柄按所有权释放，错误只回收自身 helper。
6. 文件入口通过原 ShellExecuteEx 打开；打包入口仍由相同 IApplicationActivationManager 激活 AUMID，隔离模式只改变调用所在进程。Shell 和打包激活错误分别保留 Win32／HRESULT 类型。

独立启动不可用时返回现有错误状态，未静默退回仍被终端约束的启动。没有常驻 broker、后台轮询、新依赖、安装项或登录启动注册。PicoRun 本身仍是终端的子进程，本次保护的是应用启动生命周期，没有改变 PicoRun 自身的启动策略。

父进程属性会继承指定进程的 token、Job 和设备映射，依据 [UpdateProcThreadAttribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)。显式环境、current directory 和不继承句柄的合同见 [CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)。因此隔离 helper 可能按桌面 Shell 的较低权限／设备映射运行，并非保证保留管理员调用者的所有隐式权限或映射驱动器。真正 UAC 提升、不同用户、远程会话、Shell 替换和映射网络盘没有实际验收；原入口交给 Shell 的规则保持，没有覆盖 .lnk 的显式启动设置。

## 功能与构建验证

cargo fmt --check、cargo test --offline（100 单元＋7 集成）、cargo clippy --offline --all-targets -- -D warnings、cargo build --release --offline、cargo build --offline --bin picorun、git diff --check 均通过。新增九项测试覆盖 Windows 实际 CommandLineToArgvW、中文／空白／引号／尾反斜杠／原始 UTF-16、相对入口和 AUMID、环境排序／隐藏驱动器／Path 去重／长度、SDK ABI、token／属性所有权及 typed 错误。

release 24 个成功生命周期例：.lnk、EXE、相对目录 .lnk、带 Path 的 App Paths，各正常退出／强制关闭 Job ×3。debug 同一程序构建再测 8 例，各场景一轮。所有新目标均在自有 Job 外，关闭 Job 后继续存活至少 1100 ms；正常退出 PicoRun 后再关 Job 仍存活。没有 helper 残留。原 .lnk 的 Unicode 参数、空格和工作目录准确；相对入口身份不改写；App Paths 私有 Path 正确，随后同 EXE 中性注册入口证明无父环境污染。EXE／App Paths 记录的默认工作目录与调用者一致。

release／debug 各两项错误：缓存入口被删除、缓存注册目标被替换成无效 PE。没有启动目标或留下 helper，主程序能正常退出。

debug 用 cargo build 的实际 debug PE 副本进入等效、禁止 breakaway 的 Job，没有额外运行 literal cargo 父进程；这是直接检验 cargo run 所用构建与 Job 条件，不能说实际关闭了用户终端。

另用未安装的随机包家族、valid v3 fallback 缓存测试 AUMID 失败桥接，未激活任何真实 Store 应用。原生 Edit 选中 kind2／正确 hash，helper 的父进程为 Job 外桌面 Shell，Enter 同步返回约 61.290 ms，UI 显示 HRESULT 0x80073CF1，主程序响应、无 helper 残留并正常退出。WMI stop 事件没有捕捉，采用实际错误截图／状态 hash／start 事件而不是编造退出事件；首次 observer 中断不计成功。真实 Store 成功激活及它的整个进程树生命周期仍未测试。

已有进程的 Job 关联不能事后移除；如果浏览器把请求交给已经属于旧终端 Job 的实例，本轮不能替它脱离。复验时应正常退出旧实例后测试新创建的进程。应用自己实现的父进程监控或单实例委托，也不是通用 Shell 代码能改写的生命周期。

## 机器、构建与成本

Windows 10 Pro 22H2／19045 x64，i9-11900K 8 核／16 逻辑处理器，物理内存 16933777408 bytes；Rust 1.95.0、MSVC x64。release opt-level=3、lto=fat、codegen-units=1、panic=abort、strip=true；debug 为默认 dev profile。未验收低配、换页、32 位、ARM64 或所有 Windows 10／11 系统配置。

冻结原版 SHA-256 E1E714AEE288F283E3FB685C96AA38327F23CA7C2EF318D57FB433C8DCB59265，921600 bytes。最终 release SHA-256 B628B32E70B9A1A17651CD75F3567FC70BD354C55B774AC614BC2439C4D0F3F9，938496 bytes；debug SHA-256 DB4233D94079FE3466F3CF7414FE49EA58C634E238D91AC3BBFAC0615F778C23，1848832 bytes。无新分发包、安装替换或发布。

500／2000／10000 合成目录沿用来源验证样本（1% ClickOnce，其他 .lnk），新旧版本均完整索引所有项，别名查询负载一致。每版本／规模只一个新进程，共六个，独立无磁盘索引；图标关闭、英文输入开启。固定旧→新，未清 OS 文件缓存或工作集，每进程 100 预热＋100 查询计时。查询包含原生 Edit WM_SETTEXT、同步查询和绘制，不含键盘、完整 IME 或屏幕呈现。

Startup／F5 只有单次观察，不能报告可信分布或从顺序差异推断加速；查询 P50／P95 来自该进程 100 样本。内存为完整主程序 GetProcessMemoryInfo，MiB；峰值是有限阶段读取的生命周期高水位，未覆盖最后采样之后的退出分配：

| 项数 | 版本 | Startup／F5 ms | 查询 P50／P95 ms | 隐藏 私有／WS | idle 私有／WS | 累计峰值 私有／WS |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 500 | 旧 | 1186.752／612.875 | 3.523／6.611 | 2.688／14.309 | 2.910／15.395 | 3.492／15.762 |
| 500 | 新 | 471.669／280.777 | 2.967／6.686 | 2.734／14.250 | 2.980／16.824 | 3.758／17.180 |
| 2000 | 旧 | 2841.638／1412.943 | 4.604／10.309 | 3.590／15.168 | 3.852／17.664 | 6.016／19.305 |
| 2000 | 新 | 1343.289／1250.843 | 5.056／9.681 | 3.563／15.148 | 3.785／17.578 | 6.109／19.320 |
| 10000 | 旧 | 15131.180／6344.698 | 3.624／5.922 | 7.074／18.223 | 8.934／20.270 | 16.840／26.379 |
| 10000 | 新 | 6631.493／6727.170 | 5.566／10.706 | 7.098／18.230 | 8.816／20.359 | 16.914／26.527 |

六次两秒隐藏 idle CPU 增量为 0 计量单位，非绝对零 CPU；初始 GDI／USER 8／10，idle 15／13 或 17／15。主进程内存差异较小，单轮不能证明相等。本轮查询数字更慢／波动明显，未声称 UI 无回归。没有修改搜索路径；普通启动不创建隔离 helper。

另外三次受控启动记录完整主进程有限阶段内存（MiB）。每类型一次，无预热分布，不能直接比较不同来源的总占用：

| 类型 | 索引数 | 打开前 私有／WS | marker 后 私有／WS | idle 私有／WS | 主进程累计峰值 私有／WS |
| --- | ---: | ---: | ---: | ---: | ---: |
| .lnk | 2 | 2.656／15.039 | 2.660／15.270 | 2.660／15.270 | 2.891／15.273 |
| EXE | 2 | 2.637／14.867 | 2.637／15.082 | 2.637／15.082 | 3.387／15.086 |
| App Paths | 372 | 5.328／24.238 | 5.344／24.430 | 5.316／24.438 | 6.078／24.441 |

目标在正常退出／关闭 Job 后继续存活，无 helper 残留。短时 helper 的峰值和整个进程树的并发峰值未捕捉，不是零；JSON 明确记录 false／null。一次性 helper 只在执行启动时存在，未把它的短时成本当作主程序常驻成本，也没有宣称解决所有启动峰值。

单独复跑 release search_bench，固定一个可用核；每规模一次构建、12 查询×20 轮＝240 预热，随后 100 轮＝1200 样本。500／2000／10000 的纯核心 P50／P95 为 0.0223／0.0391、0.0931／0.1574、0.4396／0.5580 ms。合成 AppEntry 不含附加别名生成、发现、UI、激活、图标或完整 launcher 内存；不能替代上面真实窗口或完整进程数字。

## 复现证据

实际程序为 D:\workspace\PicoRun\target\release\picorun.exe，也可以退出旧 PicoRun 后重新 cargo run --bin picorun 使用修复后的 debug 构建。只有新启动受本轮保护，旧实例的 Job 不会改变。

私人路径和原始环境只保存在已忽略 runtime：

- runtime/job-tree/baseline-picorun.exe、baseline-results.json、environment.json：原版四例和机器 Job／Shell 条件。
- runtime/job-tree/job-driver.ps1、ControlledJob.cs、controlled-child.rs：受控原版复现。
- runtime/job-tree/job-fixed-driver.ps1、controlled-child-fixed.rs：release 24／debug 8 生命周期例和各两错误例；两个 fixed-* 目录保存独立结果／哈希／marker。
- runtime/job-tree/aumid-negative-driver.ps1、AumidFixture.cs、aumid-negative-*：不存在包的 typed 错误及 observer 限制，未启动真实应用。
- runtime/job-tree/job-launch-memory-driver.ps1、launch-memory-*：三次主进程启动阶段内存。
- runtime/discovery-scale/measure-job-scale.ps1、summarize-job-scale.ps1、job-comparison/：六进程原始计时／内存、机器／哈希／配置、single-run 汇总／口径和独立搜索结果。
- runtime/job-tree/fixed-tests.txt：100 单元＋7 集成输出。

生产边界为 [launch.rs](../src/platform/windows/launch.rs)、[windows.rs](../src/platform/windows.rs)、[main.rs](../src/main.rs)、[discovery.rs](../src/platform/windows/discovery.rs) 与原 App Paths／packaged 模块。未选择发布许可证或添加第三方运行时。
