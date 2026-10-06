# Repair observed multi-Agent display

Architect A reviewed the candidate. Root cause: installed CLI 0.150.1 emits subAgentActivity, while Orbit only handles legacy collabAgentToolCall/thread-start parent fields. Read-only thread/read confirms three real children and twelve activity items for the user knowledge-base task.

1. Project actual child IDs, paths and explicit activity kinds; unify IDs with legacy nodes, infer only exact known path parents, resolve parents after collection, bound nodes/paths, deduplicate event IDs, keep interacted from regressing a completed child. Unknown parents remain independent. Add a real-schema RED regression before production edits.
2. Add read-only collaboration sync for a terminal real task. A bounded OwnedChild handshake + thread/read (never start/resume/turn) fetches history. Select exact thread/turn, rebuild only collaboration fields, merge into latest Store task under lifecycle protection, persist then commit and synchronize its closed Run. Keep root output/status and edited artifacts. Identical sync is a no-op. Errors keep prior graph; bounded stdout/stderr, timeout and process-group cleanup apply throughout.
3. Add one desktop button; browser remains demo-only. Verify actual recorded history without model calls, Store write failure/identity checks, runtime lifecycle, UI graph projection, regressions, App bundle and final architect C. Native UI automation remains unavailable; do not claim native button clicks were tested. Existing App must be reopened to use new bundle.

No new dependency, model rerun, unrelated feature, commit or worktree.
