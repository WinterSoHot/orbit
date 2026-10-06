# Knowledge library reading layout

architect A通过。保留苹果风格与Light/Dark、Zotero三栏；收敛滚动职责与正文层级。

- [x] 桌面外壳高度链，去重复标题；分类/列表各自滚动，右侧标题/工具栏固定。
- [x] 新增knowledge-content，正文/管理表单/版本/链接进入阅读区，保存栏移出并取消sticky；不改保存与生命周期逻辑。
- [x] 真实组件隔离长Markdown/编辑/链接/保存失败/PDF/两主题验证。
- [x] npm test/build、architect C、debug包。


2026-10-06：23 Node检查及SSR、tsc/Vite通过，debug Orbit.app成功，architect C与标准fixture增量复审通过。真实组件隔离测试涵盖35段长Markdown、编辑长文、显式保存、失败草稿保留及重试、链接插入、文档切换后保留正文；未修改存储协议或保存逻辑。

标准模式CSS1Compat宽1280×720：页面720/720，正文bottom650.6015625=保存栏top，保存栏bottom703；文档链接在正文末尾。阅读只有knowledge-content纵向滚动；编辑长文只有textarea滚动。PDF两页渲染正常，reader379=body379，page视图257，白底原色保留，展开阅读656高度正常；Light布局一致。实际窄778无横向溢出（页面scrollWidth/clientWidth均763），编辑器与保存栏恢复自然流。

边界：只读浏览器预览与内存IPC fixture，不调用模型、不写真实Store，桌面WebView未人工复测。截图 /private/tmp/orbit-library-layout.png。
