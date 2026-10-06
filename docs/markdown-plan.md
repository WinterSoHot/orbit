# Markdown preview and editing

Scope authorized by user: open existing Markdown deliverables in the workbench, render preview, edit source, save locally, and export the saved revision. One editor component; unchanged task DTO. Dialog close/backdrop/Escape share unsaved-change confirmation; native App termination is outside this dialog confirmation scope.

Architect A reviewed the candidate. Required decisions incorporated: serialized explicit saves with propagated errors; strict revision admission for demo snapshots; Runtime lock order starting → runs → Run → Store; CAS with exact unique artifact ownership and terminal task; clone/persist/commit; drafts separate from confirmed saved content.

1. Store and Runtime: implement edit_artifact(id, expected_content, content) returning Task. Limit UTF-8 content to 256000 bytes, allow empty content, reject missing/ambiguous/stale/in-flight edits. Persist atomically before changing memory; synchronize closed Run snapshot. Add tests for stale CAS, ambiguous ID, failed write, late same-revision demo save, and shutdown retaining edits.
2. UI: react-markdown + remark-gfm, skip HTML, suppress images, permit only http/https/hash links. Open deliverable preview; editor textarea with explicit save; queue operations against latest tasks, success-only baseline update; dirty-dialog close confirmation and saved-version export. Browser/demo revision guards prevent queued snapshots overwriting edits.
3. Verify rendering and edit/save/reopen through browser UI, full regression, Tauri package, final architect C diff review, then open the result for the user. No model requests needed.

Repo ruling preserved: parent unborn HEAD and unrelated files, no commits/worktree.
