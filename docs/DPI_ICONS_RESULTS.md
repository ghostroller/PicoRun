# DPI 图标尺寸修正

2026-10-09：用户要求先提交 Store 图标修复，已提交为 `b251acc`，随后反馈本机图标发糊。本轮独立修正图标提取与绘制尺寸不一致，并只读核对 Flow Launcher 的应用发现范围，后者见 [FLOW_DISCOVERY_AUDIT.md](FLOW_DISCOVERY_AUDIT.md)。该图标修复阶段未扩展扫描；之后的来源补齐见 [DISCOVERY_SOURCES_RESULTS.md](DISCOVERY_SOURCES_RESULTS.md)。

## 实测原因与实现

本设备为 125%（120 DPI）。renderer 绘制的是 20 逻辑像素，即 25×25 物理像素，而此前 SHGetFileInfoW 的 LARGEICON 实际返回 40×40、32bpp 图标。原生对照探针对 ChatGPT、Microsoft Store 检查旧输出与 IExtractIconW 的 20/25/30/32/40/48 输出：后者逐个返回精确请求尺寸，S_OK、32bpp。相同 25px 绘制的像素对照显示边缘与颜色分布不同，精确尺寸减少一次额外缩放；小图细节仍受包原始素材及系统自身缩放限制，不能称所有图标从此完全高清。

- renderer 用统一 icon_size() 给布局、图标绘制、局部失效区域和 worker 传物理像素尺寸，上限 256px。
- 文件资源保留原 .lnk 图标位置与资源索引解析，改用公开 SHDefExtractIconW 指定尺寸，保留负资源 ID 语义。通用图标从系统共享句柄以 CopyImage 取得指定尺寸的 owned 副本。
- Store 用 SHCreateItemFromParsingName → IShellItem::BindToHandler(BHID_SFUIObject, IExtractIconW) → GetIconLocation → Extract(size)。S_OK 才接收输出；S_FALSE 且不是 GIL_NOTFILENAME 才交给文件提取。尊重 GIL_DONTCACHE，不将该图像放入资源缓存。失败时保留已提交版本的 Shell 图标回退，所以其他 handler 不支持精确尺寸时仍可显示原路径图标。
- worker 只保留当前一种尺寸；尺寸变化一次释放图标、负记录、元数据与通用图标，再取可见项，不让每种 DPI 各占一组 48 条缓存。原 512 条/128 KiB 元数据预算与共 128 KiB 的解析缓冲保持。
- 空结果且从未创建 worker 时，F5 或 DPI 无效化仍保持休眠，并完成测量代数；已有 worker 可以接收空请求，清理旧尺寸资源。无新依赖或常驻工作。
- 仅 --measure-icons 诊断使用 GetIconInfo/GetObjectW 核对当前选中图标实际尺寸，所有副本 GDI 位图立即释放；克隆 Arc 后再调用原生 API，Runtime 借用不跨调用。

依据为微软 [IExtractIconW::Extract](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-iextracticonw-extract)、[GetIconLocation](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-iextracticonw-geticonlocation)、[BindToHandler](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishellitem-bindtohandler)、[SHDefExtractIconW](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shdefextracticonw)。未采用非通用的 PrivateExtractIconsW，也未引入 ImageFactory 的 HBITMAP 转换路径。独立测试的 ImageFactory25 像素和观感没有明确质量优势，其默认缩放也是系统 GDI 路径。

## 验证与边界

`cargo fmt --check`、`cargo test --offline`（61 单元 + 5 集成）、`cargo clippy --offline --all-targets -- -D warnings`、`cargo build --release --offline`、`git diff --check` 均通过。新增尺寸副本、尺寸改变释放旧资源/负缓存、非法尺寸及空结果刷新无 worker 回归。

`native_probe.exe --dpi-icons [--reference]` 对已提交版、最终修复各三个新进程，用独立目录和备用 Ctrl+Alt+Shift+F9。每轮 330 项，其中 22 个打包应用；实际 Edit 目标 AUMID、图标非空、无通用回退、重复查询命中缓存、F5 重取、开关释放和正常退出全部通过。全 22 项的非空检查并不意味着逐项都核验了精确尺寸。

首轮两项报告应用分别模拟 DPI 96/120/144/192/384/120，在实际主程序中等待取图完成、测量 HICON 为 20/25/30/40/80/25，12 项尺寸断言通过，保存 100%/125%/200% 截图并目视核对。模拟 WM_DPICHANGED 验证了尺寸管线和资源替换，不代表真实多显示器迁移或每种设备实测。已提交版没有尺寸诊断，旧40px结果来自独立的原生提取探针。未启动个人安装应用。

普通路径用上轮已有受控脚本：500 项 .lnk，含 Shell32 -154 图标资源，每种 off/on 模式 60 个逐项查询、1000 次快速请求、F5、关闭重查询及正常退出通过。沿用 [STORE_ICONS_RESULTS.md](STORE_ICONS_RESULTS.md) 对旧完整托盘菜单探针未通过的记录；本轮没有据命令处理器回归宣称完整菜单/IME 验收。

## 完整进程与真实窗口响应

机器为 Intel Core i9-11900K、约16GiB、Windows 10 Pro 22H2 / 19045 x64，系统缩放125%。Rust/Cargo 1.95.0、MSVC x64；release opt-level=3、fat LTO、codegen-units=1、panic=abort、strip=true。先 b251acc、后修复，各三个进程；首轮无磁盘索引缓存，后两轮有缓存，均启动扫描。未清 OS/Shell 缓存或工作集，不是物理冷启动。

两组首轮含额外 DPI 循环，会影响全程峰值与组件缓存；后两轮无此负载。全部查询计时前恢复120 DPI，五个固定查询预热100次后测100次，图标等待与尺寸检查在计时之外。初始隐藏采样在找到 Edit 后立即执行，可能尚在完成初始化；启动时间是到窗口创建，不能当完整就绪时间。内存计数仅含完整主程序（Shell/UI/输入组件在内），不含测试工具/外部进程。

各阶段列为三个进程 P50 / nearest-rank P95（n=3 时 P95 是最大项），MiB。两组均提供真实 Store 图标。

| 阶段 | b251acc 私有提交 | 新版私有提交 | b251acc 工作集 | 新版工作集 |
| --- | ---: | ---: | ---: | ---: |
| 隐藏、图标关闭 | 4.871 / 5.082 | 4.883 / 5.512 | 22.988 / 23.059 | 22.570 / 22.902 |
| 显示、图标关闭 | 5.488 / 5.551 | 5.273 / 5.730 | 25.266 / 25.355 | 24.219 / 25.223 |
| 首项图标完成 | 7.613 / 7.672 | 5.766 / 6.074 | 28.211 / 28.227 | 26.215 / 27.398 |
| 全部打包查询后 | 7.910 / 8.344 | 7.734 / 8.141 | 29.266 / 29.340 | 27.453 / 28.551 |
| 测量查询后 | 7.914 / 8.344 | 7.734 / 8.141 | 29.305 / 29.387 | 27.484 / 28.602 |
| F5 后 | 8.609 / 9.035 | 8.070 / 8.516 | 29.895 / 29.941 | 27.801 / 29.258 |
| 隐藏空闲结束 | 8.609 / 9.008 | 8.070 / 8.492 | 29.910 / 29.941 | 27.801 / 29.273 |
| 关闭图标后 | 7.941 / 7.988 | 7.738 / 7.746 | 28.777 / 28.781 | 27.117 / 28.547 |

全程最大累计私有提交峰值 9.480 → 9.324 MiB，工作集峰值 30.871 → 30.188 MiB。最大阶段采样 GDI/USER 数为 164/55 → 162/55，不是句柄全生命周期峰值。首轮自身累计私有提交/工作集峰值分别为旧版9.164/30.871、新版9.113/28.918MiB；不能用首轮缩放负载与后两轮无缩放负载作等价成本对比。

| 指标，ms | b251acc | 新版 |
| --- | ---: | ---: |
| 三次窗口创建 P50 / P95 | 357.1608 / 396.1335 | 383.8274 / 445.1276 |
| 首项图标完成 P50 / P95 | 40.4569 / 67.3387 | 35.1004 / 49.7253 |
| 各轮查询 P50 的中位数 | 1.9459 | 2.0113 |
| 各轮查询 P95 的中位数 | 3.6985 | 3.6601 |
| 三次 F5 P50 / P95 | 313.9213 / 320.7193 | 329.3642 / 339.3182 |
| F5 后图标等待 P50 / P95 | 6.9476 / 9.1681 | 13.8358 / 13.9904 |

查询是跨进程 WM_SETTEXT → 同步查询 → WM_PAINT，不含真实键盘调度、完整 IME、显示扫描输出或异步图标等待。表内查询为各轮分位数的中位数，不是合并300次的分位数。新版第二进程 P50/P95 为2.8442/4.5287ms，明显偏慢，不能据分位数中位数接近宣称无回归。顺序未随机化、样本少；图标关闭基线也有波动，不能把全部内存差值归因于较小 HICON。F5及随后图标等待本轮都有额外成本。六次两秒隐藏空闲 CPU 增量为0计量单位，受 GetProcessTimes 粒度限制，不是绝对零CPU。未验收低配/换页、32位、ARM64或多显示器迁移。

## 独立搜索样本

release search_bench 固定一个可用核，每规模12查询、20轮预热（240次）、100轮重复（1200样本）；索引每规模只构建一次，首搜索是该索引第一次调用，不是物理冷启动。排除发现/图标/窗口/应用启动与完整产品进程成本。

| 合成项数 | 构建 ms | 自有堆容量 bytes | 首搜索 μs | 预热 P50 / P95 ms |
| --- | ---: | ---: | ---: | ---: |
| 500 | 0.6217 | 121074 | 26.200 | 0.0219 / 0.0282 |
| 2000 | 2.0329 | 488874 | 96.400 | 0.0877 / 0.1147 |
| 10000 | 10.0576 | 2461674 | 540.900 | 0.4477 / 0.5823 |

## 交付与证据

本轮图标尺寸验证时、来源补齐之前的工作区 EXE 为 `D:\workspace\PicoRun\target\release\picorun.exe`，805888 bytes，SHA-256 `4E0B53C066BD67F15805EFC36D1E9A0AA9455E5F1D87FEA083242EE12D947368`。先退出当前 PicoRun 再运行，开启托盘图标开关；正常退出仍用Ctrl+Q或托盘。没有替换安装版或发布版本。

忽略目录内的证据：`runtime/probe-dpi-icons/`、`runtime/probe-dpi-icons-reference/` 的checks/timings/完整进程CSV/缓存/截图；`runtime/dpi-icons-summary.json` 匿名统计；`runtime/icon-size-audit.*` 和 `runtime/icon-size-audit/` 原生尺寸、四种HICON及ImageFactory对照；`runtime/icon-eval/dpi-classic-icons-final/` 500项回归；`runtime/dpi-icons-search-bench.txt` 独立核心测量。首轮随后发现的空结果休眠问题已修正，旧fixed批次保留在 `probe-dpi-icons-before-empty-guard/`，不用于本表。个人入口、AUMID和应用清单均未加入仓库。

复验用 `target/release/native_probe.exe --dpi-icons`；`--reference` 要求先把 b251acc EXE 保存在 `runtime/dpi-icons-baseline/picorun.exe`。模拟尺寸结果和名称存在性断言针对本机两项报告应用，不能用于任意机器的通用兼容性承诺。

后续来源补齐改变了当前工作区 EXE；最终扫描版本的构建、图标回归与可执行文件信息见 [DISCOVERY_SOURCES_RESULTS.md](DISCOVERY_SOURCES_RESULTS.md)。
