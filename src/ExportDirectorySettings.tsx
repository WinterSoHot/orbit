import { ArrowDownToLine, FolderOpen, Loader2 } from 'lucide-react';
import type { ExportSettings } from './bridge';

type Props={value:ExportSettings|null;desktop:boolean;busy:boolean;onChoose:()=>void;onReset:()=>void;onExport:()=>void};
export function ExportDirectorySettings({value,desktop,busy,onChoose,onReset,onExport}:Props) {
  return <><div className="settings-line export-directory-setting">
    <div><strong>导出文件夹</strong><p>交付文档和全部数据统一保存到此位置，重启后仍然生效。</p>
      <code className="export-directory-path">{value?.directory||(desktop?'正在读取导出位置…':'在桌面 App 中选择导出文件夹')}</code>
      {value&&<small className="subtle">{value.custom?'自定义位置':'默认位置'}</small>}
    </div>
    <div className="export-directory-actions"><button className="secondary-button" disabled={!desktop||busy||!value} onClick={onChoose}>{busy?<Loader2 className="spin" size={15}/>:<FolderOpen size={15}/>}选择文件夹</button><button className="secondary-button" disabled={!desktop||busy||!value?.custom} onClick={onReset}>恢复默认</button></div>
  </div><div className="settings-line workspace-export-setting">
    <div><strong>导出全部数据</strong><p>全部任务与归档、Agent 记录、交付版本，以及知识库正文、历史、标签、分组和 PDF 附件，保存为一份 JSON。</p><small className="subtle">运行中导出当前快照；未保存的编辑和聊天草稿不包含。</small></div>
    <button className="secondary-button" disabled={!desktop||busy||!value} onClick={onExport}><ArrowDownToLine size={15}/>导出全部数据</button>
  </div></>;
}
