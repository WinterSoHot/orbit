# Workspace scroll layout

沿用系统字体、蓝色强调和 Light/Dark 配色；本轮设计重点是滚动层级，不增加装饰或依赖。用户授权优化当前多滚动条布局。

候选：桌面对话/看板各一个纵向滚动区，外层不滚动，输入固定；看板按可用宽度四/两/一列；右侧详情替代看板，不再上下堆叠。图仅在确有宽度溢出时横向滚动。窄屏自然页面滚动。

- [x] architect A 与布局实现。
- [x] 隔离浏览器真实长消息/多任务/Agent详情，键盘返回、Light/Dark与宽窄布局。
- [x] npm test/build、architect C、debug App包。


验证：23 Node检查与SSR、tsc/Vite构建通过；最终修正后debug Orbit.app打包成功，architect C复审通过（修复详情模式下搜索快捷键回归）。

桌面改用fixed inset高度约束，避免重复动态视口高度；page-content解除auto margin横向收缩。真实浏览器App壳1280×720无页面溢出；778×901自然页面下宽度正确、空任务页无额外滚动。隔离layout+dense样本29任务/4Agent/长正文：宽1280页面720/720，board clientWidth=scrollWidth689，只有chat-history和board纵向滚动；长Agent详情时仅chat-history与context-body滚动，输入bottom720。窄778页面自然高度3867，内部无纵向滚动，看板763=scrollWidth。IAB760视口请求未应用，因此没有声明760实测。

原生拖放待办任务到进行中成功排队；右键菜单目标正确。交付/Agent/日志三个详情模式均可super+k返回看板并聚焦搜索。返回按钮恢复选中卡焦点。Light/Dark详情布局一致。未调用模型、未修改真实任务数据；仅浏览器隔离与App壳验证，桌面WebView未人工视觉复测。关系图/代码块/表格仍保留必要横向滚动。截图 /private/tmp/orbit-scroll-details.png。
