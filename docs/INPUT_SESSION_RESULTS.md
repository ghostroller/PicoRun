# 输入模式恢复与扫描规则

2026-10-04：修正“呼出时使用英文输入”的会话边界。开关仍在原托盘菜单中，默认关闭、独立保存，不新增设置窗口、线程、依赖或周期轮询。

## 扫描范围和重复入口

默认按 Windows Known Folder 读取当前用户 Programs、公共 Programs、当前用户桌面、公共桌面，尊重目录重定向。启动和 F5 递归发现，最多 16 层，跳过符号链接及重解析点；接受 `.exe` 或目标扩展名为 `.exe` 的 `.lnk`，不纳入文档、文件夹、网页快捷方式或 Store/UWP。名称取入口文件名；仅按入口路径忽略大小写去重。

本机只读核对截图中两组重复：AB Download Manager 是用户开始菜单与用户桌面的两个快捷方式；AdsPower Browser 是公共开始菜单与公共桌面的两个快捷方式。每组的目标、参数、工作目录和窗口显示方式相同。这里只记录来源类别，不提交个人目标路径或完整清单。

当前规则保留这两种来源的入口。后续若按启动语义去重，需要同时比较目标、参数、工作目录等信息；不能只按名称或 `.exe` 目标合并，以免删除不同启动方式。启动仍交给 Shell 执行原 `.lnk`。

## 输入会话

呼出前通过前台线程与实际焦点控件保存布局和可读取的输入法开关、转换状态；无法读取外部模式时保留可取得的布局。重复呼出不覆盖第一次备份。在原生 Edit 取得焦点后，优先保留原输入法布局并临时关闭组合输入，形成直接英文查询；输入法拒绝关闭时再尝试已加载的英文布局，不安装或卸载布局。

Esc、热键隐藏、失焦、Enter 打开、关闭选项及正常退出走同一恢复入口。关闭窗口之前恢复，避免子控件销毁后失去上下文。恢复时的焦点通知可能重入窗口过程，独立 Cell 保存状态和恢复标记，恢复期间不会再次建立英文会话，没有 Runtime 可变借用跨 Win32 调用。窗口内可手动改变输入模式，下次呼出仍优先英文。

原窗口恢复请求仅发往保存并再次核对进程/线程的控件及其默认 IME；布局有变化才投递恢复请求。外部 IME 状态的读取与恢复使用有 50 ms 超时的同步标量消息，避免阻塞在其他应用上。Windows 拒绝跨进程异步投递 `WM_IME_CONTROL`，试验中这曾导致恢复失败并保留第一次备份，已改正。读取失败、不支持的 IME、进程退出或权限限制不伪造成功；发送失败会给出状态提示。

恢复组合转换状态之后再设置中英文开关。原状态为英文时，不强行恢复可能使 IME 再次开启中文的组合转换标志；优先保证实际输入仍为英文。原生输入法可能重设辅助转换或句子标志，不宣称所有内部标志逐位恢复。

此前直接切英文 HKL 的试验中，原控件重新获得焦点后可能又选回英文，因此改用临时英文模式。探索失败、辅助标志变化及前台被 Chrome 打断的记录保留在忽略目录；中断时立即停止按键注入，不把中断记成通过。

所用接口及平台行为依据：[ImmSetOpenStatus](https://learn.microsoft.com/en-us/windows/win32/api/imm/nf-imm-immsetopenstatus)、[IME 消息转发](https://learn.microsoft.com/en-us/windows/win32/intl/ime-messages)、[前台线程信息](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getguithreadinfo)、[转换模式与 Windows 8 起的输入切换说明](https://learn.microsoft.com/en-us/windows/win32/api/imm/nf-imm-immsetconversionstatus)、[异步消息与参数限制](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew)。

## 本轮验证边界

`native_probe --input-session` 创建两个真实原生进程，一个代表原应用输入框，一个是 PicoRun；使用独立数据目录及一个合成快捷方式。快捷方式仅打开受控程序。分别以中文和同一输入法内的英文模式开始，真正键入 `weixin`，检查显示、重复呼出、Esc、热键、失焦、Enter、关闭选项和退出；返回原输入框后以真实按键及 Space 确认中文 `微信` / 英文 `weixin `。

正式 release 构建实测 82 项通过、0 项失败，另记录 4 次英文关闭状态下转换标志从 1 变为 0；实际键入英文及原布局、中英文开关均正确。证据为忽略目录 `runtime/probe-input-session/checks.txt` 与 `runtime/input-session/session-18-stdout.txt`。临时诊断日志已从正式代码移除。

随后在独立目录运行 `native_probe --ime-real`，完整原生回归 78 项通过、0 项失败，包含 500 个合成快捷方式、托盘/主题、英文开关与跨进程保存、真实英文键入、手动切换、真实中文组合与候选按键、Shell 参数与工作目录、刷新、单实例、热键冲突及退出释放。证据为忽略目录 `runtime/input-session/full-suite/runtime/probe-controlled/checks.txt`；没有打开用户应用。五个原生进程各采样 5 秒隐藏空闲 CPU，均为 0 ms；这是该测试区间的计数，不能据此承诺所有环境。

`cargo test --offline` 的 20 项单元测试与 5 项搜索流程集成测试通过；`cargo fmt --check`、`cargo clippy --offline --all-targets -- -D warnings`、`cargo build --release --offline` 均通过。现有注册表单元测试在允许访问隔离 HKCU 测试键的进程内执行，不注册真实登录启动项。

正式 `target/release/picorun.exe` 为 734,720 字节，SHA256 `2D624B573AC5AB8C9AFA0952D033715D4AB5EBC29B9A78AE06868356CA937E6E`。构建为 Rust 1.95.0 MSVC x64，静态 CRT、opt-level=3、fat LTO、codegen-units=1、panic=abort、strip=true。

受控测试进程均已退出，随后用原数据目录重新打开正式应用并核对窗口属于新版进程、窗口可见；保留用户已保存的选项，启动证据为忽略目录 `runtime/input-session/final-startup.json`。

## 完整进程占用观察

机器为 i7-11700K、约 32 GiB RAM、Windows 11 企业版 10.0.26200 x64。仅一个新 PicoRun 进程、一个合成应用入口、暗色、图标关闭；初始样本在隐藏状态采集，后续样本在完成两种模式共 10 次会话及两次受控 Shell 启动后采集。每阶段 1 次采样、正式构建 1 次完整运行；没有预热循环，系统文件缓存已热，不是重启系统后的磁盘冷启动。取 `GetProcessMemoryInfo` 的完整进程数值，峰值为从该进程启动累计的 Windows 计数。

| 阶段 | 私有提交 MiB | 工作集 MiB | 峰值提交 MiB | 峰值工作集 MiB |
| --- | ---: | ---: | ---: | ---: |
| 初始隐藏 | 1.887 | 15.832 | 1.887 | 15.832 |
| 10 次会话后 | 10.676 | 45.895 | 45.309 | 85.551 |

原始数值见忽略目录 `runtime/probe-input-session/memory.csv`。本轮是功能验收中的占用观察，没有窗口延迟或搜索核心的 P50/P95，也没有同场旧版对照；不能把占用增加全部归因于恢复代码，路径同时加载了 IME 与 Shell。保留原输入法布局会使用其运行时，旧“直接切英文布局”的低占用结论不直接适用。真实安装应用数量、其他输入法和低配机器的完整进程成本仍需独立评测。

仅覆盖本机 Windows 11 x64 与小狼毫/Rime；Windows 10、其他输入法、管理员应用和极低配实机仍未验证。本轮未改搜索算法，不把旧搜索核心或旧英文 HKL 的测量当成新版本窗口/内存承诺。
