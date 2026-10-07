use crate::{
    model::Task,
    store::Store,
    team::{AssignmentDraft, PlanDraft},
};
fn fixture() -> (Store, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("orbit-team-flow-{}", uuid::Uuid::new_v4()));
    (Store::open(dir.clone()).unwrap(), dir)
}
fn plan(store: &Store) -> PlanDraft {
    let a = store.workspace().agents;
    PlanDraft {
        title: "共同方案".into(),
        goal: "比较并汇总方案".into(),
        criteria: "有依据且明确限制".into(),
        coordinator_id: a[0].id.clone(),
        reviewer_id: a[2].id.clone(),
        workers: vec![
            AssignmentDraft {
                agent_id: a[1].id.clone(),
                goal: "产品分析".into(),
                coding: false,
            },
            AssignmentDraft {
                agent_id: a[1].id.clone(),
                goal: "技术分析".into(),
                coding: false,
            },
        ],
        project: None,
    }
}
fn complete_next(store: &Store, text: &str) -> Task {
    let (mut t, _) = store.claim_next().unwrap().unwrap();
    let q = t.queue.clone().unwrap();
    let saved = store
        .finish_claim(&t.id, &q.request_id, &q.next_run_id, None)
        .unwrap();
    t.merge_platform(&saved);
    t.thread_id = Some(format!("thread-{}", t.id));
    t.turn_id = Some(uuid::Uuid::new_v4().to_string());
    t.status = "completed".into();
    t.delivery_candidate.capture("final", text);
    t.event("done", "system", "fixture");
    store.save_existing_task(t).unwrap().unwrap()
}
#[test]
fn confirmed_children_aggregate_then_require_bound_independent_review() {
    let (s, dir) = fixture();
    assert_eq!(s.workspace().agents.len(), 3);
    let parent = s.create_team(plan(&s)).unwrap();
    let v = parent.team.as_ref().unwrap().plan.version.clone();
    assert!(s.claim_next().unwrap().is_none());
    assert!(s
        .confirm_team(&parent.id, parent.revision, "wrong")
        .is_err());
    let confirmed = s.confirm_team(&parent.id, parent.revision, &v).unwrap();
    assert_eq!(confirmed.team.as_ref().unwrap().children.len(), 2);
    let one = complete_next(&s, &crate::delivery::fixture("产品结论"));
    assert!(one.parent_link.is_some());
    assert_eq!(s.task(&parent.id).unwrap().team.unwrap().phase, "work");
    complete_next(&s, &crate::delivery::fixture("技术结论"));
    let aggregating = s.task(&parent.id).unwrap();
    assert_eq!(aggregating.team.as_ref().unwrap().phase, "summary");
    assert!(aggregating.queue.is_some());
    let summary = complete_next(&s, &crate::delivery::fixture("最终方案"));
    assert_eq!(summary.id, parent.id);
    let p = s.task(&parent.id).unwrap();
    assert_eq!(p.team.as_ref().unwrap().phase, "review");
    assert!(s
        .accept_task(&p.id, p.revision, &p.run_id, &p.turn_id)
        .is_err());
    let review_id = p.team.as_ref().unwrap().review_task.clone().unwrap();
    let review = s.task(&review_id).unwrap();
    let input = review.team_input.as_ref().unwrap().version.clone();
    let packet = format!(
        "```orbit-review\n{}\n```",
        serde_json::json!({"schemaVersion":1,"submissionId":"review-1","inputVersion":input,"verdict":"pass","summary":"已检查提供的完整方案","findings":[]})
    );
    let finished = complete_next(&s, &packet);
    assert_eq!(finished.id, review_id);
    assert!(finished.review_submission.is_some());
    assert!(finished.artifacts.is_empty());
    let p = s.task(&parent.id).unwrap();
    assert_eq!(p.team.as_ref().unwrap().phase, "ready");
    assert!(s
        .accept_task(&p.id, p.revision, &p.run_id, &p.turn_id)
        .unwrap()
        .accepted());
    let id = p.current_delivery_ids()[0].clone();
    s.edit_artifact(&id, "最终方案", "更新方案").unwrap();
    assert!(!s.task(&parent.id).unwrap().accepted());
    assert_ne!(s.task(&parent.id).unwrap().team.unwrap().phase, "ready");
    let current = s.task(&parent.id).unwrap();
    s.revise_summary(&current.id, current.revision, "使用更新后的方案重新核验")
        .unwrap();
    complete_next(&s, &crate::delivery::fixture("新方案"));
    let current = s.task(&parent.id).unwrap();
    let reviewer = s
        .task(current.team.as_ref().unwrap().review_task.as_ref().unwrap())
        .unwrap();
    let version = &reviewer.team_input.as_ref().unwrap().version;
    let packet = format!(
        "```orbit-review\n{}\n```",
        serde_json::json!({"schemaVersion":1,"submissionId":"new-review","inputVersion":version,"verdict":"pass","summary":"更新方案通过","findings":[]})
    );
    complete_next(&s, &packet);
    let current = s.task(&parent.id).unwrap();
    s.accept_task(
        &current.id,
        current.revision,
        &current.run_id,
        &current.turn_id,
    )
    .unwrap();
    s.archive_task(&current.id).unwrap();
    let descendants: Vec<_> = s
        .workspace()
        .tasks
        .into_iter()
        .filter(|t| {
            t.parent_link
                .as_ref()
                .is_some_and(|l| l.parent_id == parent.id)
        })
        .collect();
    assert!(descendants.iter().all(|t| t.archived));
    let first = descendants
        .iter()
        .find(|t| !t.artifacts.is_empty())
        .unwrap();
    let a = &first.artifacts[0];
    assert!(s
        .edit_artifact(&a.id, &a.content, "changed after archive")
        .is_err());
    let reopened = Store::open(dir.clone()).unwrap();
    assert!(reopened.workspace().error.is_none());
    assert_eq!(reopened.workspace().tasks.len(), s.workspace().tasks.len());
    assert!(reopened.workspace().tasks.iter().all(|t| t.archived));
    assert!(reopened.export_workspace().is_ok());
    assert_eq!(reopened.workspace().agents.len(), 3);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn plan_creation_and_confirmation_are_atomic_and_profiles_are_frozen() {
    let (s, dir) = fixture();
    let p = s.create_team(plan(&s)).unwrap();
    let old = p.team.as_ref().unwrap().plan.workers[0].agent.clone();
    let mut edited = old.clone();
    edited.role = "更新职责".into();
    s.save_agent(edited.clone()).unwrap();
    assert_eq!(
        s.task(&p.id).unwrap().team.as_ref().unwrap().plan.workers[0].agent,
        old
    );
    std::fs::create_dir(dir.join("workspace.tmp")).unwrap();
    assert!(s
        .confirm_team(&p.id, p.revision, &p.team.as_ref().unwrap().plan.version)
        .is_err());
    assert_eq!(s.workspace().tasks.len(), 1);
    std::fs::remove_dir(dir.join("workspace.tmp")).unwrap();
    let p = s
        .confirm_team(&p.id, p.revision, &p.team.as_ref().unwrap().plan.version)
        .unwrap();
    let child = &p.team.as_ref().unwrap().children[0];
    assert!(s.delete_task(child).is_err());
    let cancelled = s.cancel_team(&p.id, p.revision).unwrap();
    assert_eq!(cancelled.team.unwrap().phase, "cancelled");
    assert!(s.claim_next().unwrap().is_none());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn review_rejects_forged_mixed_truncated_and_stale_packets_and_revision_keeps_goal() {
    let (s, dir) = fixture();
    let p = s.create_team(plan(&s)).unwrap();
    s.confirm_team(&p.id, p.revision, &p.team.as_ref().unwrap().plan.version)
        .unwrap();
    let child = s
        .task(&s.task(&p.id).unwrap().team.unwrap().children[0])
        .unwrap();
    assert!(child.execution_input().contains("有依据且明确限制"));
    complete_next(&s, &crate::delivery::fixture("产品"));
    complete_next(&s, &crate::delivery::fixture("技术"));
    complete_next(&s, &crate::delivery::fixture("汇总"));
    let parent = s.task(&p.id).unwrap();
    let mut review = s
        .task(parent.team.as_ref().unwrap().review_task.as_ref().unwrap())
        .unwrap();
    review.begin_run(true);
    review.turn_id = Some("turn".into());
    review.thread_id = Some("thread".into());
    let packet = |version: &str| {
        format!(
            "```orbit-review\n{}\n```",
            serde_json::json!({"schemaVersion":1,"submissionId":"review","inputVersion":version,"verdict":"changes","summary":"需要核验","findings":["补充证据"]})
        )
    };
    let good = packet(&review.team_input.as_ref().unwrap().version);
    for text in [
        packet("foreign"),
        format!("Explanation\n{good}"),
        good.replace("\"schemaVersion\":1", "\"schemaVersion\":1,\"extra\":true"),
        good.replace(
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"schemaVersion\":1",
        ),
    ] {
        let mut r = review.clone();
        r.delivery_candidate.capture("final", &text);
        assert!(!crate::team::commit_review(&mut r).unwrap_or(false));
        assert!(r.review_submission.is_none());
    }
    let mut forged = review.clone();
    forged.parent_link = None;
    forged.delivery_candidate.capture("final", &good);
    assert!(crate::team::commit_review(&mut forged).is_err());
    let mut truncated = review.clone();
    truncated.delivery_candidate.capture("final", &good);
    truncated.delivery_candidate.truncated = true;
    assert!(crate::team::commit_review(&mut truncated).is_err());
    let mut mixed = review.clone();
    mixed.delivery_candidate.capture("one", &good);
    mixed.delivery_candidate.capture("two", &good);
    assert!(crate::team::commit_review(&mut mixed).is_err());
    let done = complete_next(&s, &good);
    assert!(done.review_submission.is_some());
    let p = s.task(&p.id).unwrap();
    assert_eq!(p.team.as_ref().unwrap().phase, "revision");
    assert!(s
        .accept_task(&p.id, p.revision, &p.run_id, &p.turn_id)
        .is_err());
    let original = p.prompt.clone();
    let revised = s.revise_summary(&p.id, p.revision, "补充离线证据").unwrap();
    assert_eq!(revised.prompt, original);
    assert!(revised.execution_input().contains("补充离线证据"));
    assert!(revised.team.as_ref().unwrap().review.is_none());
    let restarted = crate::runtime::Runtime::new(Store::open(dir.clone()).unwrap());
    assert!(restarted.queue_state().paused);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn retry_keeps_snapshot_history_and_old_or_archived_plans_cannot_capture() {
    use std::{fs, path::Path};
    let (root, repo) = crate::coding_tests::repo();
    let store = Store::open(root.join("app")).unwrap();
    let project = crate::coding::preflight(&repo, "refs/heads/delivery").unwrap();
    let mut draft = plan(&store);
    draft.workers.truncate(1);
    draft.workers[0].coding = true;
    draft.project = Some(project.clone());
    let parent = store.create_team(draft).unwrap();
    store
        .confirm_team(
            &parent.id,
            parent.revision,
            &parent.team.as_ref().unwrap().plan.version,
        )
        .unwrap();
    let a = complete_next(&store, &crate::delivery::fixture("notes A"));
    let owned = root.join("owned");
    fs::create_dir(&owned).unwrap();
    let w = crate::coding::prepare(&owned, &a.id, &project).unwrap();
    store
        .prepared_code(&a.id, a.run_id.as_deref().unwrap(), w.clone())
        .unwrap();
    fs::write(Path::new(&w.directory).join("one.txt"), "version A\n").unwrap();
    let ea = crate::coding::snapshot(&owned, &a.id, &w).unwrap();
    let a = store.task(&a.id).unwrap();
    store.code_snapshot(&a.id, a.revision, ea.clone()).unwrap();
    let a = store.task(&a.id).unwrap();
    store
        .enqueue(&a.id, a.revision, crate::model::QueueAction::Start)
        .unwrap();
    let b = complete_next(&store, &crate::delivery::fixture("notes B"));
    let w = crate::coding::prepare(&owned, &b.id, &project).unwrap();
    let b = store
        .prepared_code(&b.id, b.run_id.as_deref().unwrap(), w.clone())
        .unwrap();
    assert_eq!(b.code_workspace.as_ref().unwrap().snapshots.len(), 1);
    assert_eq!(
        b.code_workspace.as_ref().unwrap().snapshots[0].commit,
        ea.commit
    );
    fs::write(Path::new(&w.directory).join("one.txt"), "version B\n").unwrap();
    let eb = crate::coding::snapshot(&owned, &b.id, &w).unwrap();
    store.code_snapshot(&b.id, b.revision, eb.clone()).unwrap();
    let data = store.workspace();
    let bundles = crate::coding::export_bundles(&data.tasks).unwrap();
    assert!(bundles[0].commits.contains(&ea.commit) && bundles[0].commits.contains(&eb.commit));
    let child = store.task(&b.id).unwrap();
    let mut archived = child.clone();
    archived.archived = true;
    assert!(crate::team_store::coding_authorized(&store.workspace(), &archived).is_err());
    let p = store.task(&parent.id).unwrap();
    let revised = store.revise_team(&p.id, p.revision, plan(&store)).unwrap();
    assert_eq!(revised.team.as_ref().unwrap().phase, "plan");
    let before = fs::read(store.directory.join("workspace.json")).unwrap();
    assert!(store.code_snapshot(&child.id, child.revision, eb).is_err());
    assert_eq!(
        fs::read(store.directory.join("workspace.json")).unwrap(),
        before
    );
    assert_eq!(store.task(&parent.id).unwrap().team.unwrap().phase, "plan");
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn failed_summary_is_actionable_and_can_retry_without_repeating_workers() {
    let (s, dir) = fixture();
    let p = s.create_team(plan(&s)).unwrap();
    s.confirm_team(&p.id, p.revision, &p.team.as_ref().unwrap().plan.version)
        .unwrap();
    complete_next(&s, &crate::delivery::fixture("one"));
    complete_next(&s, &crate::delivery::fixture("two"));
    let (mut t, _) = s.claim_next().unwrap().unwrap();
    let q = t.queue.clone().unwrap();
    let saved = s
        .finish_claim(&t.id, &q.request_id, &q.next_run_id, None)
        .unwrap();
    t.merge_platform(&saved);
    t.status = "failed".into();
    t.event("summary fixture failed", "error", "fixture");
    s.save_existing_task(t).unwrap().unwrap();
    let t = s.task(&p.id).unwrap();
    assert_eq!(t.team.as_ref().unwrap().phase, "summary");
    assert!(t
        .team
        .as_ref()
        .unwrap()
        .error
        .as_ref()
        .unwrap()
        .contains("重试汇总"));
    let children = t.team.as_ref().unwrap().children.clone();
    s.enqueue(&t.id, t.revision, crate::model::QueueAction::Start)
        .unwrap();
    let done = complete_next(&s, &crate::delivery::fixture("summary retry"));
    assert_eq!(done.team.as_ref().unwrap().children, children);
    assert_eq!(done.team.as_ref().unwrap().phase, "review");
    std::fs::remove_dir_all(dir).unwrap();
}
