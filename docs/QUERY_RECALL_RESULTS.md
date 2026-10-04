# 输入框保留与全选验证

2026-10-04，Windows 11 x64，本机已安装的小狼毫/Rime。

## 行为

移除 `show()` 中清空原生 Edit 的操作。Esc、热键隐藏、失去焦点和打开应用后保留查询；再次呼出时先同步实际文本，取得输入框焦点后用 `EM_SETSEL` 全选。托盘呼出与再次运行 exe 激活已有实例采用相同流程。

直接输入替换全选文本，Delete 清空全选文本，Enter 可直接执行保留查询的当前选中结果。复用原生 Edit、控制器和已有查询缓冲，不增加常驻字段或后台工作。内容仅在同一进程内保留；正常退出程序后，新进程从空查询开始，不保存磁盘查询历史。

输入法尚未提交的组合仍按已有流程取消；已提交查询的保留与原输入模式的恢复分别处理。应用去重规则保持当前实现。

## 已通过的验证

- `cargo test --offline`：26 项单元测试与 5 项搜索流程测试通过。
- `cargo fmt --check`、`cargo clippy --offline --all-targets -- -D warnings`、`cargo build --release --offline` 通过。
- `native_probe.exe --ime-real`：109 项断言通过。新增 31 项查询保留检查，覆盖空查询、英文、中文与 UTF-16 代理对、无匹配查询，Esc/热键/失焦隐藏及托盘再次呼出，实际全局热键、第二实例呼出、键入替换、全选 Delete、保留查询直接执行与即时 Enter。快捷方式只启动受控程序。实际 Rime 检查英文替换、中文空格提交、组合 Enter 防重复和取消后再次呼出。
- `native_probe.exe --input-session`：82 项断言通过，正常退出码 0。两个受控进程验证原输入框处于中文或英文模式时的呼出，以及 Esc、热键、失焦、受控 Enter、关闭英文选项、正常退出后的模式恢复；返回原输入框实际键入确认。
- `native_probe.exe --flicker`：亮/暗主题 × 图标关闭/开启，各 3 次独立进程，共 9,888 项断言通过，正常退出码 0。覆盖首次上下键、查询不变、无结果查询重开、两行局部重绘、隐藏释放行缓冲与诊断 DPI 96/144/192。旧探针固定等待 12 个图标，首次扩展检查在保留无结果查询后超时；已按该固定样本的实际 12/0 项结果修正等待条件，并让空查询键盘场景显式设置空文本，保留失败证据。

原始证据存放忽略目录 `runtime/probe-query-recall/`，另有 `runtime/probe-controlled/`、`runtime/probe-input-session/` 和 `runtime/flicker/probe-fixed/` 的完整场景记录。本轮是输入行为验收，不据这些功能回归的采样作前后性能比较，也不重述既有搜索基准为窗口延迟承诺。Windows 10、32 位、其他输入法与极低配实机仍待验证。

## 运行文件

`target/release/picorun.exe`，Rust 1.95.0 / MSVC，静态 CRT，release `opt-level=3`、fat LTO、单 codegen unit、panic abort、strip。构建后的程序已恢复原启动参数和输入框状态；验证不修改用户图标、主题、英文输入或自启动设置。
