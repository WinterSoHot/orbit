use serde::{Deserialize,Serialize};
use crate::model::{Artifact,Task,now};
pub const MAX_PACKET:usize=256000;

#[derive(Clone,Debug,Default)]
pub struct Candidate {pub item_id:String,pub text:String,pub ambiguous:bool,pub truncated:bool,pub declared:bool}
impl Candidate {
    pub fn capture(&mut self,id:&str,text:&str){
        self.declared|=text.trim_start().starts_with("```orbit-delivery")||text.trim_start().starts_with("```orbit-review");
        if !self.item_id.is_empty()&&(self.item_id!=id||self.text!=text) {self.ambiguous=true;self.text.clear();return}
        self.item_id=id.into();self.truncated=id.is_empty()||id.len()>128||text.len()>MAX_PACKET||text.chars().count()>crate::model::OUTPUT_LIMIT;
        self.text=if self.truncated{String::new()}else{text.into()};
    }
}
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
#[serde(tag="kind",rename_all="camelCase",deny_unknown_fields)]
pub enum Item {
    Markdown{name:String,content:String},
    Link{name:String,url:String},
    Result{name:String,summary:String,evidence:Vec<String>},
}
impl Item {
    fn name(&self)->&str{match self{Self::Markdown{name,..}|Self::Link{name,..}|Self::Result{name,..}=>name}}
    fn validate(&self)->Result<(),String>{
        let name=self.name();if name.trim().is_empty()||name.chars().count()>120||name.chars().any(char::is_control){return Err("成果名称应为 1–120 字，不能含控制字符".into())}
        match self{
            Self::Markdown{content,..} if content.trim().is_empty()||content.len()>MAX_PACKET=>Err("文档正文为空或超过 256 KB".into()),
            Self::Link{url,..}=>{let parsed=reqwest::Url::parse(url).map_err(|_|"成果链接无效")?;if url.len()>4096||url.chars().any(char::is_control)||url.contains(['<','>'])||!matches!(parsed.scheme(),"http"|"https")||parsed.host_str().is_none()||!parsed.username().is_empty()||parsed.password().is_some(){Err("成果链接仅允许无账号凭据的 HTTP/HTTPS 地址".into())}else{Ok(())}},
            Self::Result{summary,evidence,..} if summary.trim().is_empty()||summary.chars().count()>10000||evidence.len()>10||evidence.iter().any(|e|e.trim().is_empty()||e.chars().count()>2000)=>Err("结果摘要或证据条目为空或超过限制".into()),
            _=>Ok(())
        }
    }
    fn artifact(&self,ids:Vec<String>)->Artifact{
        let (kind,content)=match self {Self::Markdown{content,..}=>("markdown",content.clone()),Self::Link{url,..}=>("link",url.clone()),Self::Result{..}=>("result",serde_json::to_string(self).unwrap())};
        Artifact{id:uuid::Uuid::new_v4().to_string(),name:self.name().into(),kind:kind.into(),content,source_input_ids:ids,created_at:now()}
    }
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Packet {pub schema_version:u32,pub submission_id:String,pub items:Vec<Item>}
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Receipt {
    pub id:String,pub run_id:String,pub turn_id:Option<String>,pub thread_id:String,pub item_id:String,
    pub origin:String,pub canonical:String,pub artifact_ids:Vec<String>,
}
pub fn instruction(input:&str)->String{
    format!("{input}\n\nOrbit 交付规则：普通说明、进度和澄清只回复对话，不算交付。确有成果才提交；最后一条完整最终回复必须仅包含一个 ```orbit-delivery 换行 JSON 换行 ``` 协议块，不加其他文字，不展示协议示例。JSON 格式为 {{\"schemaVersion\":1,\"submissionId\":\"本轮唯一标识\",\"items\":[成果]}}。成果仅可为 {{\"kind\":\"markdown\",\"name\":\"方案.md\",\"content\":\"完整正文\"}}、{{\"kind\":\"link\",\"name\":\"成果名称\",\"url\":\"https://有效地址\"}} 或 {{\"kind\":\"result\",\"name\":\"结果名称\",\"summary\":\"实际结果\",\"evidence\":[\"可核对的证据说明\"]}}。链接不会自动验证，结果须真实说明未验证部分；不得伪造文件、路径或已完成操作。最多10项，整个包256KB，submissionId为1–64个ASCII字母数字下划线或连字符。当前只读执行权限不变。")
}
pub fn parse(text:&str)->Result<Option<Packet>,String>{
    let text=text.trim();if !text.starts_with("```orbit-delivery"){return Ok(None)}
    if text.len()>MAX_PACKET{return Err("交付包超过 256 KB".into())}
    let body=text.strip_prefix("```orbit-delivery\n").and_then(|s|s.strip_suffix("\n```" )).ok_or("交付声明必须是独立的完整协议块")?;
    let packet:Packet=serde_json::from_str(body).map_err(|_|"交付格式无效：请检查字段、类型与完整 JSON")?;
    if packet.schema_version!=1||packet.submission_id.is_empty()||packet.submission_id.len()>64||!packet.submission_id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'||b==b'-')||packet.items.is_empty()||packet.items.len()>10{return Err("交付版本、标识或条目数量无效".into())}
    for item in &packet.items{item.validate()?;}Ok(Some(packet))
}
// Called only by Store with a successful current root run; no ordinary-text fallback.
pub fn commit(task:&mut Task)->Result<bool,String>{
    let c=&task.delivery_candidate;
    if !c.declared{return Ok(false)}
    if c.ambiguous{return Err("收到多个最终回复，无法确认独立交付；请重新明确提交".into())}
    if c.truncated{return Err("最终提交内容不完整或超限，未保存交付".into())}
    let Some(packet)=parse(&c.text)? else{return Ok(false)};
    let run=task.run_id.clone().ok_or("交付缺少运行身份")?;let turn=task.turn_id.clone().ok_or("交付缺少轮次身份")?;
    let thread=task.session_ref.as_ref().map(|s|s.id.clone()).or_else(||task.thread_id.clone()).ok_or("交付缺少会话身份")?;
    let canonical=serde_json::to_string(&packet).map_err(|_|"无法保存交付")?;
    if canonical.len()+24>MAX_PACKET{return Err("规范化交付包超限".into())}
    if let Some(old)=task.delivery_submissions.iter().find(|r|r.origin=="executor"&&r.id==packet.submission_id&&r.run_id==run&&r.turn_id.as_deref()==Some(&turn)){
        return if old.canonical==canonical{Ok(true)}else{Err("相同交付标识内容已变化，请使用新的提交标识".into())}
    }
    if task.artifacts.len()+packet.items.len()>10||task.delivery_submissions.len()>=10{return Err("已达到 10 份成果上限，请新建任务".into())}
    let ids=task.provided_input_ids();let artifacts:Vec<_>=packet.items.iter().map(|i|i.artifact(ids.clone())).collect();
    task.delivery_submissions.push(Receipt{id:packet.submission_id,run_id:run,turn_id:Some(turn),thread_id:thread,item_id:c.item_id.clone(),origin:"executor".into(),canonical,artifact_ids:artifacts.iter().map(|a|a.id.clone()).collect()});
    task.artifacts.extend(artifacts);Ok(true)
}
pub fn export_body(a:&Artifact)->Result<String,String>{
    match a.kind.as_str(){
        "markdown"=>Ok(a.content.clone()),
        "link"=>{Item::Link{name:a.name.clone(),url:a.content.clone()}.validate()?;Ok(format!("# {}\n\n成果链接：<{}>\n\n链接由执行器提供，未自动访问或验证。",a.name,a.content))},
        "result"=>{let value:Item=serde_json::from_str(&a.content).map_err(|_|"结果交付内容损坏")?;value.validate()?;let Item::Result{name,summary,evidence}=value else{return Err("结果类型不匹配".into())};if name!=a.name{return Err("结果名称不一致".into())}Ok(format!("# {}\n\n{}\n\n## 提供的证据\n\n{}\n\n以上为执行器报告，尚未独立验证。",a.name,summary,evidence.iter().map(|e|format!("- {e}")).collect::<Vec<_>>().join("\n")))},
        _=>Err("不支持的成果类型".into())
    }
}
pub fn validate_task(t:&Task)->Result<(),String>{
    if t.delivery_submissions.len()>10{return Err("交付提交记录超限".into())}
    let mut keys=std::collections::HashSet::new();let mut artifacts=std::collections::HashSet::new();
    for r in &t.delivery_submissions{
        if r.id.is_empty()||r.id.len()>64||r.run_id.is_empty()||r.run_id.len()>100||r.turn_id.as_ref().is_some_and(|s|s.is_empty()||s.len()>100)||r.thread_id.is_empty()||r.thread_id.len()>400||r.item_id.is_empty()||r.item_id.len()>128||!matches!(r.origin.as_str(),"executor"|"manual"|"workspace")||r.canonical.len()+24>MAX_PACKET||r.artifact_ids.is_empty()||r.artifact_ids.len()>10||!keys.insert((&r.run_id,&r.turn_id,&r.origin,&r.id)){return Err("交付提交身份无效".into())}
        if matches!(r.origin.as_str(),"executor"|"workspace") {let p=parse(&format!("```orbit-delivery\n{}\n```",r.canonical))?.ok_or("交付记录损坏")?;if p.submission_id!=r.id||p.items.len()!=r.artifact_ids.len()||r.turn_id.is_none(){return Err("交付记录不一致".into())}}
        if r.origin=="manual"&&(r.artifact_ids.len()!=1||r.canonical.trim().is_empty()){return Err("手动保存记录无效".into())}
        for id in &r.artifact_ids {if !artifacts.insert(id)||!t.artifacts.iter().any(|a|&a.id==id){return Err("交付成果关联无效".into())}}
    }
    for a in &t.artifacts {if a.kind!="markdown"{export_body(a)?;}}
    Ok(())
}
#[cfg(test)]
pub fn fixture(content:&str)->String{format!("```orbit-delivery\n{}\n```",serde_json::json!({"schemaVersion":1,"submissionId":"fixture","items":[{"kind":"markdown","name":"result.md","content":content}]}))}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn only_independent_typed_declarations_are_valid(){
        assert!(parse("Which option?").unwrap().is_none());let packet=fixture("# Result");assert_eq!(parse(&packet).unwrap().unwrap().items.len(),1);
        assert!(parse(&format!("Explanation\n{packet}")).unwrap().is_none());assert!(parse(&format!("{packet}\n{packet}")).is_err());
        assert!(parse("```orbit-delivery\n{\"schemaVersion\":1,\"schemaVersion\":1,\"submissionId\":\"a\",\"items\":[]}\n```").is_err());
        for content in [serde_json::json!({"kind":"file","name":"fake","path":"/secret"}),serde_json::json!({"kind":"link","name":"link","url":"file:///secret"}),serde_json::json!({"kind":"markdown","name":"doc","content":""})]{let text=format!("```orbit-delivery\n{}\n```",serde_json::json!({"schemaVersion":1,"submissionId":"a","items":[{"kind":"markdown","name":"good.md","content":"good"},content]}));assert!(parse(&text).is_err());}
    }
}

#[cfg(test)] mod store_checks {
    use super::*;
    use crate::{store::Store,protocol::project};
    use serde_json::json;
    fn setup()->(Store,Task){
        let store=Store::open(std::env::temp_dir().join(format!("orbit-delivery-{}",uuid::Uuid::new_v4()))).unwrap();
        let mut t=Task::new("delivery".into(),"goal".into(),"research".into());t.begin_run(false);t.thread_id=Some("root".into());t.turn_id=Some("turn".into());store.save_task(t.clone()).unwrap();(store,t)
    }
    fn final_message(t:&mut Task,text:&str){project(t,&json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"reply","type":"agentMessage","phase":"final_answer","text":text}}}));}
    fn end(t:&mut Task){project(t,&json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"turn","status":"completed"}}}));}
    #[test] fn typed_submission_is_atomic_idempotent_and_acceptance_tracks_current_run(){
        let (s,mut t)=setup();let text=format!("```orbit-delivery\n{}\n```",json!({"schemaVersion":1,"submissionId":"a","items":[{"kind":"markdown","name":"doc.md","content":"# Good"},{"kind":"link","name":"Website","url":"https://example.com/result"},{"kind":"result","name":"Check","summary":"Not independently verified","evidence":["Exit code reported as 0"]}]}));
        final_message(&mut t,&text);end(&mut t);let saved=s.save_existing_task(t).unwrap().unwrap();assert_eq!(saved.artifacts.len(),3);assert_eq!(saved.delivery_submissions.len(),1);
        let md=saved.artifacts[0].id.clone();let edited=s.edit_artifact(&md,"# Good","user edit").unwrap();let mut repeat=edited.actor_snapshot();repeat.event("repeat","system","test");repeat.delivery_candidate=Candidate::default();repeat.delivery_candidate.capture("reply",&text);let repeated=s.save_existing_task(repeat).unwrap().unwrap();assert_eq!(repeated.artifacts.len(),3);assert_eq!(repeated.artifacts[0].content,"user edit");
        let mut conflict=repeated.actor_snapshot();conflict.delivery_candidate=Candidate::default();conflict.delivery_candidate.capture("reply",&text.replace("# Good","different"));conflict.conversation[0].text="mutated reply".into();conflict.event("conflict","system","test");let refused=s.save_existing_task(conflict).unwrap().unwrap();assert_eq!(refused.artifacts.len(),3);assert_eq!(refused.conversation[0].text,text);assert!(refused.delivery_error.as_ref().unwrap().contains("相同交付标识"));assert!(refused.events.iter().any(|e|e.text.contains("相同交付标识")));
        let link=&refused.artifacts[1];assert!(s.edit_artifact(&link.id,&link.content,"new").is_err());let exported=std::fs::read_to_string(s.export_artifact(&link.id).unwrap()).unwrap();assert!(exported.contains("https://example.com/result"));assert!(exported.contains("未自动访问"));assert!(s.collect_artifact(&refused.artifacts[2].id).unwrap().content.contains("尚未独立验证"));
        let accepted=s.accept_task(&refused.id,refused.revision,&refused.run_id,&refused.turn_id).unwrap();assert!(accepted.accepted());
        let mut next=accepted.clone();next.begin_run(true);next.turn_id=Some("turn".into());s.save_task(next.clone()).unwrap();final_message(&mut next,"Which option?");end(&mut next);let no_delivery=s.save_existing_task(next).unwrap().unwrap();assert_eq!(no_delivery.artifacts.len(),3);assert!(no_delivery.current_delivery_ids().is_empty());assert!(no_delivery.delivery_error.is_none());assert!(s.accept_task(&no_delivery.id,no_delivery.revision,&no_delivery.run_id,&no_delivery.turn_id).is_err());assert!(s.archive_task(&no_delivery.id).is_err());
        let reopened=Store::open(s.directory.clone()).unwrap().task(&saved.id).unwrap();assert!(reopened.current_delivery_ids().is_empty());assert_eq!(reopened.delivery_submissions.len(),1);std::fs::remove_dir_all(s.directory).unwrap();
    }
    #[test] fn invalid_ambiguous_truncated_and_failed_packets_never_commit(){
        for mode in ["mixed","multiple","invalid","truncated","failed","interrupted","foreign","old","phase-less"]{
            let (s,mut t)=setup();let good=fixture("good");
            match mode{
                "mixed"=>final_message(&mut t,&format!("Intro\n{good}")),
                "multiple"=>{final_message(&mut t,&good);project(&mut t,&json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"second","type":"agentMessage","phase":"final_answer","text":good}}}));},
                "invalid"=>final_message(&mut t,"```orbit-delivery\n{\"schemaVersion\":1,\"submissionId\":\"a\",\"items\":[{\"kind\":\"markdown\",\"name\":\"good\",\"content\":\"good\"},{\"kind\":\"file\",\"name\":\"fake\",\"path\":\"/secret\"}]}\n```"),
                "truncated"=>final_message(&mut t,&format!("```orbit-delivery\n{}","x".repeat(65000))),
                "foreign"|"old"|"phase-less"=>{project(&mut t,&json!({"method":"item/completed","params":{"threadId":if mode=="foreign"{"child"}else{"root"},"turnId":if mode=="old"{"old"}else{"turn"},"item":{"id":"reply","type":"agentMessage","phase":if mode=="phase-less"{serde_json::Value::Null}else{json!("final_answer")},"text":good}}}));},
                _=>final_message(&mut t,&good)
            }
            if matches!(mode,"failed"|"interrupted"){project(&mut t,&json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"turn","status":mode}}}));}else{end(&mut t)}
            let saved=s.save_existing_task(t).unwrap().unwrap();assert!(saved.artifacts.is_empty(),"{mode}");assert!(saved.delivery_submissions.is_empty(),"{mode}");std::fs::remove_dir_all(s.directory).unwrap();
        }
        let (s,mut t)=setup();final_message(&mut t,&"normal reply".repeat(6500));end(&mut t);let saved=s.save_existing_task(t).unwrap().unwrap();assert!(!saved.events.iter().any(|e|e.text.contains("交付未保存")));std::fs::remove_dir_all(s.directory).unwrap();
    }
    #[test] fn codex_submission_requires_an_explicit_current_turn_identity(){
        for turn in [None,Some(json!(null)),Some(json!(123)),Some(json!("")),Some(json!("old"))]{
            let (s,mut t)=setup();let mut params=json!({"threadId":"root","item":{"id":"reply","type":"agentMessage","phase":"final_answer","text":fixture("good")}});if let Some(turn)=turn{params["turnId"]=turn;}
            project(&mut t,&json!({"method":"item/completed","params":params}));end(&mut t);let saved=s.save_existing_task(t).unwrap().unwrap();assert!(saved.artifacts.is_empty());assert!(saved.delivery_submissions.is_empty());std::fs::remove_dir_all(s.directory).unwrap();
        }
    }
    #[test] fn long_unicode_names_export_within_filesystem_byte_limit(){
        let (s,mut t)=setup();let name="中".repeat(120);let text=format!("```orbit-delivery\n{}\n```",json!({"schemaVersion":1,"submissionId":"names","items":[{"kind":"link","name":name,"url":"https://example.com"},{"kind":"result","name":name,"summary":"result","evidence":[]}]}));final_message(&mut t,&text);end(&mut t);let saved=s.save_existing_task(t).unwrap().unwrap();
        for artifact in &saved.artifacts{let path=std::path::PathBuf::from(s.export_artifact(&artifact.id).unwrap());assert!(path.file_name().unwrap().to_str().unwrap().len()<=255);assert!(path.extension().unwrap()=="md");assert!(std::fs::read_to_string(path).unwrap().contains(&name));}std::fs::remove_dir_all(s.directory).unwrap();
    }
    #[test] fn platform_delivery_event_never_consumes_next_actor_revision(){
        let (s,mut actor)=setup();final_message(&mut actor,&fixture("good"));end(&mut actor);let revision=actor.revision;
        let submitted=s.save_existing_task(actor.clone()).unwrap().unwrap();assert_eq!(submitted.executor_revision,Some(revision));assert!(submitted.revision>revision);
        actor.merge_platform(&submitted);actor.event("cleanup reported","error","test");assert_eq!(actor.revision,revision+1);
        let saved=s.save_existing_task(actor).unwrap().unwrap();assert!(saved.events.iter().any(|e|e.text=="cleanup reported"));assert!(saved.events.iter().any(|e|e.text.contains("成果已通过")));assert_eq!(saved.artifacts.len(),1);std::fs::remove_dir_all(s.directory).unwrap();
    }
    #[test] fn disk_failure_keeps_submission_out_of_memory_and_disk(){
        let (s,mut t)=setup();final_message(&mut t,&fixture("good"));end(&mut t);let before=std::fs::read(s.directory.join("workspace.json")).unwrap();std::fs::create_dir(s.directory.join("workspace.tmp")).unwrap();assert!(s.save_existing_task(t.clone()).is_err());assert!(s.task(&t.id).unwrap().artifacts.is_empty());assert_eq!(before,std::fs::read(s.directory.join("workspace.json")).unwrap());std::fs::remove_dir(s.directory.join("workspace.tmp")).unwrap();assert_eq!(s.save_existing_task(t).unwrap().unwrap().artifacts.len(),1);std::fs::remove_dir_all(s.directory).unwrap();
    }
    #[test] fn manual_save_uses_historical_message_and_sources_and_never_enables_acceptance(){
        let (s,mut t)=setup();let input=crate::sources::SourceInput::new("old-source".into(),"initial","goal".into(),vec![crate::sources::SourceSnapshot{id:uuid::Uuid::new_v4().to_string(),document_id:uuid::Uuid::new_v4().to_string(),revision:1,reader_revision:None,title:"old".into(),kind:"markdown".into(),url:None,text:"old excerpt".into(),pages:vec![],annotation_id:None}],t.run_id.clone(),t.turn_id.clone());t.source_inputs.push(input);s.save_task(t.clone()).unwrap();final_message(&mut t,"Ordinary reply");end(&mut t);let first=s.save_existing_task(t).unwrap().unwrap();let message=first.conversation[0].clone();assert_eq!(message.source_input_ids,vec!["old-source"]);
        let mut next=first.clone();next.begin_run(true);next.turn_id=Some("turn".into());let mut newer=next.source_inputs[0].clone();newer.id="new-source".into();newer.run_id=next.run_id.clone();newer.kind="continue".into();next.source_inputs.push(newer);s.save_task(next.clone()).unwrap();final_message(&mut next,"New ordinary reply");end(&mut next);let current=s.save_existing_task(next).unwrap().unwrap();assert!(s.save_message(&current.id,current.revision,&message.run_id,&message.thread_id,"foreign").is_err());
        let saved=s.save_message(&current.id,current.revision,&message.run_id,&message.thread_id,&message.item_id).unwrap();assert_eq!(saved.artifacts[0].content,"Ordinary reply");assert_eq!(saved.artifacts[0].source_input_ids,vec!["old-source"]);assert!(saved.current_delivery_ids().is_empty());assert!(s.accept_task(&saved.id,saved.revision,&saved.run_id,&saved.turn_id).is_err());assert!(s.archive_task(&saved.id).is_err());let repeated=s.save_message(&saved.id,saved.revision,&message.run_id,&message.thread_id,&message.item_id).unwrap();assert_eq!(repeated.artifacts.len(),1);assert_eq!(Store::open(s.directory.clone()).unwrap().task(&saved.id).unwrap().delivery_submissions[0].origin,"manual");std::fs::create_dir(s.directory.join("workspace.tmp")).unwrap();assert!(s.delete_task(&saved.id).is_err());assert!(s.task(&saved.id).is_some());std::fs::remove_dir(s.directory.join("workspace.tmp")).unwrap();s.delete_task(&saved.id).unwrap();assert!(Store::open(s.directory.clone()).unwrap().task(&saved.id).is_none());let mut late=saved.clone();late.event("late","system","test");assert!(s.save_existing_task(late).unwrap().is_none());std::fs::remove_dir_all(s.directory).unwrap();
    }
}
