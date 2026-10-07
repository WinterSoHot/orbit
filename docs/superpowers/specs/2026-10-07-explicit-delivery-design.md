# 显式交付设计

用户已批准：取消最终回复自动保存 MD，独立提交交付并校验，保留手动保存消息。规则 A 已由 task_knowledge_architect 审查，以下边界已纳入。

- 普通回复只保存在对话；成功结束不代表提交或验收。
- 共用提交契约：完整权威根最终输出去首尾空白后必须恰好是一个 orbit-delivery fenced JSON，schemaVersion=1、submissionId、items。无模糊提取，无 Markdown 回退。Codex 仅使用当前根 turn 唯一明确 final_answer completed 项；phase 缺失、多候选、截断、子 Agent、迟到、失败及中断均不提交。Qoder 仅用成功 end_turn 的完整根响应。
- 使用 serde 强类型且拒绝未知／重复字段；整包最多 256000 UTF-8 字节，1–10 条，任务总交付不超过 10。submissionId 为 1–64 个 ASCII 字母、数字、下划线或连字符。name 1–120 字符无换行；markdown content 非空且不超过 256000 字节；link url 最多4096字节并通过 URL 解析，仅HTTP/HTTPS，不自动访问；result summary 最多10000字符、evidence 最多10条每条2000字符。路径／文件及其他类型整包拒绝。沿用64k输出及8MiB存储上限，不扩大执行权限。
- Store 拥有整包提交、绑定run/turn/thread/item、来源和幂等。相同身份(task/run/turn/submissionId)与相同规范化原提交返回既有结果，冲突拒绝；一次写盘包括全部成果及提交收据，失败零部分写入。收据记录原始规范化包是不可改写的提交快照，当前文档允许用户另行编辑；非文档结果保持单份规范化内容，由此派生展示/导出。
- 交付类型 markdown、link、result，UI不宣称链接已访问或结果已独立验证。类型与名称不用作文件路径；导出与回存派生Markdown并附来源快照。已有普通MD交付保留。
- Qoder 根输出每run保存稳定本地assistant消息，thread使用session身份且有容量/截断标志。完成消息关联其当时的来源；旧消息无关联则空。
- 手动保存只能由Rust按task/revision/run/thread/item读取真实持久化完成assistant正文；拒绝活动、排队、归档、空或截断消息，manual副本不作为执行器已提交交付参与验收。
- 验收只针对当前run/turn的executor提交成果；旧版本/手动副本不充数。旧历史验收保持，新轮清空验收；编辑交付使验收失效。无提交的结束轮显示“本轮未提交交付”。
- 工作空间与全部导出写schema4，兼容1–3读取；旧程序拒绝新格式。交付收据与消息来源防止迟到actor覆盖。
- 保留苹果风格、Light/Dark、现有折叠与打开入口；成功提交显示精简成果卡，原协议信息留在对话记录中便于核对。
