# PicoRun

[English](README.md) | 简体中文

支持中文拼音搜索的轻量 Windows 原生应用启动器。

按 **Alt+Space**，输入应用名称，再按 **Enter** 打开。支持英文、中文、拼音全拼和首字母，例如输入 `微信`、`weixin` 或 `wx` 都能查找微信。

- 支持键盘和鼠标操作，可从托盘快速呼出。
- 亮色和暗色主题，中英文界面切换。
- 可选应用图标、呼出时英文输入、登录自启动。
- 运行期间保留上次查询，再次呼出时全选，直接输入即可替换。

## 安装

**目前尚未发布可直接下载的预编译版本。** 可以先[从源码构建](#从源码构建)，也可以[生成 Windows 安装包和 ZIP](#生成-windows-安装包和-zip)用于分发。

分发包面向 **Windows x64**，最低目标系统为 Windows 10。目前已验证 Windows 11 x64；Windows 10、32 位 Windows 和 ARM64 尚未验证。

| 分发方式 | 使用方法 |
| --- | --- |
| `PicoRun-<版本>-windows-x64-setup.exe` | 运行安装包，安装后从开始菜单打开 PicoRun；桌面快捷方式可选。 |
| `PicoRun-<版本>-windows-x64.zip` | 解压到所需目录，运行 `picorun.exe`。 |

安装包不需要管理员权限，仅为当前用户安装，默认目录为 `%LOCALAPPDATA%\Programs\PicoRun`。运行分发包不需要安装 Rust 或 Python。

升级、移动或卸载前，请从托盘菜单选择**退出 PicoRun**。如果移动的副本启用了登录自启动，先关闭该设置，移动后再从新位置重新开启。

## 日常使用

1. 打开 PicoRun，或在程序运行时按 **Alt+Space**。
2. 输入应用名称、拼音或首字母。
3. 用 **↑ / ↓** 选择结果，再按 **Enter** 打开。

点击其他结果行会先选中，点击已经选中的行则打开应用。键盘选中的结果和默认选中的第一行也可以直接点击打开。

| 快捷键 | 操作 |
| --- | --- |
| **Alt+Space** | 呼出或隐藏 PicoRun |
| **↑ / ↓** | 选择结果 |
| **Enter** | 打开选中的应用 |
| **Esc** | 隐藏搜索面板 |
| **F5** | 刷新应用列表 |
| **Ctrl+Q** | 退出 PicoRun |

面板失去焦点时也会隐藏。隐藏面板或打开应用后，查询内容仍会保留；再次呼出时全选上次文本，直接输入即可替换。内容只在当前运行期间保留，退出程序后不保存。

中文输入法组字和选择候选期间，按键优先交给输入法。如果 Enter 用于确认候选，松开后再按一次 Enter 才会打开应用。

## 托盘设置

单击托盘图标呼出 PicoRun，右键可刷新应用列表、修改设置或退出。通过托盘选择的设置会自动保存。

| 设置 | 默认值 | 作用 |
| --- | --- | --- |
| 界面语言 | 简体中文 | 选择**简体中文**或 **English**，立即生效。 |
| 主题 | 暗色 | 切换亮色或暗色外观。 |
| 显示应用图标 | 关闭 | 在搜索结果旁显示图标。 |
| 呼出时使用英文输入 | 关闭 | 在搜索输入框中临时使用英文，离开后恢复原来的输入模式。 |
| 登录时启动 PicoRun | 关闭 | 登录 Windows 后隐藏启动到托盘。 |

界面语言和英文输入是两个独立设置。切换界面语言会保留应用名称、查询内容和输入模式。安装器的语言选择也独立于应用界面语言。

## 应用发现与常见问题

PicoRun 从当前用户和公共的**开始菜单 Programs 文件夹**、**桌面文件夹**查找应用，在启动时或按 **F5** 时刷新。

- **找不到某个应用：** 将指向其 `.exe` 的快捷方式放到开始菜单或桌面，再按 F5。目前尚不支持 Store/UWP 应用枚举；文档、文件夹和网页快捷方式不纳入结果。
- **刚安装或删除了应用：** 按 F5 或重启 PicoRun 更新列表。
- **出现多个相似名称：** 启动设置不同的快捷方式可能分别保留。打开时会保留各自的参数和工作目录。
- **Alt+Space 已被占用：** 用其他热键启动，例如 `picorun.exe --hotkey Ctrl+Alt+P`。修改启动命令前先退出现有实例。
- **拼音没有匹配到名称：** 部分汉字有多个读音，目前覆盖不完整。可以尝试中文名称或应用名称的其他部分。

程序已经运行时，再次打开 PicoRun 会呼出现有窗口。

### 设置与数据

设置和应用索引保存在 `%LOCALAPPDATA%\PicoRun`。ZIP 版也使用这个位置，因此设置不会随解压目录一起移动。

覆盖升级和卸载安装版会保留这些数据。卸载时，只删除指向本次安装目录的 PicoRun 登录自启动项。删除 ZIP 副本时，先从托盘关闭其登录自启动（如已开启），再退出 PicoRun 并删除解压目录。

### 命令行选项

需要自定义热键、隔离配置或指定应用目录时，可以使用以下选项。运行 `picorun.exe --help` 查看说明。

| 选项 | 作用 |
| --- | --- |
| `--hidden` | 隐藏启动到托盘。 |
| `--hotkey Ctrl+Alt+P` | 使用其他全局热键。 |
| `--theme light` / `--theme dark` | 本次启动覆盖主题。 |
| `--icons on` / `--icons off` | 本次启动覆盖图标显示。 |
| `--data-dir "D:\PicoRunData"` | 指定其他设置和索引目录。 |
| `--source "D:\AppShortcuts"` | 用指定目录替代默认来源；可重复传入多个目录。 |

使用不同的应用来源时，请选择独立的数据目录。命令行的主题和图标覆盖仅对本次启动生效；通过托盘修改才会保存为偏好。

## 开发

PicoRun 使用 **Rust + Win32** 独立实现，采用原生 Edit 输入和 GDI 绘制，专注应用搜索与启动，不引入 WebView 或大型 UI 框架。当前 Rust 依赖列表为空。

### 从源码构建

需要在 Windows 上使用 **Rust MSVC 工具链**和 **Windows SDK**。重新构建前，请退出正在运行的开发版程序。

```powershell
cargo build --release --offline --bin picorun
.\target\release\picorun.exe
```

只有重新生成拼音字典时才需要 Python，见[拼音数据与生成说明](assets/README.md)。

### 生成 Windows 安装包和 ZIP

打包脚本会在 `dist/` 下生成安装包、ZIP、校验和及构建元数据，版本号来自 `Cargo.toml`。

```powershell
# 安装包和 ZIP；需要 Inno Setup 6.7.3 或更新版本
.\tools\package_windows.ps1 -Iscc 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'

# 仅生成 ZIP；不需要 Inno Setup 编译器
.\tools\package_windows.ps1 -ZipOnly
```

编译器查找、静默安装和安装生命周期验证见[安装与打包说明](docs/INSTALLATION.md)。

也可以在 GitHub 的 **Actions → Package Windows → Run workflow** 手动打包。该 workflow 仅在手动启动时运行；完成后从构建产物下载 **PicoRun-windows-x64**，其中包含安装包、ZIP、SHA-256 校验和及构建元数据，保留 30 天。见 [workflow 配置](.github/workflows/package-windows.yml)。

### 检查与技术文档

```powershell
cargo fmt --check
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo build --release --offline
```

- [开发约束](AGENTS.md)、[实施说明](docs/IMPLEMENTATION.md)和[研究记录](docs/RESEARCH.md)。
- [界面语言、当前验证与性能观察](docs/I18N_RESULTS.md)。
- [输入模式恢复](docs/INPUT_SESSION_RESULTS.md)、[鼠标操作](docs/MOUSE_LAUNCH_RESULTS.md)和[文本选区绘制](docs/EDIT_DRAG_RESULTS.md)。
- [应用发现与去重](docs/DEDUP_RESULTS.md)。

搜索核心基准、完整进程内存和真实窗口响应衡量的是不同成本，不能互相替代，也不能用它们承诺低配机器的体验。

## 许可证与致谢

整个项目的发布许可证尚未选定。拼音数据采用 MIT 许可，不代表整个应用采用 MIT。来源与许可见[第三方说明](THIRD_PARTY_NOTICES.md)和[拼音数据许可](third_party/pinyin-data/LICENSE)。仓库和分发包也保留了 Inno Setup 的来源说明与许可文本。
