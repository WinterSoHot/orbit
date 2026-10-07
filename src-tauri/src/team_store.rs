use crate::{
    model::Task,
    store::{Store, Workspace},
    team::*,
};
fn order(data: &Workspace) -> Result<u64, String> {
    data.tasks
        .iter()
        .filter_map(|t| t.queue.as_ref().map(|q| q.order))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .filter(|n| *n < 9_007_199_254_740_991)
        .ok_or("队列序号超限".into())
}
fn plan(data: &Workspace, d: PlanDraft) -> Result<Plan, String> {
    let agent = |id: &str| {
        data.agents
            .iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or_else(|| "Agent 已删除，请重新选择".to_string())
    };
    let p = Plan {
        version: uuid::Uuid::new_v4().to_string(),
        goal: d.goal.trim().into(),
        criteria: d.criteria.trim().into(),
        coordinator: agent(&d.coordinator_id)?,
        reviewer: agent(&d.reviewer_id)?,
        workers: d
            .workers
            .into_iter()
            .map(|w| {
                Ok(Assignment {
                    agent: agent(&w.agent_id)?,
                    goal: w.goal.trim().into(),
                    coding: w.coding,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        project: d.project,
    };
    p.validate()?;
    Ok(p)
}
fn flow(plan: Plan) -> TeamWorkflow {
    TeamWorkflow {
        plan,
        phase: "plan".into(),
        confirmed: None,
        children: vec![],
        summary_input: None,
        review_task: None,
        review_input: None,
        review: None,
        error: None,
        cancelled: false,
        integration: None,
        git_operation: None,
    }
}
fn items(data: &Workspace, ids: &[String]) -> Result<Vec<InputItem>, String> {
    let mut result = vec![];
    for id in ids {
        let t = data
            .tasks
            .iter()
            .find(|t| &t.id == id)
            .ok_or("必需子任务已不存在")?;
        if t.status != "completed" || t.queue.is_some() {
            return Err(format!("{} 尚未完成", t.title));
        }
        let current = t.current_delivery_ids();
        if current.is_empty() {
            return Err(format!("{} 未提交正式交付", t.title));
        }
        for a in t.artifacts.iter().filter(|a| current.contains(&a.id)) {
            result.push(InputItem {
                task_id: t.id.clone(),
                run_id: t.run_id.clone().ok_or("交付缺少运行身份")?,
                turn_id: t.turn_id.clone().ok_or("交付缺少轮次身份")?,
                artifact_id: a.id.clone(),
                kind: a.kind.clone(),
                name: a.name.clone(),
                content: a.content.clone(),
            });
        }
    }
    Ok(result)
}
fn input(
    data: &Workspace,
    p: &Plan,
    items: Vec<InputItem>,
    code: Option<CodeEvidence>,
    revision: &str,
) -> Result<ExecutionInput, String> {
    let context=serde_json::to_string(&serde_json::json!({"revision":revision,"sourcesAndDirections":data.tasks.iter().filter(|t|items.iter().any(|i|i.task_id==t.id)).map(|t|serde_json::json!({"taskId":t.id,"sourceInputs":t.source_inputs,"directions":t.directions.iter().filter(|d|d.status=="accepted").collect::<Vec<_>>(),"supplements":t.supplements})).collect::<Vec<_>>()})).map_err(|_|"无法冻结上下文")?;
    let i = ExecutionInput {
        context,
        version: uuid::Uuid::new_v4().to_string(),
        plan: serde_json::to_string(p).map_err(|_| "无法编码计划")?,
        items,
        code,
    };
    i.validate()?;
    Ok(i)
}
impl Store {
    fn team_commit(&self, data: &mut Workspace, next: Workspace) -> Result<(), String> {
        crate::team::validate_links(&next.tasks)?;
        for t in &next.tasks {
            crate::store::validate(t)?
        }
        if next.tasks.len() > 50 {
            return Err("最多保存 50 个任务，请清理无引用的归档任务".into());
        }
        self.persist(&next)?;
        *data = next;
        Ok(())
    }
    pub fn save_agent(&self, mut a: AgentProfile) -> Result<AgentProfile, String> {
        a.name = a.name.trim().into();
        a.role = a.role.trim().into();
        a.validate()?;
        let mut data = self.data.lock().unwrap();
        let mut next = data.clone();
        if let Some(old) = next.agents.iter_mut().find(|p| p.id == a.id) {
            if old.revision != a.revision {
                return Err("Agent 已更新，请重新加载".into());
            }
            a.revision = a.revision.checked_add(1).ok_or("Agent 版本超限")?;
            *old = a.clone();
        } else {
            if a.revision != 0 || next.agents.len() >= 12 {
                return Err("最多配置 12 个 Agent".into());
            }
            next.agents.push(a.clone());
        }
        self.team_commit(&mut data, next)?;
        Ok(a)
    }
    pub fn delete_agent(&self, id: &str, revision: u64) -> Result<(), String> {
        let mut data = self.data.lock().unwrap();
        if !data
            .agents
            .iter()
            .any(|a| a.id == id && a.revision == revision)
        {
            return Err("Agent 已更新或删除".into());
        }
        let mut next = data.clone();
        next.agents.retain(|a| a.id != id);
        self.team_commit(&mut data, next)
    }
    pub fn create_team(&self, d: PlanDraft) -> Result<Task, String> {
        if d.title.trim().is_empty() || d.title.chars().count() > 100 {
            return Err("请填写任务名称".into());
        }
        let mut data = self.data.lock().unwrap();
        let p = plan(
            &data,
            PlanDraft {
                title: d.title.clone(),
                ..d
            },
        )?;
        let mut parent = unstarted(d.title.trim().into(), p.goal.clone(), p.coordinator.clone());
        parent.team = Some(flow(p));
        parent.event("分工计划已创建，等待你的确认", "system", "工作台");
        let mut next = data.clone();
        next.tasks.insert(0, parent.clone());
        self.team_commit(&mut data, next)?;
        Ok(parent)
    }
    pub fn confirm_team(&self, id: &str, revision: u64, version: &str) -> Result<Task, String> {
        let mut data = self.data.lock().unwrap();
        let index = data
            .tasks
            .iter()
            .position(|t| t.id == id && t.revision == revision && !t.archived)
            .ok_or("任务已更新")?;
        let mut parent = data.tasks[index].clone();
        let w = parent
            .team
            .as_mut()
            .filter(|w| w.phase == "plan" && !w.cancelled && w.plan.version == version)
            .ok_or("计划版本已变化，不能确认")?;
        let mut next = data.clone();
        if next.tasks.len() + w.plan.workers.len() + 1 > 50 {
            return Err("任务容量不足，请为分工和独立评审预留空间".into());
        }
        let mut number = order(&next)?;
        let mut children = vec![];
        for (n, work) in w.plan.workers.iter().enumerate() {
            let mut child = unstarted(
                format!("{} · 分工 {}", work.agent.name, n + 1),
                work.goal.clone(),
                work.agent.clone(),
            );
            child.parent_link = Some(ParentLink {
                parent_id: id.into(),
                plan_version: version.into(),
                role: "worker".into(),
                coding: work.coding,
            });
            child.team_input = Some(input(&next, &w.plan, vec![], None, "")?);
            enqueue(&mut child, number);
            number += 1;
            children.push(child.id.clone());
            next.tasks.push(child);
        }
        w.children = children;
        w.confirmed = Some(version.into());
        w.phase = "work".into();
        w.error = None;
        parent.event("计划已确认，分工进入并行队列", "system", "你");
        next.tasks[index] = parent.clone();
        self.team_commit(&mut data, next)?;
        Ok(parent)
    }
    pub fn revise_team(&self, id: &str, revision: u64, d: PlanDraft) -> Result<Task, String> {
        let mut data = self.data.lock().unwrap();
        let index = data
            .tasks
            .iter()
            .position(|t| t.id == id && t.revision == revision && !t.archived)
            .ok_or("任务已更新")?;
        if data.tasks.iter().any(|t| {
            (t.id == id || t.parent_link.as_ref().is_some_and(|p| p.parent_id == id))
                && (t.queue.is_some()
                    || matches!(t.status.as_str(), "running" | "approval" | "cancelling"))
        }) {
            return Err("请先结束或取消本计划的活动任务再修改".into());
        }
        if d.title.trim().is_empty() || d.title.chars().count() > 100 {
            return Err("请填写任务名称".into());
        }
        let title = d.title.trim().to_string();
        let p = plan(&data, d)?;
        let mut next = data.clone();
        let t = &mut next.tasks[index];
        t.title = title;
        t.provider = p.coordinator.provider.clone();
        t.requested_model = p.coordinator.model.clone();
        t.assignment = Some(p.coordinator.clone());
        t.prompt = p.goal.clone();
        t.team = Some(flow(p));
        t.acceptance = None;
        t.team_input = None;
        t.status = "queued".into();
        t.event("新计划等待确认，旧分工与成果保留", "system", "你");
        let saved = t.clone();
        self.team_commit(&mut data, next)?;
        Ok(saved)
    }
    pub fn revise_summary(&self, id: &str, revision: u64, text: &str) -> Result<Task, String> {
        if text.trim().is_empty() || text.chars().count() > 2000 {
            return Err("请填写 1–2000 字修订要求".into());
        }
        let mut data = self.data.lock().unwrap();
        let index = data
            .tasks
            .iter()
            .position(|t| {
                t.id == id
                    && t.revision == revision
                    && !t.archived
                    && t.queue.is_none()
                    && !matches!(t.status.as_str(), "running" | "approval" | "cancelling")
            })
            .ok_or("任务仍在执行或已更新")?;
        let mut t = data.tasks[index].clone();
        let mut w = t
            .team
            .clone()
            .filter(|w| matches!(w.phase.as_str(), "revision" | "ready") && !w.cancelled)
            .ok_or("当前没有可修订的团队交付")?;
        let package = input(
            &data,
            &w.plan,
            items(&data, &w.children)?,
            w.integration.clone(),
            text.trim(),
        )?;
        t.team_input = Some(package.clone());
        w.summary_input = Some(package);
        w.review = None;
        w.review_input = None;
        w.review_task = None;
        w.phase = "summary".into();
        w.error = None;
        t.team = Some(w);
        t.directions.push(crate::model::Direction {
            id: uuid::Uuid::new_v4().to_string(),
            run_id: t.run_id.clone().unwrap_or_default(),
            turn_id: t.turn_id.clone().unwrap_or_default(),
            text: text.trim().into(),
            status: "accepted".into(),
            created_at: crate::model::now(),
        });
        t.acceptance = None;
        let mut next = data.clone();
        let n = order(&next)?;
        enqueue(&mut t, n);
        next.tasks[index] = t.clone();
        self.team_commit(&mut data, next)?;
        Ok(t)
    }
    pub fn cancel_team(&self, id: &str, revision: u64) -> Result<Task, String> {
        let mut data = self.data.lock().unwrap();
        let index = data
            .tasks
            .iter()
            .position(|t| t.id == id && t.revision == revision && !t.archived)
            .ok_or("任务已更新")?;
        let mut next = data.clone();
        let t = &mut next.tasks[index];
        let w = t.team.as_mut().ok_or("非团队任务")?;
        w.cancelled = true;
        w.phase = "cancelled".into();
        t.acceptance = None;
        if !matches!(t.status.as_str(), "running" | "approval" | "cancelling") {
            t.status = "interrupted".into();
        }
        t.event("团队取消意图已保存，正在停止相关运行", "system", "你");
        for child in next
            .tasks
            .iter_mut()
            .filter(|t| t.id == id || t.parent_link.as_ref().is_some_and(|p| p.parent_id == id))
        {
            if let Some(q) = child.queue.as_mut().filter(|q| q.state == "claimed") {
                q.cancel_requested = true;
                q.error = Some("团队取消，等待启动清理确认".into());
            }
            if child.queue.as_ref().is_some_and(|q| q.state == "pending") {
                child.queue = None;
                child.status = "interrupted".into();
                child.event("父任务已取消，排队已撤销", "system", "工作台");
            }
        }
        let saved = next.tasks[index].clone();
        self.team_commit(&mut data, next)?;
        Ok(saved)
    }
    pub fn advance_teams(&self) -> Result<Workspace, String> {
        let mut data = self.data.lock().unwrap();
        let mut next = data.clone();
        advance(&mut next);
        if next
            .tasks
            .iter()
            .zip(&data.tasks)
            .any(|(a, b)| a.revision != b.revision)
            || next.tasks.len() != data.tasks.len()
        {
            self.team_commit(&mut data, next)?
        }
        Ok(data.clone())
    }
}
pub fn protect(data: &Workspace, id: &str) -> Result<(), String> {
    if data.tasks.iter().any(|t| {
        t.parent_link.as_ref().is_some_and(|p| p.parent_id != id)
            && t.id == id
            && data.tasks.iter().any(|parent| {
                t.parent_link
                    .as_ref()
                    .is_some_and(|p| parent.id == p.parent_id)
            })
    }) {
        return Err("任务仍被父任务引用，请先处理父任务".into());
    }
    Ok(())
}
pub fn can_enqueue(data: &Workspace, t: &Task) -> Result<(), String> {
    if let Some(w) = &t.team {
        if !matches!(w.phase.as_str(), "summary") || w.cancelled {
            return Err("请通过计划确认、修订或取消入口推进团队任务".into());
        }
    }
    if let Some(link) = &t.parent_link {
        let parent = data
            .tasks
            .iter()
            .find(|p| p.id == link.parent_id)
            .and_then(|p| p.team.as_ref())
            .ok_or("父计划不存在")?;
        if parent.cancelled
            || parent.plan.version != link.plan_version
            || link.role == "worker" && !parent.children.contains(&t.id)
            || link.role == "review" && parent.review_task.as_ref() != Some(&t.id)
        {
            return Err("父计划已取消或更新，旧分工不能重启".into());
        }
    }
    Ok(())
}
pub fn can_accept(data: &Workspace, t: &Task) -> Result<(), String> {
    if let Some(w) = &t.team {
        if w.phase != "ready"
            || w.cancelled
            || w.confirmed.as_ref() != Some(&w.plan.version)
            || w.review_task
                .as_ref()
                .and_then(|id| data.tasks.iter().find(|r| &r.id == id))
                .is_none_or(|r| {
                    r.status != "completed" || r.queue.is_some() || r.review_submission != w.review
                })
            || w.review_input.as_ref().is_none_or(|i| {
                i.code != w.integration || i.code.as_ref().is_some_and(|c| !c.complete)
            })
            || w.review.as_ref().is_none_or(|r| r.packet.verdict != "pass")
            || w.review_input
                .as_ref()
                .is_none_or(|i| !i.matches(&data.tasks))
        {
            return Err("需要当前完整成果通过独立评审后才能验收".into());
        }
    }
    Ok(())
}
pub fn advance(data: &mut Workspace) {
    let event_ids: std::collections::HashSet<String> = data
        .tasks
        .iter()
        .flat_map(|t| t.events.iter().map(|e| e.id.clone()))
        .collect();
    let parents: Vec<_> = data
        .tasks
        .iter()
        .filter(|t| t.team.is_some())
        .map(|t| t.id.clone())
        .collect();
    for id in parents {
        let index = data.tasks.iter().position(|t| t.id == id).unwrap();
        let before = data.clone();
        if let Err(error) = advance_one(data, index) {
            *data = before;
            let p = &mut data.tasks[index];
            let w = p.team.as_mut().unwrap();
            let error: String = error.chars().take(2000).collect();
            if w.error.as_ref() != Some(&error) {
                w.error = Some(error.clone());
                p.event(&error, "error", "工作流");
            }
        }
    }
    for t in &mut data.tasks {
        for e in &mut t.events {
            if !event_ids.contains(&e.id) && !e.id.starts_with("team:") {
                e.id = format!("team:{}", uuid::Uuid::new_v4());
            }
        }
    }
}
fn advance_one(data: &mut Workspace, index: usize) -> Result<(), String> {
    let mut parent = data.tasks[index].clone();
    let mut w = parent.team.clone().unwrap();
    if w.cancelled || parent.archived || w.phase == "plan" {
        return Ok(());
    }
    if w.summary_input
        .as_ref()
        .is_some_and(|i| !i.matches(&data.tasks))
        || w.review_input
            .as_ref()
            .is_some_and(|i| !i.matches(&data.tasks))
    {
        if w.phase != "revision" || parent.acceptance.is_some() {
            w.phase = "revision".into();
            w.review = None;
            w.error = Some("采用的成果版本已变化，请重新汇总和评审".into());
            parent.acceptance = None;
            parent.event("成果变化使旧评审失效", "system", "工作台");
            parent.team = Some(w);
            data.tasks[index] = parent;
        }
        return Ok(());
    }
    if w.confirmed.as_ref() != Some(&w.plan.version)
        || w.children.len() != w.plan.workers.len()
        || w.children.is_empty()
    {
        return Err("计划未确认或必需分工不完整，不能推进".into());
    }
    match w.phase.as_str() {
        "work" => {
            let children: Vec<_> = w
                .children
                .iter()
                .filter_map(|id| data.tasks.iter().find(|t| &t.id == id))
                .collect();
            if children.len() != w.children.len() {
                return Err("分工任务缺失".into());
            }
            if children
                .iter()
                .any(|t| matches!(t.status.as_str(), "failed" | "interrupted" | "unknown"))
            {
                return Err("分工未成功结束，请核对并重试对应任务".into());
            }
            if children
                .iter()
                .any(|t| t.status != "completed" || t.queue.is_some())
            {
                return Ok(());
            }
            if w.plan.workers.iter().any(|a| a.coding) && w.integration.is_none() {
                return Err("编程成果需要保存真实变更快照并集成后再汇总".into());
            }
            let package = input(
                data,
                &w.plan,
                items(data, &w.children)?,
                w.integration.clone(),
                "",
            )?;
            parent.team_input = Some(package.clone());
            w.summary_input = Some(package);
            w.phase = "summary".into();
            w.error = None;
            enqueue(&mut parent, order(data)?);
        }
        "summary" => {
            if parent.queue.is_none()
                && matches!(parent.status.as_str(), "failed" | "interrupted" | "unknown")
            {
                return Err("汇总未成功结束，请核对后重试汇总".into());
            }
            if parent.status != "completed" || parent.queue.is_some() {
                return Ok(());
            }
            let mut material = w
                .summary_input
                .as_ref()
                .ok_or("汇总输入不存在")?
                .items
                .clone();
            material.extend(items(data, &[parent.id.clone()])?);
            let package = input(
                data,
                &w.plan,
                material,
                w.integration.clone(),
                &w.summary_input.as_ref().unwrap().context,
            )?;
            if data.tasks.len() >= 50 {
                return Err("没有剩余任务容量创建独立评审，请清理无引用的归档".into());
            }
            let mut review = unstarted(
                format!("{} · 独立评审", w.plan.reviewer.name),
                "核对冻结计划、原始成果与汇总，按协议提交独立评审。".into(),
                w.plan.reviewer.clone(),
            );
            review.team_input = Some(package.clone());
            review.parent_link = Some(ParentLink {
                parent_id: parent.id.clone(),
                plan_version: w.plan.version.clone(),
                role: "review".into(),
                coding: false,
            });
            enqueue(&mut review, order(data)?);
            w.review_task = Some(review.id.clone());
            w.review_input = Some(package);
            w.phase = "review".into();
            w.error = None;
            data.tasks.push(review);
            parent.event("汇总交付已提交，独立评审已排队", "system", "工作台");
        }
        "review" => {
            let review = data
                .tasks
                .iter()
                .find(|t| Some(&t.id) == w.review_task.as_ref())
                .ok_or("评审任务不存在")?;
            if review.queue.is_some()
                || matches!(
                    review.status.as_str(),
                    "running" | "approval" | "cancelling" | "queued"
                )
            {
                return Ok(());
            }
            if review.status != "completed" {
                return Err("评审运行失败，请重试评审任务".into());
            }
            let r = review
                .review_submission
                .clone()
                .ok_or("评审未提交有效结论，请重新明确提交")?;
            if Some(&r.packet.input_version) != w.review_input.as_ref().map(|i| &i.version)
                || Some(&r.run_id) != review.run_id.as_ref()
                || Some(&r.turn_id) != review.turn_id.as_ref()
            {
                return Err("评审输入或运行版本不一致".into());
            }
            let complete = w
                .review_input
                .as_ref()
                .and_then(|i| i.code.as_ref())
                .is_none_or(|c| c.complete);
            w.phase = if r.packet.verdict == "pass" && complete {
                "ready"
            } else {
                "revision"
            }
            .into();
            w.error = if w.phase == "ready" {
                None
            } else {
                Some(if !complete {
                    "代码 Diff 评审材料不完整，禁止合并".into()
                } else {
                    r.packet.summary.chars().take(2000).collect()
                })
            };
            w.review = Some(r);
            parent.event(
                if w.phase == "ready" {
                    "独立评审通过，等待你的验收"
                } else {
                    "独立评审需要介入或修订"
                },
                "system",
                "工作台",
            );
        }
        _ => return Ok(()),
    }
    parent.team = Some(w);
    data.tasks[index] = parent;
    Ok(())
}
impl Store {
    pub fn git_operation(&self, id: &str, op: GitOperation) -> Result<Task, String> {
        let mut data = self.data.lock().unwrap();
        let mut next = data.clone();
        let t = next
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("父任务不存在")?;
        let w = t.team.as_mut().ok_or("非团队任务")?;
        if t.archived || w.cancelled || w.confirmed.as_ref() != Some(&w.plan.version) {
            return Err("计划未确认、已取消或归档".into());
        }
        w.git_operation = Some(op);
        t.event("Git 操作记录已保存", "system", "工作台");
        let saved = t.clone();
        self.team_commit(&mut data, next)?;
        Ok(saved)
    }
    pub fn prepared_code(&self, id: &str, run: &str, w: CodeWorkspace) -> Result<Task, String> {
        let mut data = self.data.lock().unwrap();
        let task = data.tasks.iter().find(|t| t.id == id).ok_or("分工不存在")?;
        coding_authorized(&data, task)?;
        let mut next = data.clone();
        let t = next
            .tasks
            .iter_mut()
            .find(|t| t.id == id && t.run_id.as_deref() == Some(run))
            .ok_or("启动身份已变化")?;
        if t.parent_link.as_ref().is_none_or(|p| !p.coding) {
            return Err("任务没有写工作区授权".into());
        }
        let mut w = w;
        if let Some(old) = &t.code_workspace {
            if old.project != w.project || old.directory != w.directory {
                return Err("工作区身份已变化".into());
            }
            w.snapshots = old.snapshots.clone();
        }
        t.code_workspace = Some(w);
        t.event("独立代码工作区已准备", "system", "工作台");
        let saved = t.clone();
        coding_authorized(&next, &saved)?;
        self.team_commit(&mut data, next)?;
        Ok(saved)
    }
    pub fn code_snapshot(&self, id: &str, revision: u64, e: CodeEvidence) -> Result<Task, String> {
        let mut data = self.data.lock().unwrap();
        let mut next = data.clone();
        let index = next
            .tasks
            .iter()
            .position(|t| {
                t.id == id && t.revision == revision && t.status == "completed" && t.queue.is_none()
            })
            .ok_or("任务未完成或已更新")?;
        coding_authorized(&next, &next.tasks[index])?;
        let parent_id = next.tasks[index]
            .parent_link
            .as_ref()
            .ok_or("任务没有父计划")?
            .parent_id
            .clone();
        let t = &mut next.tasks[index];
        let w = t.code_workspace.as_mut().ok_or("工作区不存在")?;
        crate::team::validate_code(&e)?;
        if e.base != w.project.base {
            return Err("快照基线不匹配".into());
        }
        if w.snapshot.as_ref().is_some_and(|old| old.tree == e.tree) {
            return Ok(t.clone());
        }
        if t.artifacts.len() >= 10 || t.delivery_submissions.len() >= 10 {
            return Err("成果容量已满，快照对象保留但未提交".into());
        }
        let aid = uuid::Uuid::new_v4().to_string();
        let sid = uuid::Uuid::new_v4().to_string();
        let result=crate::delivery::Item::Result{name:"代码变更快照".into(),summary:format!("已捕获已跟踪及未被忽略的普通文件；被忽略目录不在快照范围内。\n提交：{}\n基线：{}\n测试结果未由平台独立验证。",e.commit,e.base),evidence:vec![format!("tree: {}",e.tree),format!("Diff 完整可预览：{}",e.complete)]};
        let canonical = serde_json::to_string(&crate::delivery::Packet {
            schema_version: 1,
            submission_id: sid.clone(),
            items: vec![result.clone()],
        })
        .unwrap();
        if w.snapshots.len() >= 10 {
            return Err("快照历史容量已满".into());
        }
        w.snapshots.push(e.clone());
        w.snapshot = Some(e);
        w.artifact_id = Some(aid.clone());
        t.artifacts.push(crate::model::Artifact {
            id: aid.clone(),
            name: "代码变更快照".into(),
            kind: "result".into(),
            content: serde_json::to_string(&result).unwrap(),
            created_at: crate::model::now(),
            source_input_ids: vec![],
        });
        t.delivery_submissions.push(crate::delivery::Receipt {
            id: sid,
            run_id: t.run_id.clone().ok_or("运行身份缺失")?,
            turn_id: t.turn_id.clone(),
            thread_id: t.thread_id.clone().ok_or("会话身份缺失")?,
            item_id: format!("workspace-{}", aid),
            origin: "workspace".into(),
            canonical,
            artifact_ids: vec![aid],
        });
        t.event("真实代码快照已保存", "artifact", "工作台");
        let p = next
            .tasks
            .iter_mut()
            .find(|t| t.id == parent_id)
            .ok_or("父计划不存在")?;
        let w = p.team.as_mut().unwrap();
        if w.cancelled {
            return Err("父计划已取消".into());
        }
        w.integration = None;
        w.review = None;
        w.review_input = None;
        w.review_task = None;
        w.summary_input = None;
        w.phase = "work".into();
        w.error = None;
        p.acceptance = None;
        p.event("代码快照变化，等待集成与新评审", "system", "工作台");
        advance(&mut next);
        let saved = next.tasks[index].clone();
        self.team_commit(&mut data, next)?;
        Ok(saved)
    }
    pub fn code_integration(&self, id: &str, e: CodeEvidence) -> Result<Task, String> {
        let mut data = self.data.lock().unwrap();
        let mut next = data.clone();
        let index = next
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or("父任务不存在")?;
        let p = &mut next.tasks[index];
        let w = p
            .team
            .as_mut()
            .filter(|w| w.phase == "work" && !w.cancelled)
            .ok_or("计划已变化，不能集成")?;
        w.integration = Some(e);
        w.error = None;
        p.event("独立代码变更已集成，等待汇总与评审", "system", "工作台");
        advance(&mut next);
        let saved = next.tasks[index].clone();
        self.team_commit(&mut data, next)?;
        Ok(saved)
    }
}

pub fn coding_authorized(data: &Workspace, t: &Task) -> Result<(), String> {
    let link = t
        .parent_link
        .as_ref()
        .filter(|l| l.coding && l.role == "worker")
        .ok_or("非编程分工")?;
    let parent = data
        .tasks
        .iter()
        .find(|p| p.id == link.parent_id && !p.archived)
        .ok_or("父任务不存在或已归档")?;
    let w = parent
        .team
        .as_ref()
        .filter(|w| {
            !w.cancelled
                && w.confirmed.as_ref() == Some(&w.plan.version)
                && w.plan.version == link.plan_version
                && w.children.len() == w.plan.workers.len()
        })
        .ok_or("父计划未确认或已经变化")?;
    let index = w
        .children
        .iter()
        .position(|id| id == &t.id)
        .ok_or("不是当前必需分工")?;
    let a = &w.plan.workers[index];
    if t.archived
        || t.provider != "codex"
        || !a.coding
        || t.assignment.as_ref() != Some(&a.agent)
        || t.code_workspace
            .as_ref()
            .is_some_and(|c| Some(&c.project) != w.plan.project.as_ref())
    {
        return Err("编程分工授权或工作区身份无效".into());
    }
    Ok(())
}
