# 拼音字典

来源：[mozillazg/pinyin-data](https://github.com/mozillazg/pinyin-data)，MIT，Copyright (c) 2016 mozillazg。项目仓库保留生成后的紧凑数据与许可；上游原始文本只在构建时使用，运行时不需要 Python。

此版本原始 `pinyin.txt` 的 SHA256：

```text
621f8ca9eff8519f47e2b17b564fd318161e13bca07eea8c8e04993cd5d3b52e
```

44,435 条 Unicode 字符记录；每字仅首读音，去声调、`ü/ǖ/ǘ/ǚ/ǜ` 转 `v`。小型词组覆盖在 `src/pinyin.rs` 单独维护，不能声称通用多音字消歧。

## PRPY v1 格式

- 12 字节头：`PRPY` 魔数、u32 小端版本 1、u32 小端记录数。
- 记录按码点升序，每条 6 字节：u32 小端码点、u16 小端音节池偏移。
- 记录后为去重后的 ASCII 小写音节池，以 NUL 分隔。
- 编译期 `include_bytes!` 保存为只读静态数据；索引构建时二分查读音，无需逐字符 HashMap 或堆对象。

生成器检查重复字符、非法音节和池大小，测试检查全部记录及偏移。此文件只嵌入受控仓库资源，不用来接收外部未验证字典。

持有与上述哈希一致的原始文本时，运行：

```powershell
python tools/build_pinyin.py path/to/pinyin.txt assets/pinyin.bin
```

许可见 [../third_party/pinyin-data/LICENSE](../third_party/pinyin-data/LICENSE)。

