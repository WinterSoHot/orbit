# 本地资料库操作 Implementation Plan

**Goal:** 提供分类管理、回收站、文档操作与存储位置入口。
**Architecture:** 复用 LibraryStore 原子JSON事务与CAS；原PDF目录保留。前端动作先固定同步guard、再flush，然后调用后端并重建外部更新后的编辑session。
**Tech Stack:** Tauri 2 / Rust / React / TypeScript，现有依赖。
**Spec:** docs/knowledge-delete.md

## Global Constraints
100文档/100分类/8层/16MiB元数据/64MiB附件；回收站占容量；不改源文件/任务交付。浏览器只读。无新数据库依赖。

## Review Focus
- 未知/损坏schema不能空库覆盖。
- 分类删除递归移除关联，保留资料且更新CAS。
- 回收站草稿禁止autosave与beforeunload写入。
- 永久删除失败/迟到保存不能使资料复活。
- 全量导出含回收站和分类，附件缺失必须整体失败。

### 1. 本地模型与事务
Files: src-tauri/src/knowledge.rs，src-tauri/src/store.rs（导出测试）
- [x] 写失败测试：trash/restore/purge CAS、附件源保留、持久化失败、分类多关联和子树删除。
- [x] 运行cargo test（desktop默认features）；规则B审查red结果。
- [x] Document增加collection_ids/deleted_at，Library增加collections/schema_version；Disk显式V1/V2兼容与验证。
- [x] save_collection(Collection)->Collection、delete_collection(id,revision)->Library、organize(id,revision,ids)->Document、trash/restore(id,revision)->Document、purge(id,revision)->Option<String>。
- [x] 数据持久化成功后才清理私有PDF；目录sync失败保留附件并返回警告。
- [x] 运行知识库与完整Rust测试。

### 2. 桌面桥接和三栏界面
Files: src-tauri/src/lib.rs，src/bridge.ts，src/knowledge.ts，src/KnowledgeLibrary.tsx，src/KnowledgeEditor.tsx，src/library.css
- [x] 接入上述commands；get_library_directory与open_library_location(document_id?,reveal)由后端解析固定安全路径。
- [x] 分类树创建/重命名/父分类/删除、未分类与回收站；文档加入/移出分类，移入回收站/恢复/永久删除确认。
- [x] EditorHandle.flush返回最新document；同步mutation guard和只读模式；迟到保存仅更新已有且较新记录，创建独立插入。
- [x] 分类变更刷新库并重建session；恢复/回收站重建session。
- [x] 添加SSR/Node边界测试，受控UI验证实际按钮与成功/失败路径。

### 3. 交付
- [x] npm test + npm run build + cargo test，失败遵循规则B。
- [x] 规则C审查最终增量；处理阻塞后复审。
- [x] 清理临时UI入口完成；macOS Orbit.app打包成功（73.50 MiB，/private/tmp/orbit-library-bundle.log）。
