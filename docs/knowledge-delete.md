# 本地资料库：分类、回收站和文档操作

用户选择参考 Zotero 本地资料库与常用操作。沿用独立 Orbit 知识库，不连接 Zotero 数据库，不增加引用管理。

- 原子 JSON 元数据在下一次成功写入时升级为 schema 2：分类树、文档多分类、删除时间。V1 显式兼容；损坏/未知版本拒绝写入。现有 UUID、正文、历史和附件不迁移物理位置。
- 保留 AppData/library.json + library-blobs/<UUID>.pdf（库内副本），显示实际数据目录；分类不对应物理目录。同一资料可属于多个分类。
- 分类最多100个、8层；删除分类连同子分类一次事务移除关联，不删除资料。受影响文档revision递增。
- 全部资料、未分类、分类、类型、智能分组、标签默认排除回收站。回收站只读，可阅读、导出；恢复后可编辑。普通删除先保存当前内容后移入回收站；永久删除仅在回收站，并确认。
- 永久删除先提交元数据再清理库内附件；清理失败报告警告，不恢复已删除记录。不修改导入源、任务交付、其他资料的内部链接。
- 现有重命名、标签、Markdown 编辑与版本恢复、链接和导出保留；补充分类操作、PDF 默认应用打开及 Finder 显示、资料库目录打开。仅接受已验证库内固定路径，不提供任意命令/路径接口。
- 100篇文档、16MiB元数据、64MiB附件容量包含回收站。完整工作台导出包含所有分类、回收站和PDF；并发永久删除导致快照缺附件时导出整体失败。
- 删除guard同步关闭autosave/beforeunload；flush返回session最新版本用于CAS；创建插入与迟到保存更新分开。外部分类修改后重建当前编辑session。

规则 A：knowledge_architect 2026-10-04 审查无架构否决，采纳保留物理目录、显式schema、分类事务、回收站只读与会话重建要求。
参考：https://www.zotero.org/support/collections_and_tags ，https://www.zotero.org/support/zotero_data ，https://www.zotero.org/support/attaching_files 。Orbit保留JSON格式，并非Zotero数据库格式兼容。

## 验证记录（2026-10-04）
- Rust 默认desktopfeatures完整测试：92 passed，5 ignored（现有真实执行器 smoke tests）。
- Node 16 passed，SSR验证回收站只读/浏览器修改禁用；TypeScript+Vite生产构建通过。
- 实际组件受控内存UI：回收站带草稿阅读、导出、模拟beforeunload，writes=0；恢复后编辑成功；模拟trash失败保留正文；两分类计数与关联有效；分类子树删除保留资料与剩余关联、重建后CAS编辑成功；trash从active移除；purge确认取消无调用。
- 后端真实临时文件测试：purge CAS/元数据失败不删附件、提交后清理失败返回warning、源文件保留、旧导出快照缺附件失败、完整导出含回收站/分类/schema。
- 规则C：knowledge_architect最终增量审查通过，无阻塞；用户真实资料库未被用于删除测试。
- 限制：PDF系统阅读使用临时副本；成功打开的副本暂无应用级清理，永久删除仅清理库内记录和附件，不清除系统临时阅读副本。存储目录当前固定在AppData，提供查看/打开入口；不是Zotero数据库格式兼容。

- 桌面打包成功：src-tauri/target/debug/bundle/macos/Orbit.app（73.50 MiB）。验证日志：/private/tmp/orbit-library-full-rust.log、orbit-library-node.log、orbit-library-build-verified.log、orbit-library-bundle.log。
