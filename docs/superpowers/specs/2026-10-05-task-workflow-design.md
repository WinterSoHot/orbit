# Orbit 任务看板、对话与队列

用户已选择本轮范围：常驻任务对话、四列看板、人工验收、真实等待队列。保留苹果风格、Codex/Qoder 选择与知识库；暂不实现跨任务 Chief 自动分配。

左侧对话始终呈现初始目标、补充记录、交付版本、执行中的正文与审批。右侧看板包含待办、进行中、待介入、已完成；卡片选择关联同一任务对话。拖动和按钮走同一服务端命令，不伪造执行状态。Agent 与完整日志在详情标签中查看。

Store 持久记录每任务至多一个 QueueRequest，包含 requestId、FIFO order、nextRunId、pending/claimed、Start 或 Continue 来源锚与文本。单 dispatcher 和所有运行入口共用 lifecycle 锁，只运行一个根任务。锁序 lifecycle→queue control→Store；持 Store 锁时不调用执行器。入队/执行前均校验来源锚，claim 与 launch 使用同一 nextRunId。成功启动后按 requestId+nextRunId 清理 claimed；任何启动、写入或失效来源错误均保留请求、暂停队列，不自动重试。暂停优先于调度。失败/中断暂停后续任务。重启 pending 默认暂停；claimed 禁止重试，确认所有执行器 idle 后用户可撤销请求，再明确重新发起。

验收以 CAS revision + runId + turnId 校验，Acceptance 绑定准确 run/turn/artifactIds。begin_run 和任何正文编辑清验收；终态执行器回写保留交付正文并拒绝不同 turn/run；平台 queue/accept 字段在同一次 Store 临界区合并。归档要求已验收且无排队请求，已有旧归档仍可读取/导出/删除。

列映射优先级：有效验收且无请求→已完成；审批、失败/中断/unknown、claimed、队首错误→待介入；pending 或 active→进行中；未启动→待办；其余无验收终态→待介入。等待时保留草稿并禁发送；运行补充只在执行器确认 steer 能力时开放，终态补充仅在可恢复会话时开放。

存储写 v2，读 v1/v2。旧程序明确拒绝非 v1 并禁止覆盖源文件，因此不支持降级写入。保持 8 MiB/50 任务/10 份交付上限，无新依赖。

视觉采用 SF 系统字体，#f5f5f7 背景、#fff 面板、#1d1d1f 正文、#86868b 次级、#007aff 操作、#e5e5ea 分隔；对话与看板平行、左对齐，窄屏切为纵向，提供折叠对话、明显键盘焦点和拖动的按钮替代。

验证覆盖 FIFO/CAS、claim 崩溃和清理失败、旧运行回写、验收版本变更、重启暂停、跨执行器队列与能力差异。使用隔离数据，不运行用户真实模型任务；完整 npm/Rust 检查与独立 UI fixture。AGENTS A 已通过，任一测试失败执行 B，最终 Diff 执行 C。

规则 A 补充审查：平台 UI revision 与 executorRevision 分离。Store 分配对外 revision，actor 自行推进本地序号；平台 queue/accept 操作不推进 executor 水位。新 run claim 初始化水位，同 run save_task 保留水位；legacy None 保守使用 current revision。save_existing 接受新执行序号后发出 Store 版本，拒绝重复/倒序。两 adapter refresh 仅合并平台字段，不导入 UI 时钟或回滚执行 status/nodes/turn；稳定终态 artifacts 仅同 run/turn 合并。Codex 分支持久化与 steer 的本地快照恢复 own clock，向 UI 发 Store 版本。平台清请求和快速 completion 交错回归已覆盖。
