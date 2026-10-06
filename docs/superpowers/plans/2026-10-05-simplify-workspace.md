# Compact Workspace Implementation Plan
> 执行方式：主会话原地实施，architect 只读审查。

**Goal:** 精简顶部与常驻三栏交互。
**Architecture:** 仅修改 App、TaskWorkspace、TaskChat 和作用域 CSS，复用现有操作回调和权限。
**Tech Stack:** React、TypeScript、原生 details、现有 Lucide。
**Spec:** ../specs/2026-10-05-simplify-workspace-design.md

## Constraints / Review Focus
只精简任务页顶部。菜单操作保留 disabled/capability；筛选和折叠不丢草稿或切换任务；claimed 核对撤销仍可达；compact 不影响归档聊天；窄屏、长标题和键盘焦点可用。无后端变更或新依赖。规则 A 候选审查已通过。

- [x] App：侧栏唯一新建入口、任务页去重复顶部；场景筛选移到看板。
- [x] TaskWorkspace：平齐标题栏、状态主操作、更多菜单、按需详情、看板折叠、精简卡片。
- [x] TaskChat：可选 compact 去重复标题；CSS 宽对话与屏内滚动、小屏纵排。
- [x] npm test/build、隔离页面视觉与操作检查；任一失败触发规则 B。
- [x] architect C 实际 Diff 复审、打包，报告实测限制。

验证：22项Node测试、SSR、生产构建、最新debug Orbit.app打包通过，architect C最终复审通过。浏览器检查菜单/Agent详情、折叠草稿、空页恢复看板、展开/收起搜索快捷键及新建弹层焦点保护。778px实测纵排，620px环境未达到；文档/Agent弹层此次仅源码核对保护路径。后端未变更，未调用真实模型。非阻塞：历史搜索请求在弹层关闭后可能重新聚焦搜索，后续可消费请求以严格恢复原焦点。
