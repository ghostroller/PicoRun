# Windows 分发验证

2026-10-05，Windows 11 Enterprise build 26200、x64，i7-11700K / 32 GiB。本轮新增构建与安装脚本，没有修改 Rust 主程序或搜索核心，没有安装到用户正式程序目录。

## 首轮产物（i18n 前）

本表记录首轮打包基线；随后加入中英文界面的更新产物与回归见 [I18N_RESULTS.md](I18N_RESULTS.md)。

首轮 `dist/` 中提供 0.1.0 安装包、ZIP、SHA256SUMS 和构建元数据，不提交二进制。两种包共用显式的五文件清单：主程序、独立使用说明、第三方说明、拼音数据 MIT 许可及 Inno Setup 许可；没有用户清单、配置、探针、编译器或额外运行时。

| 文件 | 字节 | SHA256 |
| --- | ---: | --- |
| PicoRun-0.1.0-windows-x64-setup.exe | 2,413,613 | `c9569b92e1fd275974cad1ec67dce09d284b13dd5dfbbf1c14c18dc30234e4ad` |
| PicoRun-0.1.0-windows-x64.zip | 402,766 | `6a06ed54bcdec05559cb9f5ea1be3e6c10b652f26f7b4bd6dba25605dedc1849` |
| 包内 picorun.exe | 772,608 | `B8BAD99519218D997F8ABFE71F3346C72C7E5562B76711183F0F2EED90AE9580` |

Rust 1.95.0 / MSVC，显式 `x86_64-pc-windows-msvc` target，release opt-level 3、fat LTO、1 codegen unit、panic abort、strip、静态 CRT。显式 target 构建的哈希与此前默认 host 构建不同；这不是功能或性能对照。

Inno Setup 6.7.3 的官方下载签名验证为 Valid，发布者为 Pyrsys B.V.；原安装程序 SHA256 为 `9C73C3BAE7ED48D44112A0F48E66742C00090BDB5BEF71D9D3C056C66E97B732`。使用其 `/PORTABLE=1` 模式解包到工作区忽略目录，未注册编译器快捷方式、文件关联或卸载项。产品安装器未配置代码签名。

## 功能回归

PowerShell 7.4.1 / .NET 8.0.1 与 Windows PowerShell 5.1 分别完成一轮，每轮 **53 项检查**全部通过。每轮使用空的受控应用来源，两次运行实际包内的主程序；从同一 `.iss` 编译 0.1.0 和仅用于升级验证的 0.1.1 两个隔离安装器。

覆盖默认安装、Unicode/空格目录、HKCU 卸载项、默认开始菜单快捷方式、桌面图标默认关闭与显式启用、原目录覆盖升级、已存在自启动命令保留、程序运行时拒绝升级/卸载、完整卸载及快捷方式清理、保留安装目录中的无关文件。安装后与 ZIP 中的 exe 均匹配构建哈希，实际原生 Edit 接受中英混合文本并正常退出。

卸载注册清理覆盖带引号的中文路径、大小写差异、无引号的绝对路径、其他目录副本、`.exe.backup` 前缀碰撞、未闭合引号和引号后错误字符。只删除属于本安装的 `PicoRun` 值，保留同一测试键中的其他值。所有测试注册使用唯一的非 Run 命名空间；实际用户 Run 值、主题/英文输入/图标设置和应用缓存哈希前后相同。

验证期间保存并正常退出原来的开发版 PicoRun，结束后恢复原启动参数、查询、选择和隐藏状态。最终只有恢复的开发版实例运行；测试卸载项、快捷方式和专用注册表项已清理。

构建脚本在 Windows PowerShell 5.1 下也生成 ZIP。PowerShell 7 下两次重建 ZIP 哈希相同；PowerShell 5.1 使用 .NET Framework 的不同压缩实现，ZIP 为 414,764 字节，SHA256 为 `30178477203D95634AFFF5C9679C0D70FA0F05929BF7619FFAFAD00E3F28F3BB`。逐文件验证两种 ZIP 的名称和内容哈希完全相同，不宣称跨 .NET 版本的压缩字节一致。

`cargo test --offline` 的 29 项单元测试与 5 项搜索流程测试、`cargo fmt --check`、`cargo clippy --offline --all-targets -- -D warnings`、`cargo build --release --offline` 均通过。

## 修正与证据

初轮探针没有等待卸载器复制自身后的第二阶段，已改用 Windows `Start-Process -Wait` 等待整个流程。随后中文路径归属测试发现未指定类型的引号字面量使 `Pos` 选择 ANSI 重载：最小样本的引号位置为 30，而 Unicode 字符索引应为 26。改用显式 `String` 引号参数后索引与截取统一，中文路径与完整两轮回归通过。安装器版本资源的字符串有末尾填充空格，测试读取时使用 Trim。失败记录保留，一项失败流程遗留的专用非 Run 键已清理。

成功原始证据位于忽略目录 `runtime/probe-packaging/8fe26288b6b046b7ba0897e3b3b28553/`（PowerShell 7）和 `runtime/probe-packaging/31a6c4db49ae43d4ae207f5e1cf6ca9c/`（PowerShell 5.1）；含每阶段安装器日志、退出码及检查列表。其他失败、中间构建、最小解析测试和用户状态恢复记录同样在忽略目录，不提交真实路径或个人清单。

## 适用范围

安装生命周期采用真实 Inno 流程，但测试构建替换了 AppId、默认目录、快捷方式名称和自启动测试键，避免改变用户正式安装和 Run 注册。没有在正式默认程序目录安装，也没有通过 Windows 设置页面执行卸载。普通安装向导的视觉布局未单独做截图验收。

本轮只有搜索测试和包内原生窗口功能检查，没有新增搜索速度、完整进程内存或按键到画面的性能实验；文件大小不是常驻内存指标，也不据安装方式宣称提速。Windows 10 实机、极低配、32 位、ARM64 和实际登录流程仍未验证。安装器限制为 x64 Windows 10 及更新系统；整体发布许可证、主程序版本资源、品牌图标和签名仍由后续工作处理。使用与重建方法见 [INSTALLATION.md](INSTALLATION.md)。
