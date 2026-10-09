# 2026-10-09：Flow Launcher 应用发现范围核对

本轮按用户要求只读研究成熟启动器 Flow Launcher 的官方 Program 插件，判断 PicoRun 可能遗漏的应用入口。没有复制外部实现，没有扩展本轮生产扫描代码，没有导出私人应用名称或路径。下面的优先级依据源码覆盖范围，不是本设备应用覆盖率或性能收益实测。

后续实施：用户授权前三项建议后，已独立补齐 App Paths、StartMenu 根目录、ClickOnce 和有界名称别名，见 [DISCOVERY_SOURCES_RESULTS.md](DISCOVERY_SOURCES_RESULTS.md)。下表的“PicoRun 当前”及“未实现”描述均指调查时的 b251acc 状态；原调查证据和后续实现分开保留。

## 来源与复现边界

固定官方仓库 Flow-Launcher/Flow.Launcher 的 dev 提交 [c68afbcc05f37e9d1c27bd0338313b48fb0782a6](https://github.com/Flow-Launcher/Flow.Launcher/commit/c68afbcc05f37e9d1c27bd0338313b48fb0782a6)，通过 git ls-remote、GitHub Commit/Tree API 和该 SHA 的 raw 文件核对。此处是开发分支源码快照，不代表所有发布版本。只读公开源码缓存位于已忽略的 runtime/flow-discovery-audit/，不属于 PicoRun 构建依赖，也不提交缓存。

此前 [RESEARCH.md](RESEARCH.md) 的去重调查固定在另一个提交，保留其当时范围。本轮结论对应上面的 SHA。PicoRun 对照依据 [README.md](../README.md)、[discovery.rs](../src/platform/windows/discovery.rs) 与 [packaged.rs](../src/platform/windows/discovery/packaged.rs)。

## 默认来源与可选来源

| 来源／能力 | Flow Launcher 此快照 | PicoRun 当前 | 覆盖差异及建议 |
| --- | --- | --- | --- |
| 开始菜单 | 默认启用；用户／公共 StartMenu 根目录递归，排除两类 Startup 子树 | 用户／公共 Programs 目录递归 | 优先补 StartMenu 根本层的合法应用入口；已有 Programs 子树不重复扫描，排除 Startup |
| App Paths 注册表 | 默认启用；HKLM／HKCU Software\Microsoft\Windows\CurrentVersion\App Paths 子键默认值 | 未实现 | 最高优先级：补已注册但没有开始菜单／桌面快捷方式的应用；需核对双视图与启动环境 |
| 桌面 | Program 插件没有默认桌面源；可添加自定义路径 | 默认扫描用户／公共桌面 | PicoRun 已覆盖，不能把桌面称作 Flow 的默认来源 |
| 自定义目录 | 默认空；启用目录递归扫描，叠加默认来源；合并重叠根减少 I/O | --source 替换全部默认来源，包括打包应用 | 可选附加来源便于便携应用；保留现有替换语义 |
| PATH | 默认关闭；开启后只扫描各 PATH 目录一层 | 未实现 | 次级、可选，避免默认纳入大量命令行工具与系统项 |
| ClickOnce | .appref-ms 默认启用 | 仅 .lnk／.exe | 高优先级：当前会完全遗漏这种应用入口 |
| 游戏协议快捷方式 | 默认接受限定 Steam／Epic 协议的 .url；HTTP／HTTPS 默认关闭 | 不接受 .url | 中优先级；限定应用激活协议，不泛化为网页入口 |
| 打包应用 | 默认启用；当前用户 PackageManager → 各包 GetAppListEntries() | 当前用户 AppsFolder → 已注册打包 AUMID | 先按 AUMID 差分验证，不能凭源码断言哪一种更完整 |
| 搜索名称 | .lnk 使用 Shell 本地化名；未命中时尝试原快捷方式名与目标 exe 文件名；描述搜索默认关闭 | 普通入口按文件名，打包入口按 Shell 本地化名 | 评估预计算有界别名，区分真正漏扫与已索引但名字不匹配 |

默认开关见 [Settings.cs#L117-L124](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Settings.cs#L117-L124)。默认文件类型是 .exe、.appref-ms、.lnk；Steam／Epic 开启、HTTP 关闭；自定义后缀和协议默认关闭，见 [Settings.cs#L16-L42](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Settings.cs#L16-L42)。存在启用协议时加入 .url 后缀，见 [Settings.cs#L44-L71](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Settings.cs#L44-L71)。

目录枚举默认递归且忽略不可访问目录；开始菜单排除 Startup；PATH 显式关闭递归，见 [Win32.cs#L456-L524](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L456-L524)。开始菜单返回 StartMenu 而非 Programs 已知目录，见 [Win32.cs#L750-L763](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L750-L763)。自定义来源按启用状态与存在性过滤，递归后合并默认源，见 [Win32.cs#L668-L706](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L668-L706)；重叠根归并见 [Win32.cs#L829-L849](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L829-L849)。

PicoRun 文件扫描有 16 层边界，不追踪符号链接或 reparse point；不能因参考项目递归较宽就直接删除安全边界。

## App Paths：优先补齐，保留注册启动语义

Flow 取 HKLM／HKCU App Paths 子键默认字符串，裁去外围引号与空格，再按后缀分类，随后展开环境变量。本段没有显式同时枚举 WOW64 32／64 位视图，也没有读取 App Paths 的 Path 值，见 [Win32.cs#L527-L605](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L527-L605)。

PicoRun 独立实现应明确 x64 下 HKCU／HKLM 与 32／64 位视图的读取及去重，使用有界原生读取，不启动外部脚本。只接受可确认的应用注册入口，失效或无法理解的值保守处理。可优先现有快捷方式作代表；不能仅因 exe 相同就合并带参数、工作目录、显示模式或权限差异的入口。

微软规定默认值映射应用名到完整路径，Path 值可影响 Shell 启动后进程的搜索环境。把默认值直接转为裸 exe 后打开，不能宣称保留全部注册启动语义。后续需明确索引身份、Shell 注册启动、同名冲突规则，并以受控测试程序验证 Path 环境及 HKCU／HKLM 优先级，见 [Application Registration](https://learn.microsoft.com/en-us/windows/win32/shell/app-registration)。本轮未实施或实测这些规则。

## 特殊入口、名称与产品范围

Flow 的 .lnk 接纳较宽：LnkProgram() 不要求目标扩展名为 .exe，可读取但没有普通文件目标的快捷方式也可能保留。有真实目标时保存 exe 名、参数与描述，再取 Shell 本地化名称，见 [Win32.cs#L334-L370](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L334-L370)。PicoRun 的 exe 过滤可能排除特殊 Shell／协议应用入口，但直接放开全部 .lnk 也会纳入文件、目录或网页。应先核实漏项类型，再增加严格应用识别。

Flow 解析 InternetShortcut.URL，按启用前缀接纳，图标可读 IconFile。默认前缀为 steam://run/、steam://rungameid/、com.epicgames.launcher://apps/，见 [Settings.cs#L32-L38](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Settings.cs#L32-L38)、[Win32.cs#L390-L427](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L390-L427)。参考这些应用激活入口不授权 PicoRun 增加通用网址搜索。

名称显示优先 Shell 本地化名；名称匹配不足时尝试目标 exe 文件名和原快捷方式名。描述搜索是默认关闭的选项，见 [Win32.cs#L104-L150](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L104-L150)。本轮未见 Program 插件为全部应用提供任意用户搜索别名，也未见专门枚举 AppExecutionAlias；可选 PATH 可能接触这些入口，不等于默认索引所有执行别名。PicoRun 如加入别名，应扫描时预计算并限制条数／字节，搜索时不解析候选或转换拼音。

Windows 设置搜索是 Flow 的独立 WindowsSettings 插件，读取自身 JSON 清单、按系统版本过滤并翻译；它不是 Program 扫描遗漏，不纳入本轮 PicoRun 范围，见 [WindowsSettings/Main.cs#L75-L104](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.WindowsSettings/Main.cs#L75-L104)。

## 打包应用与隐藏入口

Flow 用当前用户 SID 调用 PackageManager.FindPackagesForUser()，排除 IsFramework、IsDevelopmentMode 和安装路径为空的包。各包通过 GetAppListEntries() 建立应用，使用其 AUMID、本地化 DisplayName 与 Description；再排除用户禁用的 AUMID，见 [UWPPackage.cs#L48-L68](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/UWPPackage.cs#L48-L68)、[UWPPackage.cs#L199-L284](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/UWPPackage.cs#L199-L284)、[UWPPackage.cs#L387-L394](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/UWPPackage.cs#L387-L394)。

此段未见独立 IsResourcePackage 过滤，也未见自行从 manifest 恢复 AppListEntry="none" 隐藏入口的扫描。Flow 读取 manifest 用于图标等附加信息，应用列表来自系统 API。微软定义 AppListEntry="none" 为无需 All Apps 列表项的入口，见 [uap:VisualElements](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-uap-visualelements)。依赖包和隐藏辅助入口不能简单计入应展示的应用，开发模式包排除也不宜盲目移植。

PicoRun AppsFolder 路径已经由 Shell 给出本地化名，并核对 AUMID 对应当前用户已注册包家族。是否与 Flow 的包内 AppList 列表完全一致，需要按 AUMID 做本地差分，真实清单只放忽略目录。本轮没有该差分，不能声称一方更完整，不能保证两者对所有隐藏、稀疏或开发包相同。

Flow 此快照使用的同步 GetAppListEntries() 由微软标注 Windows 10 2004／19041 起提供，见 [Package.GetAppListEntries](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.package.getapplistentries?view=winrt-26100)。不能据这段源码宣称全部 Windows 10 兼容，也不据此替换 PicoRun 当前原生 AppsFolder 路径。

## 去重与过滤

Flow 自动开始菜单和 App Paths 来源按目标路径加参数整体转小写分组，优先开始菜单快捷方式，再优先有描述项；自定义目录与 PATH 未经过此启动分组，最终全部按入口 UID 去重，见 [Win32.cs#L644-L706](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L644-L706)。启动键不含工作目录、显示模式和运行权限，参数也转小写，不能替代 PicoRun 已有保守规则。

Flow 默认不隐藏卸载程序，也不隐藏 WindowsApps 目录中与打包项重复的 Win32 项；这是可选查询过滤，见 [Settings.cs#L120-L124](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Settings.cs#L120-L124)、[Main.cs#L156-L203](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Main.cs#L156-L203)。本轮未见 Win32 文件扫描按隐藏文件属性排除入口。用户禁用单项依据路径或 AUMID UID，与默认源和自定义目录开关分开。

## 刷新、缓存与监听

Flow 启动先读 Win32／UWP 缓存，缓存为空或距上次索引超过 30 小时才重建。这是启动时检查，不是 30 小时周期轮询。ReloadDataAsync() 可手动重建全部，见 [Main.cs#L302-L327](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Main.cs#L302-L327)、[Main.cs#L559-L562](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Main.cs#L559-L562)。

常驻 FileSystemWatcher 监听开始菜单和自定义目录创建／删除，包含子目录，按扫描后缀筛选；容量 1 队列合并事件，500 ms 后重建全部 Win32。此段未见独立 App Paths 注册表或 PATH 变更监听，见 [Win32.cs#L766-L818](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/Win32.cs#L766-L818)。

UWP 使用当前用户 PackageCatalog 监听安装／卸载／更新完成，3 秒合并后重建全部打包应用，见 [UWPPackage.cs#L295-L321](https://github.com/Flow-Launcher/Flow.Launcher/blob/c68afbcc05f37e9d1c27bd0338313b48fb0782a6/Plugins/Flow.Launcher.Plugin.Program/Programs/UWPPackage.cs#L295-L321)。这些是 Flow 的取舍，不是本轮对 PicoRun 的实现建议。PicoRun 维持启动／F5 扫描和阻塞空闲，不增加 watchers、周期轮询、外部脚本或常驻包目录元数据。

## 推荐顺序与验证边界

1. App Paths：有界读取 HKCU／HKLM 和双注册表视图；明确环境、同名注册与启动语义；和快捷方式保守去重。
2. StartMenu 根本层与 ClickOnce：根本层补漏，保留现有 Programs 递归并排除 Startup；支持有效 .appref-ms 应用入口。
3. 名称别名：Shell 本地化名、原快捷方式名和目标 exe 名的有界预计算；检验中文、英文、全拼和首字母。
4. 限定游戏协议：支持可识别应用激活入口，网页入口继续排除。
5. 可选附加目录／PATH：便携目录叠加默认来源；PATH 仅一层，不全盘扫描。

每步实现先以受控测试程序和合成入口证明正确打开，再执行对应测试、fmt、离线 Clippy 和 release 构建。扫描覆盖与名称匹配分开测试；还应验证来源失败、重复入口、注册视图冲突、参数／环境、刷新后备用来源和 500／2000／10000 项规模。搜索核心性能、完整进程私有提交／工作集／峰值、真实窗口响应分别报告，记录机器、构建、冷启动／预热、重复次数、P50／P95 及统计口径。

本轮只交付固定源码证据与优先级，没有读取本设备私人清单做来源差分，没有应用覆盖或性能收益实测，也没有改变扫描行为。