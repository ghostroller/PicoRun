# 前期证据与适用范围

2026-10-09：终端强制关闭会通过继承的 Windows Job 连带结束应用，受控原版／修复版验证与 Windows SDK 父进程属性、环境及安全边界见 [LAUNCH_LIFETIME_RESULTS.md](LAUNCH_LIFETIME_RESULTS.md)。选择仅在需要时创建独立 helper，保留普通桌面启动的原路径；没有靠隐藏控制台或修改终端 Job 处理生命周期。

2026-10-09：Flow Launcher Program 插件的默认与可选来源核对见 [FLOW_DISCOVERY_AUDIT.md](FLOW_DISCOVERY_AUDIT.md)，固定官方提交；该调查阶段未扩展扫描，用户随后授权的实施范围如下。

## 2026-10-09：来源补齐与有界名称别名

用户随后批准实施 [FLOW_DISCOVERY_AUDIT.md](FLOW_DISCOVERY_AUDIT.md) 的前三项建议。PicoRun 独立补充默认 StartMenu 根目录递归、HKCU／HKLM 双注册表视图的 App Paths、ClickOnce `.appref-ms`（包括指向该入口的 `.lnk`），并用 Shell 本地化名、原快捷方式 stem、目标 EXE stem 建立有界搜索名称。没有复制 Flow 的扫描或排名代码，不声称应用清单或排序与 Flow 等价；打包应用仍由原 AppsFolder／AUMID 路径发现，尚未按 AUMID 做两者的完整差分。

开始菜单仍由 Known Folder 处理重定向，Startup 也由系统已知目录定位后排除，不依赖英文目录名。App Paths 原生读取分别覆盖 HKCU／HKLM 的 32／64 位视图；仅接受合法、现存、绝对 `.exe` 映射，按注册值类型展开 `REG_EXPAND_SZ`，保留可选 `Path`。只合并键名、目标和环境都相同的注册映射，不因 EXE 相同就合并不同环境或原快捷方式；App Paths `Path` 的启动处理使用一次性无窗 PicoRun helper 的私有环境和 Shell 精确路径，主进程 PATH 保持原样。helper 只在用户请求打开相应注册项时运行，不是索引脚本或常驻服务。

别名仅在索引时生成归一化名、全拼及首字母，最多两个附加名称／总计九个键，附加键每条不超过 512 UTF-8 字节，附加 owned 容量不超过 2 KiB；超限整组跳过而不截断字符。规范键组可恢复名称数量，因此没有原始别名或计数常驻字段；添加别名及保存时重算布局的开销应纳入索引／缓存计量，搜索不重新转换候选拼音。共享目标池保持 512 项／128 KiB，上限以外的 `.lnk` 目标在最后索引阶段按需重读。

索引仍为启动／F5 更新，无 watcher、轮询或新依赖。`--source` 保持替代默认来源的既有语义，也排除默认 App Paths 和打包来源；本轮未加入 PATH 目录或游戏协议扫描。`apps-v1.bin` 文件名不变，当前写 v3 并读取 v1／v2／v3；由当前保存器生成的 v3 目标与别名可往返，外部构造的非规范键组不承诺再次保存成功。

单元测试已覆盖注册映射、别名、旧缓存迁移和有界输入；这些不代替真实应用启动、窗口响应或完整进程内存验收。混合路径曾导致本机 `windows.storage.dll` 退出异常，冻结旧版亦复现。仅将文件系统 Shell 传参统一为本机分隔符后，混合／标准路径各三轮正常退出；原默认／自定义来源探针也完整通过。保存身份和 COM 收尾仍保留，原失败证据不计为通过。受控验证、500／2000／10000 项样本、完整进程私有提交／工作集／峰值和系统边界见 [DISCOVERY_SOURCES_RESULTS.md](DISCOVERY_SOURCES_RESULTS.md)，不将本轮实现等同于 Windows 10、低配实机或 32 位／ARM64 的完整验证。

## 2026-10-04：其他启动器的应用去重

实施前只读核对下列固定提交的源代码和现有测试，未构建、运行其他启动器或进行同机性能比较。Flow 为 dev，其他为各项目默认分支快照，不将源码快照等同于所有发布版本。这里只借鉴原则，不复制外部实现；随后用户批准的 PicoRun 实现与实测见 [DEDUP_RESULTS.md](DEDUP_RESULTS.md)。

| 项目与提交 | 已核对的去重规则 | 对本项目的意义 |
| --- | --- | --- |
| Flow Launcher `5da5103df70632485a72ce0dec81d5f4df9faa57` | 自动索引来源按实际目标与参数拼接、整体转小写后分组，优先开始菜单快捷方式，其次带描述的项；最终另按入口 UID 去重。用户自定义来源没有全部经过该启动分组。 | 有参数意识及入口优先级；键不包含工作目录、窗口模式或运行权限，参数也被转为小写，不宜直接采用相同规则。[ProgramsHasher / All](https://github.com/Flow-Launcher/Flow.Launcher/blob/5da5103df70632485a72ce0dec81d5f4df9faa57/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L644) |
| PowerToys Run / Command Palette `1400fd8e999f381329e16e9df4084f7dc588c8a7` | 用 HashSet 比较名称、ExecutableName、FullPath，忽略大小写；.lnk 的 FullPath 会更新为目标路径，原快捷方式另存。比较键不包含参数、工作目录、窗口模式或权限。 | 同名同目标的桌面/开始菜单副本能合并；按源码推断，同名但参数不同的项也可能被合并。现有命令提示符测试保留多个入口依赖名称不同，不能据此证明保留所有参数差异。[Run 比较器](https://github.com/microsoft/PowerToys/blob/1400fd8e999f381329e16e9df4084f7dc588c8a7/src/modules/launcher/Plugins/Microsoft.Plugin.Program/Programs/Win32Program.cs#L889)、[Command Palette](https://github.com/microsoft/PowerToys/blob/1400fd8e999f381329e16e9df4084f7dc588c8a7/src/modules/cmdpal/ext/Microsoft.CmdPal.Ext.Apps/Programs/Win32Program.cs#L836)、[命令提示符测试](https://github.com/microsoft/PowerToys/blob/1400fd8e999f381329e16e9df4084f7dc588c8a7/src/modules/launcher/Plugins/Microsoft.Plugin.Program.UnitTests/Programs/Win32Tests.cs#L325) |
| Wox Windows `20aeef60860219044a4f3cec83bdbd817a467189` | 启动键包含目标、动词、原始参数、工作目录、Show；参数大小写和空工作目录保留。用结构化编码避免字段拼接歧义，不支持的快捷方式无启动键，只按入口路径合并。排序确定代表项，用户开始菜单优先公共开始菜单、桌面等来源；其他名称进入搜索别名。 | 最接近本项目的启动语义约束。有参数/权限变体、名称别名、顺序稳定性和备用来源晋升测试。[启动键](https://github.com/Wox-launcher/Wox/blob/20aeef60860219044a4f3cec83bdbd817a467189/wox.core/util/shell/shortcut_windows.go#L25)、[合并与优先级](https://github.com/Wox-launcher/Wox/blob/20aeef60860219044a4f3cec83bdbd817a467189/wox.core/plugin/system/app/app_dedup_windows.go#L61)、[变体和别名测试](https://github.com/Wox-launcher/Wox/blob/20aeef60860219044a4f3cec83bdbd817a467189/wox.core/plugin/system/app/app_dedup_windows_test.go#L159) |
| LaunchyQt `9041a9e06a95d36e5d24e547ac1fd831db5be950` | CatItem 相等条件为 fullPath 与 shortName；快速 Catalog 用 QSet 在加入时查重。文件扫描创建的是入口路径，并按已索引入口路径跳过重复访问。 | 可借鉴哈希查重，但仅这条规则不能合并位于不同目录的应用快捷方式。慢 Catalog 的加载行为也与快速版本不同，不能泛称全部版本都会消除重复。[相等条件](https://github.com/samsonwang/LaunchyQt/blob/9041a9e06a95d36e5d24e547ac1fd831db5be950/src/LaunchyLib/CatalogItem.cpp#L89)、[快速 Catalog](https://github.com/samsonwang/LaunchyQt/blob/9041a9e06a95d36e5d24e547ac1fd831db5be950/src/Launchy/CatalogFast.cpp#L76)、[扫描入口](https://github.com/samsonwang/LaunchyQt/blob/9041a9e06a95d36e5d24e547ac1fd831db5be950/src/Launchy/CatalogBuilder.cpp#L235) |

性能结论仅为代码结构分析：哈希查重平均 O(n)，同时包含字符串长度成本；Wox 另排序以确定代表项，整体带 O(n log n) 排序成本。实际耗时还取决于快捷方式解析与 I/O，不能据复杂度给出毫秒或内存降幅。

Wox 的启动键复用了其严格的启动快捷路径：还读取链接文件、检查扩展块、用 COM 取字段并检查目标 PE，只接受能理解启动数据的本地 GUI 应用。控制台、安装器、兼容层及未知元数据等回退原入口。这些额外读取有实际成本；其源清单与去重查询快照同时保留，以支持增量删除后晋升备用入口，常驻结构也较多。本项目只在启动/F5 扫描，不能直接继承这种常驻/增量设计。[保守读取范围](https://github.com/Wox-launcher/Wox/blob/20aeef60860219044a4f3cec83bdbd817a467189/wox.core/util/shell/shortcut_windows.go#L67)、[源清单与查询快照](https://github.com/Wox-launcher/Wox/blob/20aeef60860219044a4f3cec83bdbd817a467189/wox.core/plugin/system/app/app.go#L2128)

PicoRun 首版建议保持同名前置条件，对可确认启动语义的普通快捷方式比较目标、原始参数、工作目录、显示模式与运行权限；特殊或不完整信息保留。复用已有 COM 对象与缓冲，一次加载取字段，HashMap 在扫描期间分组；相同组按来源优先级及路径确定代表项，可在插入时比较优先级，避免额外为查重排序。先合并再生成拼音索引，扫描后释放临时键，打开仍执行原 .lnk。读取失败回补旧索引时也必须纳入一致的保守去重规则。

实际实现基于微软 [MS-SHLLINK 结构规范](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-shllink/747629b3-b5be-452a-8101-b9a2ec49978c) 独立编写有界读取器，在同名碰撞时读取 Unicode StringData 的完整参数/工作目录、Show/标志和扩展数据，而不使用可能静默截断的固定缓冲 [GetArguments](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishelllinkw-getarguments)。COM 继续检查目标，256 KiB 以上、特殊标志或不支持的扩展块保守保留；支持的属性存储/追踪/图标环境块按完整字节比较，差异不合并。补齐标准 [KnownFolderDataBlock](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-shllink/5c7410e4-ec19-4ec5-8fff-cf4ccc46c5b6) 的完整 GUID/偏移比较后，截图中的 AB Download Manager 两项也能合并。临时共享目标池省去常见同名组的第二次 COM Load，过预算回退重读或独立保留；不把这些元数据加入常驻模型。未实现跨名称别名合并、Wox 的 PE 检查或多源增量快照。

不同名称的合并可随后单独评估：参考 Wox 保留中文、英文及拼音搜索别名，增加有界的别名存储。仅同名合并已经能处理本轮截图的两组重复，无需先扩展模型或常驻源清单。实现后以 500/2000/10000 项及不同重复比例比较启动/F5 的 P50/P95、完整进程私有提交/工作集/峰值，同时检验参数大小写、工作目录、运行权限、特殊快捷方式、目录失败和备用入口，不把其他项目的核心复杂度当作本项目性能结果。

## 2026-10-04：输入模式恢复与重复入口

原英文功能仅切换并备份 PicoRun 线程的 HKL，没有保存原应用焦点输入框的中文/英文模式。实际试验显示直接切英文 HKL 会影响 TSF 在原控件中的布局记忆；改为优先保留布局、临时关闭组合输入，恢复时使用原状态。外部查询/恢复使用有界标量消息，避免跨进程异步 IME 控制消息被 Windows 拒绝后遗留旧备份。两个原生进程的真实按键覆盖原先中文和英文、重复呼出及退出路径；失败尝试保留在忽略目录。截图重复源于桌面与开始菜单的不同入口路径，该轮尚未改扫描去重语义；后续实现见本文件上面的去重记录。证据与输入法兼容边界见 [INPUT_SESSION_RESULTS.md](INPUT_SESSION_RESULTS.md)。

## 2026-10-04：选择重绘与闪烁

上下键原先每次使整个面板失效，GDI 先清空背景再绘制列表，存在文字暂时消失的阶段。改为仅使变化的两行失效，并复用单行位图完成背景、文字与图标后复制；边界不重绘，隐藏释放位图和 DC。真实窗口对照覆盖亮暗主题、图标开关、已有/空查询、无结果、重开和缩放。额外比较的 DIB 方案完整进程占用更高，保留兼容位图。耗时、完整进程内存和未验证边界见 [SELECTION_RENDER_RESULTS.md](SELECTION_RENDER_RESULTS.md)。

## 2026-10-04：可选登录自启动

加入默认关闭的托盘开关，使用 Win32 注册表 API 写入 HKCU 的 `Run\PicoRun`，按当前 exe、热键和显式目录构造带 `--hidden` 的启动命令。普通运行不写注册表，不加轮询、任务或服务。真实窗口探针在非 Run 隔离项验证勾选、删除、外部变化与进程重启，再直接执行保存的命令；实际用户 Run 值保持原样。完整测试、采样、系统 API 来源和未验证登录边界见 [STARTUP_RESULTS.md](STARTUP_RESULTS.md)。

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

## 2026-10-03：应用结果图标

正式应用保持无结果图标，另建原生实验版本对比后台 Shell 提取与直接 exe/dll/ico 资源提取。394 个真实入口各路径 5 轮开/关，500 个合成入口各路径 3 轮开/关，共 32 个进程；只加载可见项，48 条缓存，空闲阻塞。直接提取相对本批无图标对照，连续查询后的私有提交中位数增加 1.15 MiB、工作集增加 2.37 MiB，查询探针中位数增加约 0.45 ms，完整 12 行缓存绘制增加约 0.61 ms。大量不同结果的活动负载 CPU 时间约翻倍，累计私有提交峰值增加约 10 MiB，不能只看常驻数字。

Shell 路径相对自己的对照增加约 5.34 MiB 私有提交、10.41 MiB 工作集，首屏图标全部就绪中位数约 373 ms；直接路径约 128 ms，但使用通用图标回退，不能完全复制 Shell 图标处理。建议使用后台直接提取和可关闭选项，针对低内存目标默认关闭。完整统计口径、系统 API 来源、复现补丁、未验证边界见 [ICON_EVALUATION.md](ICON_EVALUATION.md)。没有打开任何索引应用，也没有导出个人名称或路径。

按用户要求保留上述基线，在独立原型中添加 512 条/128 KiB 有界解析缓存、按原样资源路径和索引共享的 48 个资源图标缓存、共享通用图标/负结果，以及图标完成后的局部重绘。三种配置在同一 release 下重测：394 个真实入口各 5 轮、500 个合成入口各 3 轮。真实活动 CPU 中位数 9.94 → 8.53 s（−14.2%），新结果查询绘制 5.96 → 5.07 ms（−15.0%）；常驻变化很小，最大私有提交峰值 13.87 → 14.09 MiB，未解决峰值。前 200 次真实查询提取 224 → 166 次；合成重复资源样本有更大收益，不能推广为真实比例。窗口刷新、缓存边界、完整过程与可重建补丁见 [ICON_CACHE_RESULTS.md](ICON_CACHE_RESULTS.md)，正式源码和 exe 保持未合并图标。

随后按用户要求加入正式托盘开关，默认关闭、持久化，关闭释放图标线程/缓存/快照，开启采用上述优化方案。394 个真实入口各 5 轮、500 个合成入口各 3 轮，与冻结优化原型同场对照，共 32 个性能进程。正式关闭与原型关闭私有提交中位数均约 4.32 MiB，正式开启约 5.91 MiB；真实查询绘制原型关闭/正式关闭为 9.39/9.07 ms，原型开启/正式开启为 8.61/8.50 ms。开启后关闭不再加载图标，但完整工作集未恢复到全新关闭启动；本轮未复现上一批约 5 ms 的绝对耗时，冻结原型也变慢，合成样本部分中位耗时增加，不能宣称全部负载保持原数字。正式窗口菜单、保存、连续切换、刷新和既有回归通过；实际小狼毫/Rime 完整按键复验最终通过，前三次前台中断分别保留、不记作成功。完整口径、局限与可运行 exe 见 [ICON_SWITCH_RESULTS.md](ICON_SWITCH_RESULTS.md)。

