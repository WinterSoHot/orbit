# 执行器统一架构

用户需求：Codex 和 Qoder 是新建任务时可选择的执行器，后续可以扩展其他执行器。

共享 Runtime 负责创建、分发、单根任务准入、继续交付、持久化、编辑、归档及删除。Executor trait 只负责执行器协议、运行控制、协作读取和自有进程生命周期。前端从后端执行器目录读取名称、说明及能力，不按 Codex 判断按钮是否可用。协议输出转换为已有 Task 快照，复用 runId/revision 防止旧快照覆盖新运行。

首版注册 Codex App Server 和 Qoder ACP v1 两个实现，不建设动态插件加载系统。新增执行器实现同一 trait，并加入目录；业务 UI 与存储操作无需增加 provider 分支。

## 契约与范围

- 每次启动拥有独立本地 runId 和自有进程。全局最多一个根任务。共享锁只覆盖准入、启动登记和存储提交，不覆盖初始化、模型响应、审批等待或进程退出等待。中断和答复不取启动锁。
- Codex 保留现有 threadId、turnId、恢复 anchor、子会话读取和精确 active-writer 分支恢复。Task 增加可选 sessionRef 与 capabilities；旧记录缺失 provider 时默认 Codex。任务 provider 不可变且必须匹配 sessionRef.provider。未知 provider 的合法记录仍可查看、导出、归档及删除，执行明确拒绝。
- Qoder 启动本机 `qodercli --acp` 或 `qoder --acp`，复用本机登录状态，不使用 bypass/yolo。首次 initialize 使用 protocolVersion 1，只接受返回版本 1。clientCapabilities 仅声明限定工作目录的文件读取，禁用写入和终端。工具权限请求自动拒绝并记录；这是客户端权限限制，不是 OS 文件沙箱。
- `session/new` 使用绝对 cwd 与空 mcpServers；只接收当前 sessionId 的 session/update。文本流持久化为对话，成功 end_turn 后只解析独立的显式交付声明，工具事件进入运行记录，不将普通工具伪造成子 Agent。
- 每轮结束后清理自有进程。再次补充时重新 initialize，仅本次 loadSession=true 才执行 session/load；使用原 sessionRef.cwd，加载重放不计作新输出，但仍回复权限和处理协议错误。恢复失败不创建替代会话。
- Qoder 初版不支持运行中 steer、fork 和子 Agent 历史同步，UI 按能力关闭并说明。已确认能力用于历史展示，不能替代新握手。Qoder 本地 turnId 仅是本轮归属 token，不冒充协议提供的 turn。
- cancel 是通知，等待原 prompt 响应确认。取消超时或断管标记结果需核对，仅停止本应用拥有的进程。end_turn 成功，cancelled 中断，其他 stopReason 保存原值并标记失败；异常退出为 unknown。所有输出、会话字段和文件读取都有大小限制。文件读取绑定启动时打开并核对任务归属的目录 FD，根路径被替换后不会重新授权；取消超时清理在状态写盘前安排。

## 官方依据

- https://docs.qoder.com/cli/acp
- https://docs.qoder.com/cli/sdk/input-modes
- https://docs.qoder.com/cli/sdk/references-typescript
- https://github.com/agentclientprotocol/agent-client-protocol/blob/main/schema/v1/schema.json

architect 已按规则 A 审查上述边界。当前环境未发现 Qoder 可执行文件，协议模拟能验证客户端逻辑，真实握手和模型运行不作完成承诺。

新增执行器同时提供 delivery::instruction 输入契约和当前根轮次完整最终 Candidate；共享 Store 校验声明并原子保存 typed Artifact/Receipt。执行器不得直接创建交付，普通回复不转 Markdown，手动副本不计入当前轮验收。
