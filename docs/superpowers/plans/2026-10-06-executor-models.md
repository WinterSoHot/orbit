# Executor model selection Implementation Plan

Goal: 分提供者展示本机模型目录，在新任务选择并持久化模型。
Architecture: 现有 Executor 增加统一 models 接口；Codex app-server model/list 与 Qoder --list-models 返回统一模型条目，UI 共用选择面板。Task 保存不可变 requestedModel，旧记录 None。
Tech stack: Rust/serde，React，现有 Tauri IPC；不增加依赖。

- [x] 目录与任务：解析真实目录，去重/隐藏过滤/大小限制；Task 模型验证及双保存入口不可变。Codex 在初始化后选择模型；Qoder 每轮 --model argv。旧记录兼容。
- [x] IPC 与查询：provider 路由，短命进程，无会话和推理；统一截止时间、分页上限、输出上限和进程清理。
- [x] UI：顶部提供者切换，下方状态及模型行；每提供者默认选择保存本地。新建任务覆写，副本保留请求模型。失效选择明确显示并阻止提交；请求竞态按 provider 去重处理。
- [x] 验证：目录/任务持久化/协议传参测试，前端测试与构建，桌面编译，Light/Dark UI 检查；architect 最终 Diff 审查。

Constraints: 不硬编码型号；请求模型与实际返回模型分开；设置不影响已有任务。Codex Run.requested_model 初始保持 None。所有失败清理 OwnedChild。AGENTS A 已审查，B 对预期 RED 同样适用，C 完工审查。

Validation: 前端29项+SSR通过；Rust完整116项通过，6项ignored（其中目录只读验证单独执行通过）；强化不可变定向测试通过；Codex/Qoder实际目录读取通过，无推理。Tauri debug Orbit.app构建通过。Light/Dark和提供者/模型交互检查通过；architect C通过。
