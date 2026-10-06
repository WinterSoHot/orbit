# Other pages polish

architect A通过。保持苹果风格、Light/Dark和现有操作边界；优化归档、待介入、交付物、设置，任务/知识库不改。

- [x] 辅助页高度链与紧凑标题，去重复全局统计；卡片和设置分组层级。
- [x] 归档交付优先，运行证据原生折叠；清单/详情独立滚动，取消正文嵌套滚动。
- [x] 四页非空/空态与两主题/窄屏验证，保留原有操作。
- [x] npm test/build、architect C、debug包。

验证：23 项 Node 测试及现有 SSR 检查通过，TypeScript/Vite 构建通过。1280×720 验证四页 Light/Dark 与空状态；778 像素窄窗口验证归档搜索/选择及待介入表单可达，无横向溢出。浏览器使用隔离测试数据，未执行真实 Store、CLI、导出或删除操作。

architect C 最终 Diff 审查通过，无阻塞问题；窄屏搜索无匹配时的额外提示作为非阻塞建议暂缓。Tauri debug 构建退出码 0，生成 src-tauri/target/debug/bundle/macos/Orbit.app。
