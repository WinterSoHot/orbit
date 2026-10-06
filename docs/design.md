# Orbit — Tauri 个人 Agent 工作台

用户已选择独立桌面 App，并要求用 Tauri 实现此前讨论的工作台。本次实现 macOS 本地原型。

## 范围

- Tauri 2、React、TypeScript、Rust；任务列表、只读 Agent 关系图、待介入、活动流、交付物。
- 演示模式可运行、审批、补充指令、中断、查看纯文本交付，无模型调用。
- Codex 模式由用户显式启动，Rust 直接调用已安装的 `codex app-server`。固定 provider read-only sandbox；无 API key 输入、无权限升级、无自动重试。
- 真实运行最多一个活跃根任务。独立进程，初始化、启动、状态、文本、审批、steer、interrupt；子代理仅展示有协议证据的关系。
- JSON 原子快照替代 SQLite。Rust 拥有真实状态，前端仅保存 demo 状态；启动恢复使旧运行和审批失效并显示需核对。
- 真实 turn 完成产生纯文本对话结果，不等同业务验收。关闭 App 清理自己启动的进程组，不承诺所有外部副作用已停止。
- 本机原型依赖用户安装并认证的 Codex CLI；商业认证、跨平台安装、后台运行、自动恢复、任意文件打开暂不实现。

## 约束

真实运行/审批用 taskId、runId、turnId、requestId 关联。审批由 Rust 原子消费，不接受前端提供的 provider payload。取消失效在途审批；terminal 完成不能被迟到事件覆盖。前端不渲染不可信 HTML、不提供通用 shell/fs IPC。限制事件、文本及消息容量。

## 验收

前端构建；任务状态/审批/恢复的自动检查；Rust 协议检查；本机 Codex 无推理握手；浏览器交互和布局检查；尝试生成并启动 macOS .app。真实推理不会自动运行。

## Architect 规则 A 审查已处理

采用 Rust 单一真实状态、只读 sandbox、审批原子消费、独立取消/未知状态、stdout/stderr 持续消费、JSON 单写入者、只读 SVG、单真实根任务限制。协议以本机 Codex 0.150.1 导出的 schema 核验。
