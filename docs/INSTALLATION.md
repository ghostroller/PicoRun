# Windows 安装与打包

当前提供 Windows x64 的 Inno Setup 安装包和 ZIP。程序保持 Rust + Win32、静态 CRT，无需在用户机器上安装 Rust、Python 或 UI 框架。主程序没有新增安装、下载或更新逻辑。

本次产物、两种 PowerShell 下的生命周期回归和验证边界见 [INSTALLATION_RESULTS.md](INSTALLATION_RESULTS.md)。

## 安装包

默认以当前用户权限安装到 `%LOCALAPPDATA%\Programs\PicoRun`，不请求管理员权限。开始菜单创建 PicoRun 快捷方式，桌面图标初始不勾选。完成页面可启动程序；静默安装不自动启动。

安装器不创建或修改登录自启动。已有的界面语言、主题、英文输入和图标选择继续由 `%LOCALAPPDATA%\PicoRun` 下的数据文件读取。安装程序目录和用户数据目录分别管理；卸载保留用户数据。若需从 ZIP 迁移，先在旧版托盘取消自启动，退出旧版，安装后再按需勾选。

安装器使用固定 `AppId=PicoRun.Native`。后续版本复用上次安装目录、卸载项和桌面图标选择；覆盖更新明确列出的程序文件，不清空整个目录。更改 `Cargo.toml` 的版本后重新打包。不要修改 AppId 或把版本号放入默认目录。

PicoRun 的 `Local\PicoRun.Native.v1` 互斥量同时用于安装、升级和卸载的运行检测。程序仍运行时提示从托盘退出；静默模式返回失败，不强制结束程序或安排重启替换。

卸载只移除安装记录中的程序文件、快捷方式和卸载项。读取当前用户 `Run\PicoRun` 后，解析命令的首个 exe；只有可理解的绝对路径与本次安装的 `picorun.exe` 相同（忽略大小写、规范化路径）才删除该值。不同目录的副本、路径前缀碰撞和无法解析的命令保留，其他值也保留。整个用户数据目录不纳入删除列表。

静默安装示例：

```powershell
Start-Process -FilePath .\PicoRun-0.1.0-windows-x64-setup.exe -ArgumentList '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-' -Wait
```

`/TASKS=desktopicon` 可显式选择桌面图标，`/DIR="D:\Apps\PicoRun"` 可指定安装目录。升级通常使用原目录，以保持已有自启动路径有效。

## ZIP

解压后运行 `picorun.exe`。分发包只含主程序、独立使用说明、第三方说明和许可文本，不包含验证探针、个人应用目录、设置或构建工具。

ZIP 仍使用系统 LocalAppData 下的 `PicoRun` 数据目录，不称为完全便携模式。删除或移动前先退出；启用了自启动时先取消，移动后再重新勾选。ZIP 与安装版采用相同单实例规则。

## 重建

构建机需要 Windows、Rust MSVC 工具链、Windows SDK 和 Inno Setup 6.7.3 或更新版本。编译器仅用于构建，不随产品分发。脚本兼容 Windows PowerShell 5.1+ 和 PowerShell 7。

```powershell
./tools/package_windows.ps1 -Iscc 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
# 仅生成 ZIP，无需 Inno 编译器
./tools/package_windows.ps1 -ZipOnly -OutputDirectory runtime/zip-release
```

也可使用 `INNO_SETUP_ISCC` 或 PATH 中的 `ISCC.exe`；本次工作区便携编译器放在忽略目录 `runtime/build-tools/inno-6.7.3/`。构建脚本不自动下载或安装工具。

版本来自 Cargo 元数据；用 `--release --offline --bin picorun --target x86_64-pc-windows-msvc` 构建，并检查实际 PE 的 x64 标识。固定文件清单形成两种包，ZIP 条目时间固定，同一 PowerShell/.NET 环境下，同样的文件可重复生成相同 ZIP；不同 .NET 压缩实现的 ZIP 字节可能不同，包内文件仍相同。输出目录限定在工作区内，默认 `dist/`，可用 `-OutputDirectory` 指定其他工作区目录。

产物包括 `PicoRun-<版本>-windows-x64-setup.exe`、`PicoRun-<版本>-windows-x64.zip`、`PicoRun-<版本>-SHA256SUMS.txt` 和构建元数据 JSON。`dist/`、中间文件和原始验证日志均忽略，不进入源码提交。

## GitHub 手动打包

[Package Windows](../.github/workflows/package-windows.yml) 是唯一的 Actions workflow，只监听 `workflow_dispatch`，不由 push、PR 或 tag 自动触发。配置进入默认分支后，在 **Actions → Package Windows → Run workflow** 选择分支并运行。

任务使用 `windows-2025` runner、Rust 1.95.0 MSVC 和 Inno Setup 6.7.3。Inno 编译器在 runner 上从官方固定版本下载，校验 SHA-256 后安装；项目的本地打包脚本继续保持不自动下载或安装工具。任务只调用现有 `tools/package_windows.ps1` 构建 Windows x64 安装包和 ZIP，版本来自所选分支的 `Cargo.toml`。

成功后，从该次运行的 **Artifacts** 下载 `PicoRun-windows-x64`，包含上面的四种产物，保留 30 天。workflow 只有源码读取权限，不发布 GitHub Release。此任务负责构建和打包，不执行 GUI、输入法或性能验收。

## 验证与边界

```powershell
./tools/verify_windows_package.ps1 -Iscc '<ISCC.exe 路径>'
```

验证要求没有正在运行的 PicoRun；发现已有实例时停止。测试从真实 ZIP 提取相同主程序，使用同一 Inno 源文件生成两个版本的隔离安装器，仅替换 AppId、安装路径、快捷方式名称和注册表测试命名空间。安装、覆盖升级、运行检测、卸载和命令归属解析均走真实流程。

专用自启动数据写到唯一的 `HKCU\Software\PicoRun\Verification\Packaging-<标识>`，不写实际 Run 项；快捷方式和卸载项仅属于本次测试，结束正常卸载并清理。比较用户实际 Run 值与设置/缓存哈希，确认没有变化。源程序已有实例的保存与恢复由本轮工作区辅助脚本处理，不包含在分发包。

安装器提供简体中文和英文；应用界面语言通过托盘独立选择并保存，不由安装器覆盖。简体中文文件来自 Inno 官方仓库的用户贡献翻译，原作者信息与许可保留；不是 PicoRun 项目许可证。整体发布许可证仍由用户决定。目前安装器有版本元数据，主程序 PE 版本资源、专用品牌图标和代码签名仍未配置。

已验证环境为 Windows 11 x64。安装器限制为 x64 Windows 10 或更新系统；这表示安装范围，不表示已在 Windows 10 实机验收。32 位、ARM64、极低配机器、实际登录自启动和系统“已安装应用”页面的交互仍待验证。

Inno 官方依据：[当前用户权限](https://jrsoftware.org/ishelp/topic_setup_privilegesrequired.htm)、[固定 AppId](https://jrsoftware.org/ishelp/topic_setup_appid.htm)、[AppMutex](https://jrsoftware.org/ishelp/topic_setup_appmutex.htm)、[命令行参数](https://jrsoftware.org/ishelp/topic_setupcmdline.htm)、[卸载事件](https://jrsoftware.org/ishelp/topic_scriptevents.htm)。
