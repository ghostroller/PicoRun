# PicoRun

English | [简体中文](README.zh-CN.md)

A small native Windows app launcher with Chinese pinyin search.

Press **Alt+Space**, type an app name, and press **Enter** to open it. Search using English, Chinese, full pinyin, or pinyin initials—for example, find 微信 with `微信`, `weixin`, or `wx`.

- Keyboard and mouse controls, with a tray icon for quick access.
- Light and dark themes; English and Simplified Chinese interfaces.
- Optional app icons, English input on opening, and startup at sign-in.
- Remembers your query while running and selects it when reopened, ready to replace.

## Installation

Download the installer or ZIP from [GitHub Releases](https://github.com/ghostroller/PicoRun/releases/latest). Each release lists the versioned Windows x64 packages, SHA-256 checksums, and build metadata. You can also [build from source](#build-from-source).

Packages target **Windows x64**, with Windows 10 as the minimum target. Prior testing covers Windows 11 x64; packaged app discovery and search have also been checked on Windows 10 22H2 x64. Full Windows 10 compatibility, low-end hardware, 32-bit Windows, and ARM64 remain unverified.

| Package | How to use it |
| --- | --- |
| `PicoRun-<version>-windows-x64-setup.exe` | Run the installer, then open PicoRun from the Start menu. A desktop shortcut is optional. |
| `PicoRun-<version>-windows-x64.zip` | Extract the archive to a folder and run `picorun.exe`. |

The installer runs without administrator privileges and installs for the current user, by default to `%LOCALAPPDATA%\Programs\PicoRun`. Packaged builds do not require Rust or Python on your computer.

Before upgrading, moving, or uninstalling PicoRun, choose **Quit PicoRun** from its tray menu. If you move a copy with sign-in startup enabled, turn that setting off first and enable it again from the new location.

## Everyday use

1. Open PicoRun, or press **Alt+Space** while it is running.
2. Type an app name, pinyin, or initials.
3. Use **↑ / ↓** to select a result, then **Enter** to open it.

Clicking a different result selects it; clicking the selected result opens it. This also works for a result selected with the keyboard, including the initially selected first row.

| Shortcut | Action |
| --- | --- |
| **Alt+Space** | Show or hide PicoRun |
| **↑ / ↓** | Select a result |
| **Enter** | Open the selected app |
| **Esc** | Hide the search panel |
| **F5** | Refresh the app list |
| **Ctrl+Q** | Quit PicoRun |

The panel also hides when it loses focus. Hiding it or opening an app keeps your query; reopening selects the previous text so typing replaces it. Queries are kept only for the current session and are not saved after quitting.

During Chinese IME composition, candidate selection takes priority over launcher shortcuts. If Enter confirms a candidate, release it and press Enter again to open the app.

## Tray settings

Click the tray icon to show PicoRun. Right-click it to refresh the app list, change settings, or quit. Settings selected from the tray are saved automatically.

| Setting | Default | What it does |
| --- | --- | --- |
| UI language | Simplified Chinese | Choose **简体中文** or **English**; changes take effect immediately. |
| Theme | Dark | Choose a light or dark appearance. |
| Show app icons | Off | Display icons beside search results. |
| English input when opened | Off | Temporarily use English input in the search field, then restore the previous input mode when you leave. |
| Start PicoRun at sign-in | Off | Start PicoRun hidden in the tray when you sign in to Windows. |

UI language and English input are independent settings. Changing the interface language preserves app names, your query, and your input mode. The installer's language selection is also separate from the app's interface language.

## Finding apps and troubleshooting

By default, PicoRun looks for apps in your user and shared **Start menu Programs** folders and **Desktop** folders, plus installed packaged apps for the current user through Windows **AppsFolder**. It refreshes this list at startup or when you press **F5**; there is no background polling.

- **An app is missing:** press F5 after installing it. For a desktop app without a discoverable shortcut, add a shortcut pointing to its `.exe` to the Start menu or Desktop. Documents, folders, and web shortcuts are excluded.
- **A Store app has a different name:** packaged apps use the localized name supplied by Windows. Try that name or its pinyin; opening uses the app's Windows activation ID (AUMID). With app icons enabled, PicoRun loads their icons from Windows AppsFolder.
- **Using `--source`:** custom folders replace all default sources, including packaged apps. Omit this option to include installed Store apps.
- **An app was installed or removed:** press F5 or restart PicoRun to update the list.
- **Similar names appear more than once:** shortcuts with different launch settings may be kept separately. PicoRun preserves their arguments and working directories when opening them.
- **Alt+Space is already in use:** start PicoRun with another hotkey, such as `picorun.exe --hotkey Ctrl+Alt+P`. Quit any existing instance before changing the startup command.
- **Pinyin does not match a name:** some characters have multiple pronunciations; coverage is incomplete. Try the Chinese name or another part of the app's name.

Opening PicoRun again while it is already running shows the existing window.

### Settings and data

Settings and the app index are stored in `%LOCALAPPDATA%\PicoRun`. The ZIP build uses the same location, so its settings do not travel with the extracted folder.

Upgrades and uninstalling the installer build preserve this data. Uninstalling removes PicoRun's sign-in startup entry only when it points to that installation. To remove a ZIP copy, disable its sign-in startup from the tray if enabled, quit PicoRun, and delete the extracted folder.

### Command-line options

These options are useful for a different hotkey, an isolated configuration, or a custom app folder. Run `picorun.exe --help` for usage.

| Option | Effect |
| --- | --- |
| `--hidden` | Start hidden in the tray. |
| `--hotkey Ctrl+Alt+P` | Use a different global hotkey. |
| `--theme light` / `--theme dark` | Override the theme for this launch. |
| `--icons on` / `--icons off` | Override app icons for this launch. |
| `--data-dir "D:\PicoRunData"` | Use another settings and index folder. |
| `--source "D:\AppShortcuts"` | Search this folder instead of the default locations; repeat the option for multiple folders. |

Choose a separate data directory when using a different set of source folders. Theme and icon overrides become saved preferences only if you change them from the tray menu.

## Development

PicoRun is independently implemented in **Rust + Win32**, with native Edit input and GDI rendering. It focuses on app search and launching, with no WebView or large UI framework. The current Rust dependency list is empty.

### Build from source

Build on Windows with the **Rust MSVC toolchain** and **Windows SDK**. Exit a running development build before rebuilding its executable.

```powershell
cargo build --release --offline --bin picorun
.\target\release\picorun.exe
```

Python is only needed if you regenerate the pinyin dictionary; see [pinyin data and generation](assets/README.md).

### Create Windows packages

The packaging script produces an installer, ZIP, checksums, and build metadata in `dist/`. The version comes from `Cargo.toml`.

```powershell
# Installer and ZIP; requires Inno Setup 6.7.3 or newer
.\tools\package_windows.ps1 -Iscc 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'

# ZIP only; no Inno Setup compiler required
.\tools\package_windows.ps1 -ZipOnly
```

See [installation and packaging details](docs/INSTALLATION.md) for compiler discovery, silent installation, and lifecycle verification.

To publish on GitHub, push a version tag matching `Cargo.toml`, such as `v0.1.0`. Open **Actions → Package Windows → Run workflow**, select `main` for the workflow, and enter the tag. The workflow runs only when started manually, builds the tagged source, and publishes the installer, ZIP, SHA-256 checksums, and build metadata to that tag's Release. It also retains an Actions artifact for 30 days. Existing published releases are not overwritten. See [the workflow](.github/workflows/package-windows.yml).

### Checks and engineering notes

```powershell
cargo fmt --check
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo build --release --offline
```

Detailed engineering notes are currently in Chinese:

- [Development constraints](AGENTS.md), [implementation](docs/IMPLEMENTATION.md), and [research](docs/RESEARCH.md).
- [UI language, current validation, and performance observations](docs/I18N_RESULTS.md).
- [Input mode restoration](docs/INPUT_SESSION_RESULTS.md), [mouse controls](docs/MOUSE_LAUNCH_RESULTS.md), and [text selection rendering](docs/EDIT_DRAG_RESULTS.md).
- [App discovery and deduplication](docs/DEDUP_RESULTS.md).

Search benchmarks, full-process memory measurements, and native window response tests cover different costs. Their results should not be treated as interchangeable or as guarantees for low-end hardware.

## License and acknowledgments

A project-wide license has not been selected yet. The pinyin data is MIT-licensed; its license does not apply to the whole application. See [third-party notices](THIRD_PARTY_NOTICES.md) and the [pinyin data license](third_party/pinyin-data/LICENSE). Inno Setup attribution and its license are also included in the repository and distribution packages.
