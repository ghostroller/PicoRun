# 第三方数据

`assets/pinyin.bin` 是 [mozillazg/pinyin-data](https://github.com/mozillazg/pinyin-data) 的单字首读音、去声调、去重音节版本。

Copyright (c) 2016 mozillazg，MIT 许可。完整许可位于 `third_party/pinyin-data/LICENSE`，随仓库保留；以后发布二进制时也应附带该许可。

本项目当前未复制 ALTRun、WindMenu 或其性能原型的实现代码。它们的研究报告仅作架构和测量参考。项目整体的发布许可证尚未选定，不能将本数据的 MIT 许可自动套用到整个项目。

## Windows 安装工具

安装包由 [Inno Setup](https://jrsoftware.org/) 编译。简体中文安装界面使用其官方源码仓库 `is-6_7_3` 标签下的用户贡献翻译，作者信息按原文件保留：[ChineseSimplified.isl](https://github.com/jrsoftware/issrc/blob/is-6_7_3/Files/Languages/Unofficial/ChineseSimplified.isl)。原样许可条件见 `third_party/inno-setup/LICENSE`。这些许可只适用于 Inno Setup 工具与相关文件，不改变 PicoRun 整体许可证；编译器不随 PicoRun 安装或 ZIP 分发。

