import {useId,useState} from 'react';
import {Send} from 'lucide-react';
import type {Approval,InputQuestion} from './model';

type Props={approval:Approval;busy:boolean;onAnswer:(answers:Record<string,string>)=>void};
export function RealApprovalForm(props:Props) {
  return <ClarificationForm key={`${props.approval.id}:${props.approval.runId}:${props.approval.turnId}`} {...props}/>;
}
function ClarificationForm({approval,busy,onAnswer}:Props) {
  const formId=useId();
  const [texts,setTexts]=useState<Record<string,string>>({}),[choices,setChoices]=useState<Record<string,number|'other'>>({});
  const questions:InputQuestion[]=approval.questionIds.map((id,i)=>approval.questions?.find(q=>q.id===id)||{id,question:`答复 ${i+1}`});
  const mismatch=!!approval.questions?.length&&(approval.questions.length!==questions.length||approval.questionIds.some(id=>!approval.questions!.some(q=>q.id===id)));
  const error=approval.questionError||(mismatch||new Set(approval.questionIds).size!==approval.questionIds.length?'澄清问题数据不完整，请重新发起请求':null);
  const textValue=(id:string)=>Object.hasOwn(texts,id)?texts[id]:'';
  const choiceValue=(id:string)=>Object.hasOwn(choices,id)?choices[id]:undefined;
  const value=(q:InputQuestion)=>{const choice=choiceValue(q.id);return q.options?.length?(typeof choice==='number'?q.options[choice]?.label||'':choice==='other'?textValue(q.id):''):textValue(q.id);};
  const invalid=!!error||questions.some(q=>q.isSecret?!value(q).length:!value(q).trim());
  return <form className="real-approval-form clarification-form" onSubmit={event=>{event.preventDefault();if(!busy&&!invalid)onAnswer(Object.fromEntries(questions.map(q=>[q.id,value(q)])));}}>
    {error?<p className="clarification-error" role="alert">{error}</p>:questions.map((q,i)=>{
      const options=q.options||[],custom=!options.length||choiceValue(q.id)==='other';
      return <fieldset key={q.id} disabled={busy} className="clarification-question"><legend>{q.header&&<span className="clarification-header">{q.header}</span>}{q.question}</legend>
        {!!options.length&&<div className="clarification-options">{options.map((option,index)=><label className="clarification-option" key={index}><input type="radio" name={`${formId}-${q.id}`} checked={choiceValue(q.id)===index} onChange={()=>setChoices(current=>({...current,[q.id]:index}))}/><span><strong>{option.label}</strong>{option.description&&<small>{option.description}</small>}</span></label>)}{q.isOther&&<label className="clarification-option"><input type="radio" name={`${formId}-${q.id}`} checked={choiceValue(q.id)==='other'} onChange={()=>setChoices(current=>({...current,[q.id]:'other'}))}/><span><strong>其他</strong><small>填写自己的回答</small></span></label>}</div>}
        {custom&&<label className="clarification-custom">{options.length?'自定义答复':q.isSecret?'私密答复':'你的答复'}{q.isSecret?<input type="password" aria-label={approval.questions?.length?q.question:`答复 ${i+1}`} autoComplete="off" value={textValue(q.id)} onChange={e=>setTexts(current=>({...current,[q.id]:e.target.value}))} maxLength={2000} required/>:<textarea aria-label={approval.questions?.length?q.question:`答复 ${i+1}`} placeholder="填写你的回答…" value={textValue(q.id)} onChange={e=>setTexts(current=>({...current,[q.id]:e.target.value}))} maxLength={2000} rows={2} required/>}</label>}
      </fieldset>;
    })}
    <div className="clarification-footer"><span>答复将发送给当前执行器</span><button className="approval-primary" disabled={busy||invalid}><Send size={13}/>提交答复</button></div>
  </form>;
}
