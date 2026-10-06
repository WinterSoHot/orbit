# Orbit 独立知识库

用户已确认：参考 DEVONthink 的功能，在 Orbit 内建立独立知识库，不连接 DEVONthink 数据库。继续使用本地 Tauri App 和苹果风格界面。

## 功能与界面

新增“知识库”主导航。知识库采用分组/标签、文档列表、阅读编辑区三栏。支持 PDF、Markdown 本机导入；创建 Markdown；添加网页 URL 并提取静态正文，或自行输入离线正文。网页保留来源及抓取时间，未抓到正文明确失败，不执行原 HTML/脚本、不加载远端图片。交付文档可显式收藏为独立 Markdown 副本，后续编辑不修改原任务。

标签可添加多项并编辑。智能分组持久化名称及 AND 条件：类型、单个精确标签、标题/正文关键词、最近 N 天更新。空条件匹配全部；N 天按当前时间向前 N×24 小时计算。PDF 未做 OCR/全文提取，关键词只匹配标题、标签与备注。PDF 本地逐页阅读/缩放，不运行 PDF JavaScript。

稳定文档链接为 `orbit://document/<UUID>`，可复制为 Markdown 链接、在文档预览中点击切换；显示文本正文中的入链/出链。链接按 ID 解析，重命名不失效，缺失目标明确提示。首版链接仅在 Orbit 文档预览中解析，不注册系统级 URL scheme。

Markdown/网页正文支持预览与编辑。编辑后约 600ms 持久化草稿、约 2s 自动提交正文；手动保存支持 ⌘/Ctrl S。自动历史按 30 秒分桶保留先前正文，手动修改和恢复始终保留先前内容，正文不变不新增历史。每文档最多 20 个独立内容快照。历史面板可预览、恢复；恢复先备份当前可见文本，备份写盘失败则拒绝恢复。重开自动呈现已成功持久化的草稿并标明恢复状态；崩溃前尚未写盘的输入不作保证。

## 数据与边界

`knowledge.rs` 负责 LibraryStore，与执行器协议独立。AppData 下 `library.json` 保存 UUID 文档、类型、标题、标签、来源 URL、正文、revision、草稿、版本、时间及分组规则。不可变 PDF 存于私有 `library-blobs/<UUID>.pdf`；不在打字时重写附件。单 PDF 20MiB、所有 PDF 共 64MiB、单正文/草稿/版本 512KiB、文档 100 篇、元数据文件 16MiB。数量上限不保证能同时容纳全部最大文档。每次变更按最终实际 JSON UTF-8 字节预算校验。

本机文件仅通过原生 picker 取得；有界读取并拒绝非普通文件。私有 PDF 由目录 FD 和 UUID 文件名定位，NOFOLLOW/NONBLOCK，读取检查大小与 PDF 头。网络/解析/文件读取放在锁外。网络仅 HTTP(S)，不带用户凭据，总超时 20s、重定向最多 5 次、解压后正文最多 4MiB。静态正文抓取不是完整网页归档，不覆盖登录后/脚本渲染页面。

候选 Library 在同目录新临时文件中写入、sync、原子 rename 成功后才提交内存。只有 NotFound 初始化；读盘失败、坏格式、不支持版本和超限进入不可写状态并保留原文件。新 PDF 写成功而元数据失败时尽力清理新增文件，绝不修改导入源。原数据错误在界面可见。

所有内容操作经过同一每文档队列；绑定 documentId、编辑 session UUID、递增 sequence、CAS revision。旧草稿/旧响应不得覆盖提交、恢复或更新的本地文本。恢复设置队列 epoch fence、清理定时器、暂停输入；发生冲突保留本地文本并停止自动重试。切换文档和离开知识库先 flush 成功；失败则保留当前编辑器。退出时尽力 flush，意外退出仅保证已持久草稿。

单文档可导出原 PDF 或已保存 Markdown。设置“导出全部数据”扩展为 schema version 2：工作台记录 + 知识库元数据/规则/正文/已存草稿/历史 + PDF base64 内容。workspace → library 固定顺序复制一致快照后释放锁，附件读取及序列化/写盘不持锁。源知识库损坏或 PDF 无法读取时完整导出失败，不偷偷遗漏。没有导入恢复功能，不称为已验证可恢复备份。

## 范围与依据

本轮完成本地管理、静态网页正文、PDF 阅读、Markdown 编辑/历史、规则筛选、内部链接和完整导出。OCR、云同步、DEVONthink 数据库连接、AI 分类与系统级深链接不在本次范围。

参考：[DEVONthink 文档链接与智能组织](https://www.devontechnologies.com/apps/devonthink/linking)、[Mozilla PDF.js](https://mozilla.github.io/pdf.js/examples/)、[reqwest blocking](https://docs.rs/reqwest/latest/reqwest/blocking/struct.ClientBuilder.html)。Agent Reach 的 Exa 后端未配置，官方资料通过搜索工具补充；Jina 路由尝试记录在临时目录，不改变账号或安装后端。


## 验证补充

- PDF.js 6.4.299 在本地 worker 中解析，单次预览画布每边不超过 4096、总像素不超过 800 万，浮点缩放预留向上取整余量。单图片超过 800 万像素可能省略，界面明确提示可导出原件查看。
- 本轮仅在 macOS Intel 打包验证；同一 Runtime 的操作共享 Store/LibraryStore，CAS 和队列保护同一实例的写入。尚未验证多个独立 Orbit 进程同时写同一 AppData，或跨操作系统分发。
- 没有文档/分组删除与导入完整导出文件功能。内容管理、标签与规则、内部链接、编辑/历史是本轮范围。

- 编辑器的保存判断查询队列最新正文、草稿和在途计数；撤回到已提交正文仍会清理旧草稿。草稿响应不重新计时自动提交。知识库操作从首次 flush 到抓取/导入/切换结束全程锁定旧编辑器，失败解锁；PDF 单页渲染失败后可继续尝试其他页面。
