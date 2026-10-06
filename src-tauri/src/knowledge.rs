use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Markdown,
    Pdf,
    Web,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    pub content: String,
    pub at: u64,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub content: String,
    pub at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    pub session_id: String,
    pub sequence: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub tags: Vec<String>,
    pub url: Option<String>,
    pub content: String,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
    pub draft: Option<Draft>,
    pub versions: Vec<Version>,
    pub blob_id: Option<String>,
    pub size_bytes: u64,
    pub stamp: Option<Stamp>,
    #[serde(default)]
    pub collection_ids: Vec<String>,
    #[serde(default)]
    pub deleted_at: Option<u64>,
    #[serde(default)]
    pub pdf_reader: Option<PdfReaderData>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdfAnnotation {
    pub id: String, pub page: u32, pub rects: Vec<[f64;4]>,
    pub text: String, pub comment: String, pub color: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdfReaderData {
    pub revision: u64, pub page: u32, pub scale: f64, pub annotations: Vec<PdfAnnotation>,
}
impl Default for PdfReaderData {
    fn default()->Self { Self {revision:0,page:1,scale:1.,annotations:vec![]} }
}
fn validate_reader(value:&PdfReaderData)->Result<(),String> {
    if value.page==0 || value.page>100000 || !value.scale.is_finite() || !(0.25..=4.).contains(&value.scale) || value.annotations.len()>500 || value.annotations.iter().map(|a|a.rects.len()).sum::<usize>()>10000 {
        return Err("PDF 阅读状态超限或无效".into());
    }
    let mut ids=std::collections::HashSet::new();
    for a in &value.annotations {
        if uuid::Uuid::parse_str(&a.id).is_err() || !ids.insert(&a.id) || a.page==0 || a.page>100000
            || a.rects.is_empty() || a.rects.len()>256 || a.text.len()>16384 || a.comment.len()>16384
            || !["yellow","blue","pink"].contains(&a.color.as_str())
            || a.rects.iter().any(|r| r.iter().any(|v| !v.is_finite() || v.abs()>10000000.) || r[0]>=r[2] || r[1]>=r[3]) {
            return Err("PDF 批注超限或无效".into());
        }
    }
    Ok(())
}
fn invalidate_reader(d:&mut Document)->Result<(),String> {
    if d.kind==Kind::Pdf { let value=d.pdf_reader.get_or_insert_with(PdfReaderData::default);value.revision=value.revision.checked_add(1).ok_or("PDF 阅读版本号超限")?; }
    Ok(())
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartGroup {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub kind: Option<Kind>,
    pub tag: String,
    pub keyword: String,
    pub updated_days: Option<u32>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub revision: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub documents: Vec<Document>,
    pub groups: Vec<SmartGroup>,
    pub collections: Vec<Collection>,
    pub schema_version: u32,
    pub error: Option<String>,
}
#[derive(Serialize)]
pub struct LibraryExport {
    pub records: Library,
    pub attachments: HashMap<String, String>,
}
#[derive(Debug, Deserialize)]
pub struct NewDocument {
    pub title: String,
    pub kind: Kind,
    pub content: String,
    pub url: Option<String>,
    pub tags: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub document_id: String,
    pub expected_revision: u64,
    pub session_id: String,
    pub sequence: u64,
    pub operation: String,
    pub content: String,
    pub version_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}
const MAX_TEXT: usize = 512 * 1024;
const MAX_METADATA: u64 = 16 * 1024 * 1024;
const MAX_PDF: u64 = 20 * 1024 * 1024;
const MAX_ATTACHMENTS: u64 = 64 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Disk {
    version: u32,
    documents: Vec<Document>,
    groups: Vec<SmartGroup>,
    #[serde(default)]
    collections: Vec<Collection>,
}
pub struct LibraryStore {
    directory: PathBuf,
    data: std::sync::Mutex<Library>,
}
impl LibraryStore {
    pub fn open(directory: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&directory).map_err(|_| "无法创建知识库目录")?;
        let store = Self {
            directory,
            data: std::sync::Mutex::new(Library::default()),
        };
        let loaded = (|| -> Result<Library, String> {
            let path = store.directory.join("library.json");
            let mut options = std::fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            let file = match options.open(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Library { schema_version: 3, ..Library::default() }),
                Err(_) => return Err("知识库记录无法读取".into()),
            };
            if !file.metadata().map_err(|_| "记录无法核对")?.is_file() {
                return Err("知识库记录不是普通文件".into());
            }
            use std::io::Read;
            let mut bytes = Vec::new();
            file.take(MAX_METADATA + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "记录读取失败")?;
            if bytes.len() as u64 > MAX_METADATA {
                return Err("知识库记录超过 16MiB".into());
            }
            let disk: Disk = serde_json::from_slice(&bytes).map_err(|_| "知识库格式损坏")?;
            if !matches!(disk.version, 1 | 2 | 3) {
                return Err("知识库版本不受支持".into());
            }
            // V2 required fields cannot silently fall back to V1 defaults.
            if disk.version >= 2 {
                let raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "知识库格式损坏")?;
                if !raw.get("collections").is_some_and(|v| v.is_array())
                    || !raw["documents"].as_array().is_some_and(|docs| docs.iter().all(|d|
                        d.get("collectionIds").is_some_and(|v| v.is_array()) && d.get("deletedAt").is_some())) {
                    return Err("知识库 V2 字段缺失".into());
                }
            } else if !disk.collections.is_empty() || disk.documents.iter().any(|d| !d.collection_ids.is_empty() || d.deleted_at.is_some()) {
                return Err("旧知识库包含不支持的字段".into());
            }
            if disk.version<3 && disk.documents.iter().any(|d| d.pdf_reader.is_some()) { return Err("旧知识库包含不支持的 PDF 阅读字段".into()); }
            if disk.version==3 {
                let raw:serde_json::Value=serde_json::from_slice(&bytes).map_err(|_|"知识库格式损坏")?;
                if !raw["documents"].as_array().is_some_and(|docs|docs.iter().all(|d|d.get("pdfReader").is_some())) {return Err("知识库 V3 字段缺失".into());}
            }
            let data = Library {
                documents: disk.documents,
                groups: disk.groups,
                collections: disk.collections,
                schema_version: 3,
                error: None,
            };
            encode(&data)?;
            for d in &data.documents {
                if d.kind == Kind::Pdf {
                    store.pdf_bytes(d)?;
                }
            }
            Ok(data)
        })();
        *store.data.lock().unwrap() = loaded.unwrap_or_else(|reason| Library {
            error: Some(format!("{reason}；原文件保留，知识库暂不可写")),
            ..Library::default()
        });
        Ok(store)
    }
    pub fn view(&self) -> Library {
        self.data.lock().unwrap().clone()
    }
    pub fn snapshot(&self) -> Result<Library, String> {
        let data = self.view();
        if let Some(error) = &data.error {
            return Err(error.clone());
        }
        Ok(data)
    }
    pub fn get(&self, id: &str) -> Result<Document, String> {
        let data = self.data.lock().unwrap();
        if let Some(error) = &data.error {
            return Err(error.clone());
        }
        data.documents
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| "文档不存在".into())
    }
    fn transaction<T>(
        &self,
        edit: impl FnOnce(&mut Library) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut data = self.data.lock().unwrap();
        if let Some(error) = &data.error {
            return Err(error.clone());
        }
        let mut next = data.clone();
        let result = edit(&mut next)?;
        let bytes = encode(&next)?;
        self.persist(&bytes)?;
        *data = next;
        Ok(result)
    }
    fn persist(&self, bytes: &[u8]) -> Result<(), String> {
        use std::io::Write;
        let temporary = self
            .directory
            .join(format!("library-{}.tmp", uuid::Uuid::new_v4()));
        let write = (|| -> Result<(), String> {
            let mut options = std::fs::OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&temporary)
                .map_err(|_| "知识库无法写入临时记录")?;
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| "知识库写入失败，当前文本保留")?;
            std::fs::rename(&temporary, self.directory.join("library.json"))
                .map_err(|_| "知识库替换失败，原记录保留")?;
            Ok(())
        })();
        if write.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        write
    }
    pub fn create(&self, input: NewDocument) -> Result<Document, String> {
        if input.kind == Kind::Pdf {
            return Err("PDF 请通过本机导入".into());
        }
        let title = title(&input.title)?;
        let tags = tags(input.tags)?;
        check_text(&input.content)?;
        let url = input.url.as_deref().map(valid_url).transpose()?;
        if input.kind == Kind::Web && url.is_none() {
            return Err("网页需要来源 URL".into());
        }
        let now = crate::model::now();
        let d = Document {
            id: uuid::Uuid::new_v4().to_string(),
            kind: input.kind,
            title,
            tags,
            url,
            content: input.content,
            revision: 1,
            created_at: now,
            updated_at: now,
            draft: None,
            versions: vec![],
            blob_id: None,
            size_bytes: 0,
            stamp: None,
            collection_ids: vec![],
            deleted_at: None,
            pdf_reader: None,
        };
        self.transaction(|data| {
            data.documents.insert(0, d.clone());
            Ok(d)
        })
    }
    pub fn import_path(&self, path: &Path) -> Result<Document, String> {
        use std::io::Read;
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "pdf" | "md" | "markdown" | "txt") {
            return Err("只支持 PDF、Markdown 和 UTF-8 文本文件".into());
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options.open(path).map_err(|_| "导入源无法读取")?;
        if !file.metadata().map_err(|_| "导入源无法核对")?.is_file() {
            return Err("请选择普通文件".into());
        }
        let limit = if extension == "pdf" {
            MAX_PDF
        } else {
            MAX_TEXT as u64
        };
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "导入读取失败")?;
        if bytes.len() as u64 > limit {
            return Err("文件超过限制：PDF 20MiB，正文 512KiB".into());
        }
        let name = path
            .file_stem()
            .and_then(|p| p.to_str())
            .unwrap_or("导入文档")
            .chars()
            .take(128)
            .collect::<String>();
        if extension != "pdf" {
            let content =
                String::from_utf8(bytes).map_err(|_| "文本不是 UTF-8，请转换编码后导入")?;
            return self.create(NewDocument {
                title: name,
                kind: Kind::Markdown,
                content,
                url: None,
                tags: vec![],
            });
        }
        if !bytes.starts_with(b"%PDF-") {
            return Err("不是有效 PDF 文件".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = crate::model::now();
        let d = Document {
            id: id.clone(),
            kind: Kind::Pdf,
            title: title(&name)?,
            tags: vec![],
            url: None,
            content: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft: None,
            versions: vec![],
            blob_id: Some(id.clone()),
            size_bytes: bytes.len() as u64,
            stamp: None,
            collection_ids: vec![],
            deleted_at: None,
            pdf_reader: None,
        };
        self.write_blob(&id, &bytes)?;
        let result = self.transaction(|data| {
            data.documents.insert(0, d.clone());
            Ok(d)
        });
        if result.is_err() {
            self.remove_new_blob(&id);
        }
        result
    }
    pub fn change(&self, change: Change) -> Result<Document, String> {
        check_text(&change.content)?;
        if change.session_id.is_empty() || change.session_id.len() > 80 || change.sequence == 0 {
            return Err("编辑请求无效".into());
        }
        self.transaction(|data| {
            let d = data
                .documents
                .iter_mut()
                .find(|d| d.id == change.document_id)
                .ok_or("文档不存在")?;
            if d.deleted_at.is_some() { return Err("回收站文档只读，请先恢复".into()); }
            if d.revision != change.expected_revision {
                return Err("文档已被其他操作更新，当前草稿保留；请核对最新文档".into());
            }
            if d.stamp
                .as_ref()
                .is_some_and(|s| s.session_id == change.session_id && s.sequence >= change.sequence)
            {
                return Err("旧编辑请求已失效".into());
            }
            let now = crate::model::now().max(d.updated_at);
            match change.operation.as_str() {
                "draft" => {
                    d.draft = (change.content != d.content).then(|| Draft {
                        content: change.content,
                        at: now,
                    });
                }
                "commit" | "automatic" => {
                    if change.content != d.content {
                        let bucket_exists = change.operation == "automatic"
                            && d.versions.last().is_some_and(|v| {
                                v.reason == "automatic" && now.saturating_sub(v.at) < 30_000
                            });
                        if !bucket_exists {
                            push_version(d, d.content.clone(), now, &change.operation);
                        }
                        d.content = change.content;
                    }
                    d.draft = None;
                }
                "restore" => {
                    let id = change.version_id.as_deref().ok_or("请选择历史版本")?;
                    let restored = d
                        .versions
                        .iter()
                        .find(|v| v.id == id)
                        .ok_or("历史版本不存在")?
                        .content
                        .clone();
                    push_version(d, change.content, now, "restore");
                    d.content = restored;
                    d.draft = None;
                }
                "metadata" => {
                    d.title = title(change.title.as_deref().ok_or("缺少标题")?)?;
                    d.tags = tags(change.tags.clone().ok_or("缺少标签")?)?;
                }
                _ => return Err("未知编辑操作".into()),
            }
            d.revision = d.revision.checked_add(1).ok_or("文档版本号超限")?;
            d.updated_at = now;
            d.stamp = Some(Stamp {
                session_id: change.session_id,
                sequence: change.sequence,
            });
            Ok(d.clone())
        })
    }
    pub fn trash(&self, id: &str, revision: u64) -> Result<Document, String> {
        self.transaction(|data| {
            let d = current_document(data, id, revision)?;
            if d.deleted_at.is_some() { return Err("文档已在回收站".into()); }
            invalidate_reader(d)?;
            d.deleted_at = Some(crate::model::now());
            bump(d)?;
            Ok(d.clone())
        })
    }
    pub fn restore_document(&self, id: &str, revision: u64) -> Result<Document, String> {
        self.transaction(|data| {
            let d = current_document(data, id, revision)?;
            if d.deleted_at.is_none() { return Err("文档不在回收站".into()); }
            invalidate_reader(d)?;
            d.deleted_at = None;
            bump(d)?;
            Ok(d.clone())
        })
    }
    pub fn save_pdf_reader(&self,id:&str,mut value:PdfReaderData)->Result<PdfReaderData,String> {
        validate_reader(&value)?;
        self.transaction(|data| {
            let doc=data.documents.iter_mut().find(|d|d.id==id).ok_or("文档不存在")?;
            if doc.kind!=Kind::Pdf || doc.deleted_at.is_some() {return Err("PDF 不存在或位于回收站，无法保存批注".into());}
            if doc.pdf_reader.as_ref().map_or(0,|v|v.revision)!=value.revision {return Err("PDF 批注已更新，请重新打开文档；当前输入保留".into());}
            value.revision=value.revision.checked_add(1).ok_or("PDF 阅读版本号超限")?;
            doc.pdf_reader=Some(value.clone());
            Ok(value)
        })
    }
    pub fn organize(&self, id: &str, revision: u64, ids: Vec<String>) -> Result<Document, String> {
        self.transaction(|data| {
            if ids.len() > 100 || ids.iter().any(|id| !data.collections.iter().any(|c| c.id == *id)) {
                return Err("分类不存在或超限".into());
            }
            let mut unique = std::collections::HashSet::new();
            if ids.iter().any(|id| !unique.insert(id)) { return Err("分类重复".into()); }
            let d = current_document(data, id, revision)?;
            if d.deleted_at.is_some() { return Err("回收站文档只读，请先恢复".into()); }
            d.collection_ids = ids;
            bump(d)?;
            Ok(d.clone())
        })
    }
    pub fn save_collection(&self, mut c: Collection) -> Result<Collection, String> {
        c.name = title(&c.name)?;
        self.transaction(|data| {
            if c.id.is_empty() {
                c.id = uuid::Uuid::new_v4().to_string(); c.revision = 1;
                data.collections.push(c.clone());
            } else {
                let old = data.collections.iter_mut().find(|old| old.id == c.id).ok_or("分类不存在")?;
                if old.revision != c.revision { return Err("分类已更新，请重新打开".into()); }
                c.revision = c.revision.checked_add(1).ok_or("分类版本号超限")?;
                *old = c.clone();
            }
            Ok(c)
        })
    }
    pub fn delete_collection(&self, id: &str, revision: u64) -> Result<Library, String> {
        self.transaction(|data| {
            let c = data.collections.iter().find(|c| c.id == id).ok_or("分类不存在")?;
            if c.revision != revision { return Err("分类已更新，请重新打开".into()); }
            let mut removed = std::collections::HashSet::from([id.to_string()]);
            loop {
                let before = removed.len();
                for c in &data.collections {
                    if c.parent_id.as_ref().is_some_and(|p| removed.contains(p)) { removed.insert(c.id.clone()); }
                }
                if before == removed.len() { break; }
            }
            data.collections.retain(|c| !removed.contains(&c.id));
            for d in &mut data.documents {
                if d.collection_ids.iter().any(|id| removed.contains(id)) {
                    d.collection_ids.retain(|id| !removed.contains(id)); bump(d)?;
                }
            }
            Ok(data.clone())
        })
    }
    pub fn purge(&self, id: &str, revision: u64) -> Result<Option<String>, String> {
        let pdf = self.transaction(|data| {
            let d = current_document(data, id, revision)?;
            if d.deleted_at.is_none() { return Err("请先移入回收站".into()); }
            let pdf = d.kind == Kind::Pdf;
            data.documents.retain(|d| d.id != id);
            Ok(pdf)
        })?;
        if !pdf { return Ok(None); }
        // The metadata rename must be durable before we remove its attachment.
        if std::fs::File::open(&self.directory).and_then(|f| f.sync_all()).is_err() {
            return Ok(Some("资料已删除，目录同步失败；库内附件保留".into()));
        }
        Ok(self.remove_blob(id).err().map(|e| format!("资料已删除；{e}")))
    }
    pub fn directory(&self) -> &Path { &self.directory }
    pub fn attachment_path(&self, id: &str) -> Result<PathBuf, String> {
        let d = self.get(id)?; self.pdf_bytes(&d)?;
        Ok(self.directory.join("library-blobs").join(format!("{id}.pdf")))
    }
    pub fn save_group(&self, mut group: SmartGroup) -> Result<SmartGroup, String> {
        group.name = title(&group.name)?;
        group.tag = group.tag.trim().to_lowercase();
        group.keyword = group.keyword.trim().to_string();
        if group.tag.chars().count() > 60
            || group.keyword.chars().count() > 200
            || group.updated_days.is_some_and(|n| n == 0 || n > 3650)
        {
            return Err("分组条件超出范围".into());
        }
        self.transaction(|data| {
            if group.id.is_empty() {
                group.id = uuid::Uuid::new_v4().to_string();
                group.revision = 1;
                data.groups.push(group.clone());
            } else {
                let old = data
                    .groups
                    .iter_mut()
                    .find(|g| g.id == group.id)
                    .ok_or("分组不存在")?;
                if old.revision != group.revision {
                    return Err("分组已更新，请重新打开".into());
                }
                group.revision = group.revision.checked_add(1).ok_or("分组版本号超限")?;
                *old = group.clone();
            }
            Ok(group)
        })
    }
    pub fn export_snapshot(&self, data: &Library) -> Result<LibraryExport, String> {
        if let Some(error) = &data.error {
            return Err(error.clone());
        }
        let mut attachments = HashMap::new();
        use base64::Engine;
        for d in &data.documents {
            if d.kind == Kind::Pdf {
                attachments.insert(
                    d.id.clone(),
                    base64::engine::general_purpose::STANDARD.encode(self.pdf_bytes(d)?),
                );
            }
        }
        Ok(LibraryExport {
            records: data.clone(),
            attachments,
        })
    }
    pub fn pdf_base64(&self, id: &str) -> Result<String, String> {
        use base64::Engine;
        let d = self.get(id)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(self.pdf_bytes(&d)?))
    }
    pub fn document_bytes(&self, id: &str) -> Result<(String, Vec<u8>), String> {
        let d = self.get(id)?;
        if d.kind == Kind::Pdf {
            Ok((format!("{}.pdf", d.title), self.pdf_bytes(&d)?))
        } else {
            Ok((format!("{}.md", d.title), d.content.into_bytes()))
        }
    }
    fn pdf_bytes(&self, d: &Document) -> Result<Vec<u8>, String> {
        use std::io::Read;
        if d.kind != Kind::Pdf
            || d.blob_id.as_deref() != Some(d.id.as_str())
            || uuid::Uuid::parse_str(&d.id).is_err()
        {
            return Err("PDF 附件引用无效".into());
        }
        let file = self.blob_file(&d.id, false)?;
        if !file.metadata().map_err(|_| "PDF 无法核对")?.is_file() {
            return Err("PDF 附件不是普通文件".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_PDF + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "PDF 附件读取失败")?;
        if bytes.len() as u64 != d.size_bytes
            || bytes.len() as u64 > MAX_PDF
            || !bytes.starts_with(b"%PDF-")
        {
            return Err("PDF 附件缺失、截断或超限，原记录保留".into());
        }
        Ok(bytes)
    }
    #[cfg(unix)]
    fn blob_root(&self) -> Result<std::fs::File, String> {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(self.directory.join("library-blobs"))
            .map_err(|_| "PDF 存储目录无法访问或已变化".into())
    }
    fn blob_file(&self, id: &str, create: bool) -> Result<std::fs::File, String> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err("附件 ID 无效".into());
        }
        if create {
            std::fs::create_dir_all(self.directory.join("library-blobs"))
                .map_err(|_| "无法创建 PDF 目录")?;
        }
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let root = self.blob_root()?;
            let name = std::ffi::CString::new(format!("{id}.pdf")).unwrap();
            let flags = if create {
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC
            } else {
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC
            };
            let fd = unsafe { libc::openat(root.as_raw_fd(), name.as_ptr(), flags, 0o600) };
            if fd < 0 {
                return Err("PDF 附件无法访问或已变化".into());
            }
            Ok(unsafe { std::fs::File::from_raw_fd(fd) })
        }
        #[cfg(not(unix))]
        {
            let path = self
                .directory
                .join("library-blobs")
                .join(format!("{id}.pdf"));
            if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("附件路径已变化".into());
            }
            std::fs::OpenOptions::new()
                .read(!create)
                .write(create)
                .create_new(create)
                .open(path)
                .map_err(|_| "PDF 附件无法访问".into())
        }
    }
    fn write_blob(&self, id: &str, bytes: &[u8]) -> Result<(), String> {
        use std::io::Write;
        let mut file = self.blob_file(id, true)?;
        let result = file
            .write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "PDF 保存失败".to_string());
        if result.is_err() {
            drop(file);
            self.remove_new_blob(id);
        }
        result
    }
    fn remove_new_blob(&self, id: &str) { let _ = self.remove_blob(id); }
    fn remove_blob(&self, id: &str) -> Result<(), String> {
        if uuid::Uuid::parse_str(id).is_err() { return Err("附件 ID 无效".into()); }
        #[cfg(unix)]
        let result = {
            use std::os::fd::AsRawFd;
            let root = self.blob_root()?;
            let name = std::ffi::CString::new(format!("{id}.pdf")).unwrap();
            if unsafe { libc::unlinkat(root.as_raw_fd(), name.as_ptr(), 0) } == 0 { Ok(()) }
            else { Err(std::io::Error::last_os_error()) }
        };
        #[cfg(not(unix))]
        let result = std::fs::remove_file(self.directory.join("library-blobs").join(format!("{id}.pdf")));
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("库内附件清理失败，残留附件仍在存储目录".into()),
        }
    }

}
fn current_document<'a>(data: &'a mut Library, id: &str, revision: u64) -> Result<&'a mut Document, String> {
    let d = data.documents.iter_mut().find(|d| d.id == id).ok_or("文档不存在")?;
    if d.revision != revision { return Err("文档已更新，请重新打开后操作".into()); }
    Ok(d)
}
fn bump(d: &mut Document) -> Result<(), String> {
    d.revision = d.revision.checked_add(1).ok_or("文档版本号超限")?;
    d.updated_at = crate::model::now().max(d.updated_at);
    Ok(())
}
fn check_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_TEXT {
        return Err("正文或草稿超过 512KiB".into());
    }
    Ok(())
}
fn title(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 128 {
        return Err("标题需要 1–128 个字符".into());
    }
    Ok(value.into())
}
fn tags(values: Vec<String>) -> Result<Vec<String>, String> {
    if values.len() > 24 {
        return Err("最多 24 个标签".into());
    }
    let mut result = Vec::new();
    for raw in values {
        let tag = raw.trim().trim_start_matches('#').to_lowercase();
        if tag.is_empty() || tag.chars().count() > 60 {
            return Err("标签需要 1–60 个字符".into());
        }
        if !result.contains(&tag) {
            result.push(tag);
        }
    }
    Ok(result)
}
fn push_version(d: &mut Document, content: String, at: u64, reason: &str) {
    d.versions.push(Version {
        id: uuid::Uuid::new_v4().to_string(),
        content,
        at,
        reason: reason.into(),
    });
    if d.versions.len() > 20 {
        d.versions.remove(0);
    }
}
fn encode(data: &Library) -> Result<Vec<u8>, String> {
    if data.documents.len() > 100 || data.groups.len() > 50 {
        return Err("知识库最多 100 篇文档、50 个智能分组".into());
    }
    if data.collections.len() > 100 { return Err("最多 100 个分类".into()); }
    let mut collection_ids = std::collections::HashSet::new();
    for c in &data.collections {
        if uuid::Uuid::parse_str(&c.id).is_err() || !collection_ids.insert(&c.id) || c.revision == 0 || title(&c.name)? != c.name {
            return Err("分类记录无效".into());
        }
    }
    for c in &data.collections {
        let mut visited = std::collections::HashSet::new();
        let mut node = Some(c);
        while let Some(current) = node {
            if !visited.insert(&current.id) || visited.len() > 8 { return Err("分类不能循环，最多 8 层".into()); }
            node = match &current.parent_id {
                Some(id) => Some(data.collections.iter().find(|p| p.id == *id).ok_or("父分类不存在")?),
                None => None,
            };
        }
    }
    let mut ids = std::collections::HashSet::new();
    let mut attachments = 0u64;
    for d in &data.documents {
        if uuid::Uuid::parse_str(&d.id).is_err()
            || !ids.insert(&d.id)
            || d.revision == 0
            || title(&d.title)? != d.title
            || tags(d.tags.clone())? != d.tags
            || d.updated_at < d.created_at
        {
            return Err("文档记录无效".into());
        }
        let mut membership = std::collections::HashSet::new();
        if d.collection_ids.iter().any(|id| !collection_ids.contains(id) || !membership.insert(id)) {
            return Err("文档分类引用无效".into());
        }
        if let Some(reader)=&d.pdf_reader {
            if d.kind!=Kind::Pdf || reader.revision==0 {return Err("PDF 阅读记录无效".into());}
            validate_reader(reader)?;
        }
        check_text(&d.content)?;
        if let Some(draft) = &d.draft {
            check_text(&draft.content)?;
        }
        if d.versions.len() > 20 {
            return Err("历史版本超限".into());
        }
        let mut versions = std::collections::HashSet::new();
        for v in &d.versions {
            check_text(&v.content)?;
            if uuid::Uuid::parse_str(&v.id).is_err() || !versions.insert(&v.id) {
                return Err("历史版本记录无效".into());
            }
        }
        if let Some(url) = &d.url {
            valid_url(url)?;
        }
        if d.kind == Kind::Web && d.url.is_none() {
            return Err("网页缺少来源".into());
        }
        if d.kind == Kind::Pdf {
            if d.blob_id.as_deref() != Some(d.id.as_str())
                || d.size_bytes < 5
                || d.size_bytes > MAX_PDF
            {
                return Err("PDF 记录超限或无效".into());
            }
            attachments = attachments
                .checked_add(d.size_bytes)
                .ok_or("附件大小溢出")?;
        } else if d.blob_id.is_some() || d.size_bytes != 0 {
            return Err("文档附件类型无效".into());
        }
    }
    if attachments > MAX_ATTACHMENTS {
        return Err("PDF 附件总量超过 64MiB".into());
    }
    ids.clear();
    for g in &data.groups {
        if uuid::Uuid::parse_str(&g.id).is_err()
            || !ids.insert(&g.id)
            || g.revision == 0
            || title(&g.name)? != g.name
            || g.tag.chars().count() > 60
            || g.keyword.chars().count() > 200
            || g.updated_days.is_some_and(|n| n == 0 || n > 3650)
        {
            return Err("智能分组记录无效".into());
        }
    }
    let bytes = serde_json::to_vec(&Disk {
        version: 3,
        documents: data.documents.clone(),
        groups: data.groups.clone(),
        collections: data.collections.clone(),
    })
    .map_err(|_| "知识库编码失败")?;
    if bytes.len() as u64 > MAX_METADATA {
        return Err("正文、草稿及历史总量超过 16MiB；此次操作未保存".into());
    }
    Ok(bytes)
}

fn valid_url(raw: &str) -> Result<String, String> {
    if raw.len() > 4096 {
        return Err("网址过长".into());
    }
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| "请输入有效 HTTP/HTTPS 网址")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("仅支持不含登录凭据的 HTTP/HTTPS 网址".into());
    }
    Ok(url.to_string())
}
fn parse_web(url: &str, html: &str) -> Result<NewDocument, String> {
    let url = valid_url(url)?;
    let parsed = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("article, main").unwrap();
    let body = scraper::Selector::parse("body").unwrap();
    let root = parsed
        .select(&selector)
        .next()
        .or_else(|| parsed.select(&body).next())
        .ok_or("网页没有可读正文")?;
    let mut content = String::new();
    for node in root.descendants() {
        if let scraper::Node::Text(text) = node.value() {
            let excluded = node.ancestors().any(|a| {
                a.value().as_element().is_some_and(|e| {
                    matches!(
                        e.name(),
                        "script"
                            | "style"
                            | "noscript"
                            | "template"
                            | "svg"
                            | "iframe"
                            | "object"
                            | "form"
                            | "nav"
                            | "aside"
                    )
                })
            });
            let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !excluded && !clean.is_empty() {
                if !content.is_empty() {
                    content.push_str("\n\n");
                }
                content.push_str(&clean);
            }
        }
    }
    if content.is_empty() {
        return Err("网页没有可读静态正文；可手动粘贴内容".into());
    }
    check_text(&content)?;
    let title_selector = scraper::Selector::parse("title").unwrap();
    let title = parsed
        .select(&title_selector)
        .next()
        .map(|e| e.text().collect::<String>())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| url.clone())
        .trim()
        .chars()
        .take(128)
        .collect();
    Ok(NewDocument {
        title,
        kind: Kind::Web,
        content,
        url: Some(url),
        tags: vec![],
    })
}
pub fn fetch_web(url: &str) -> Result<NewDocument, String> {
    use std::io::Read;
    let url = valid_url(url)?;
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if valid_url(attempt.url().as_str()).is_err() {
            attempt.error("重定向网址不受支持")
        } else {
            reqwest::redirect::Policy::limited(5).redirect(attempt)
        }
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(policy)
        .user_agent("Orbit/0.1 (local document library)")
        .build()
        .map_err(|_| "网页连接无法初始化")?;
    let response = client
        .get(url)
        .send()
        .map_err(|_| "网页连接失败或超时；可手动添加正文")?
        .error_for_status()
        .map_err(|_| "网页服务器拒绝请求；可手动添加正文")?;
    let final_url = response.url().to_string();
    let mut bytes = Vec::new();
    response
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "网页正文读取失败或超时")?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("网页超过 4MiB，未保存".into());
    }
    let html = String::from_utf8(bytes).map_err(|_| "网页不是 UTF-8；可手动粘贴正文")?;
    parse_web(&final_url, &html)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("orbit-library-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn note(title: &str, content: &str) -> NewDocument {
        NewDocument {
            title: title.into(),
            content: content.into(),
            kind: Kind::Markdown,
            url: None,
            tags: vec!["研究".into()],
        }
    }
    fn change(d: &Document, seq: u64, op: &str, body: &str, version: Option<String>) -> Change {
        Change {
            document_id: d.id.clone(),
            expected_revision: d.revision,
            session_id: "editor-a".into(),
            sequence: seq,
            operation: op.into(),
            content: body.into(),
            version_id: version,
            title: None,
            tags: None,
        }
    }
    #[test]
    fn pdf_reader_is_durable_independent_and_invalidated_by_lifecycle() {
        let dir=TestDir::new();let source=dir.0.join("source.pdf");
        fs::write(&source,b"%PDF-1.4 reader fixture").unwrap();
        let store=LibraryStore::open(dir.0.join("app")).unwrap();let doc=store.import_path(&source).unwrap();
        let mut reader=PdfReaderData::default();reader.page=2;reader.scale=1.5;
        reader.annotations.push(PdfAnnotation{id:uuid::Uuid::new_v4().to_string(),page:2,rects:vec![[10.,20.,30.,40.]],text:"Selected".into(),comment:"Comment".into(),color:"yellow".into()});
        let saved=store.save_pdf_reader(&doc.id,reader).unwrap();assert_eq!(saved.revision,1);
        let body=store.change(change(&doc,1,"commit","Notes",None)).unwrap();
        assert_eq!(body.pdf_reader.as_ref().unwrap().annotations.len(),1);assert_eq!(body.revision,2);
        let reopened=LibraryStore::open(dir.0.join("app")).unwrap();assert_eq!(reopened.get(&doc.id).unwrap().pdf_reader.unwrap().page,2);
        assert_eq!(reopened.export_snapshot(&reopened.view()).unwrap().records.documents[0].pdf_reader.as_ref().unwrap().annotations[0].comment,"Comment");
        let mut invalid=saved.clone();invalid.annotations[0].rects[0][2]=f64::NAN;
        assert!(store.save_pdf_reader(&doc.id,invalid).is_err());
        let trash=store.trash(&doc.id,body.revision).unwrap();assert!(store.save_pdf_reader(&doc.id,saved.clone()).is_err());
        let restored=store.restore_document(&doc.id,trash.revision).unwrap();
        assert!(restored.pdf_reader.as_ref().unwrap().revision>saved.revision);assert!(store.save_pdf_reader(&doc.id,saved).is_err());
        assert_eq!(fs::read(&source).unwrap(),b"%PDF-1.4 reader fixture");
        let mut current=restored.pdf_reader.unwrap();current.annotations.clear();store.save_pdf_reader(&doc.id,current).unwrap();
        assert!(store.save_pdf_reader(&uuid::Uuid::new_v4().to_string(),PdfReaderData::default()).is_err());
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&fs::read(dir.0.join("app/library.json")).unwrap()).unwrap()["version"],3);
    }
    #[test]
    fn pdf_reader_failures_keep_records_and_empty_lifecycle_invalidates_requests() {
        let dir=TestDir::new();let source=dir.0.join("pdf.pdf");fs::write(&source,b"%PDF-1.4 fixture").unwrap();
        let l=LibraryStore::open(dir.0.join("app")).unwrap();let d=l.import_path(&source).unwrap();
        let trash=l.trash(&d.id,d.revision).unwrap();let restored=l.restore_document(&d.id,trash.revision).unwrap();
        assert!(l.save_pdf_reader(&d.id,PdfReaderData::default()).is_err());
        let before=restored.pdf_reader.unwrap();let path=dir.0.join("app/library.json");let bytes=fs::read(&path).unwrap();
        fs::rename(&path,dir.0.join("preserved")).unwrap();fs::create_dir(&path).unwrap();
        let mut edit=before.clone();edit.page=3;assert!(l.save_pdf_reader(&d.id,edit).is_err());
        assert_eq!(l.get(&d.id).unwrap().pdf_reader.unwrap().page,before.page);assert_eq!(fs::read(dir.0.join("preserved")).unwrap(),bytes);
        fs::remove_dir(&path).unwrap();let mut raw:serde_json::Value=serde_json::from_slice(&bytes).unwrap();raw["documents"][0]["pdfReader"]["revision"]=u64::MAX.into();
        fs::write(&path,serde_json::to_vec(&raw).unwrap()).unwrap();let exhausted=LibraryStore::open(dir.0.join("app")).unwrap();
        assert!(exhausted.view().error.is_none());assert!(exhausted.save_pdf_reader(&d.id,exhausted.get(&d.id).unwrap().pdf_reader.unwrap()).is_err());assert!(exhausted.view().error.is_none());
        raw["documents"][0]["pdfReader"]["annotations"]=serde_json::json!([{ "id":uuid::Uuid::new_v4().to_string(),"page":1,"rects":[[1.,1.,0.,0.]],"text":"x","comment":"","color":"yellow" }]);
        fs::write(&path,serde_json::to_vec(&raw).unwrap()).unwrap();let damaged=LibraryStore::open(dir.0.join("app")).unwrap();assert!(damaged.view().error.is_some());assert!(damaged.save_pdf_reader(&d.id,PdfReaderData::default()).is_err());
    }
    #[test]
    fn durable_imports_export_pdf_and_utf8_without_modifying_sources() {
        let dir = TestDir::new();
        let source = dir.0.join("来源.pdf");
        let pdf = b"%PDF-1.4\nfixture\n%%EOF";
        fs::write(&source, pdf).unwrap();
        let library = LibraryStore::open(dir.0.join("app")).unwrap();
        let d = library.import_path(&source).unwrap();
        assert_eq!(d.kind, Kind::Pdf);
        let md = dir.0.join("笔记.md");
        fs::write(&md, "# 中文\n保留正文").unwrap();
        let n = library.import_path(&md).unwrap();
        assert_eq!(n.content, "# 中文\n保留正文");
        let exported = library.export_snapshot(&library.view()).unwrap();
        use base64::Engine;
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&exported.attachments[&d.id])
                .unwrap(),
            pdf
        );
        assert_eq!(fs::read(&source).unwrap(), pdf);
        let reopened = LibraryStore::open(dir.0.join("app")).unwrap();
        assert_eq!(reopened.get(&n.id).unwrap().content, n.content);
    }
    #[test]
    fn trash_preserves_history_restore_allows_edit_and_purge_requires_current_revision() {
        let dir=TestDir::new();let l=LibraryStore::open(dir.0.clone()).unwrap();
        let original=l.create(note("target","first")).unwrap();
        let changed=l.change(change(&original,1,"commit","second",None)).unwrap();
        let target=l.change(change(&changed,2,"draft","visible draft",None)).unwrap();
        let link=format!("[target](orbit://document/{})",target.id);
        let other=l.create(note("reference",&link)).unwrap();
        assert!(l.purge(&target.id,target.revision).is_err());
        assert!(l.trash(&target.id,original.revision).is_err());
        let trashed=l.trash(&target.id,target.revision).unwrap();
        assert!(trashed.deleted_at.is_some());assert_eq!(trashed.draft.as_ref().unwrap().content,"visible draft");
        assert_eq!(trashed.versions[0].content,"first");
        assert!(l.change(change(&trashed,3,"draft","late",None)).is_err());
        assert_eq!(l.document_bytes(&target.id).unwrap().1,b"second");
        assert_eq!(l.export_snapshot(&l.view()).unwrap().records.documents.len(),2);
        let restored=l.restore_document(&trashed.id,trashed.revision).unwrap();
        assert!(restored.deleted_at.is_none());
        let edited=l.change(change(&restored,3,"commit","after restore",None)).unwrap();
        let trashed=l.trash(&edited.id,edited.revision).unwrap();
        assert!(l.purge(&trashed.id,target.revision).is_err());
        assert_eq!(l.purge(&trashed.id,trashed.revision).unwrap(),None);
        assert!(l.get(&target.id).is_err());assert_eq!(l.get(&other.id).unwrap().content,link);
        assert_eq!(LibraryStore::open(dir.0.clone()).unwrap().view().documents.len(),1);
    }
    #[test]
    fn pdf_purge_commits_before_cleanup_and_preserves_source() {
        let dir=TestDir::new();let source=dir.0.join("source.pdf");fs::write(&source,b"%PDF-1.4\nfixture").unwrap();
        let l=LibraryStore::open(dir.0.clone()).unwrap();let doc=l.import_path(&source).unwrap();
        let trashed=l.trash(&doc.id,doc.revision).unwrap();let snapshot=l.view();
        let blob=dir.0.join("library-blobs").join(format!("{}.pdf",doc.id));
        assert!(blob.exists());assert!(l.export_snapshot(&snapshot).unwrap().attachments.contains_key(&doc.id));
        let metadata=dir.0.join("library.json");fs::rename(&metadata,dir.0.join("preserved.json")).unwrap();fs::create_dir(&metadata).unwrap();
        assert!(l.purge(&doc.id,trashed.revision).is_err());assert!(l.get(&doc.id).is_ok());assert!(blob.exists());
        fs::remove_dir(&metadata).unwrap();fs::rename(dir.0.join("preserved.json"),&metadata).unwrap();
        assert_eq!(l.purge(&doc.id,trashed.revision).unwrap(),None);assert!(!blob.exists());
        assert_eq!(fs::read(&source).unwrap(),b"%PDF-1.4\nfixture");assert!(l.export_snapshot(&snapshot).is_err());
        assert!(l.export_snapshot(&l.view()).unwrap().attachments.is_empty());
    }
    #[test]
    fn collections_allow_multiple_memberships_and_delete_subtree_without_documents() {
        let dir=TestDir::new();let l=LibraryStore::open(dir.0.clone()).unwrap();
        let root=l.save_collection(Collection{name:"Research".into(),..Default::default()}).unwrap();
        let child=l.save_collection(Collection{name:"Notes".into(),parent_id:Some(root.id.clone()),..Default::default()}).unwrap();
        let other=l.save_collection(Collection{name:"Other".into(),..Default::default()}).unwrap();
        let doc=l.create(note("note","body")).unwrap();
        let member=l.organize(&doc.id,doc.revision,vec![child.id.clone(),other.id.clone()]).unwrap();
        assert!(l.organize(&doc.id,doc.revision,vec![]).is_err());
        assert!(l.organize(&doc.id,member.revision,vec!["missing".into()]).is_err());
        let trashed=l.trash(&doc.id,member.revision).unwrap();
        assert!(l.organize(&doc.id,trashed.revision,vec![]).is_err());
        let mut cycle=root.clone();cycle.parent_id=Some(child.id.clone());assert!(l.save_collection(cycle).is_err());
        let updated=l.delete_collection(&root.id,root.revision).unwrap();
        assert_eq!(updated.collections.len(),1);let d=l.get(&doc.id).unwrap();
        assert_eq!(d.collection_ids,vec![other.id]);assert!(d.revision>trashed.revision);assert_eq!(d.content,"body");
        assert!(l.delete_collection(&child.id,child.revision).is_err());
        assert_eq!(LibraryStore::open(dir.0.clone()).unwrap().view().collections.len(),1);
    }
    #[test]
    fn legacy_records_upgrade_on_write_and_incomplete_v2_is_not_overwritten() {
        let dir=TestDir::new();let l=LibraryStore::open(dir.0.clone()).unwrap();let d=l.create(note("legacy","body")).unwrap();
        let path=dir.0.join("library.json");let mut value:serde_json::Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["version"]=1.into();value.as_object_mut().unwrap().remove("collections");
        for doc in value["documents"].as_array_mut().unwrap(){doc.as_object_mut().unwrap().remove("collectionIds");doc.as_object_mut().unwrap().remove("deletedAt");}
        fs::write(&path,serde_json::to_vec(&value).unwrap()).unwrap();
        let legacy=LibraryStore::open(dir.0.clone()).unwrap();assert!(legacy.view().error.is_none());assert_eq!(legacy.get(&d.id).unwrap().content,"body");
        legacy.trash(&d.id,d.revision).unwrap();let upgraded:serde_json::Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(upgraded["version"],3);assert!(upgraded["collections"].is_array());assert!(upgraded["documents"][0]["deletedAt"].is_number());
        let mut incomplete=upgraded;incomplete.as_object_mut().unwrap().remove("collections");let bytes=serde_json::to_vec(&incomplete).unwrap();fs::write(&path,&bytes).unwrap();
        let bad=LibraryStore::open(dir.0.clone()).unwrap();assert!(bad.view().error.is_some());assert!(bad.create(note("overwrite","no")).is_err());assert_eq!(fs::read(&path).unwrap(),bytes);
    }
    #[test]
    fn collection_depth_limit_and_revision_overflow_are_atomic() {
        let dir=TestDir::new();let l=LibraryStore::open(dir.0.clone()).unwrap();let mut parent=None;
        for n in 0..8 {let c=l.save_collection(Collection{name:format!("level {n}"),parent_id:parent,..Default::default()}).unwrap();parent=Some(c.id);}
        assert!(l.save_collection(Collection{name:"too deep".into(),parent_id:parent.clone(),..Default::default()}).is_err());assert_eq!(l.view().collections.len(),8);
        let d=l.create(note("overflow","body")).unwrap();let mut data=l.view();data.documents[0].revision=u64::MAX;data.documents[0].collection_ids=vec![parent.unwrap()];
        fs::write(dir.0.join("library.json"),encode(&data).unwrap()).unwrap();let l=LibraryStore::open(dir.0.clone()).unwrap();let before=fs::read(dir.0.join("library.json")).unwrap();
        assert!(l.trash(&d.id,u64::MAX).is_err());let root=&data.collections[0];assert!(l.delete_collection(&root.id,root.revision).is_err());
        assert_eq!(fs::read(dir.0.join("library.json")).unwrap(),before);assert_eq!(l.view().collections.len(),8);
    }
    #[test]
    fn committed_purge_cleanup_failure_returns_warning_without_restoring_record() {
        let dir=TestDir::new();let source=dir.0.join("source.pdf");fs::write(&source,b"%PDF-1.4\nfixture").unwrap();
        let l=LibraryStore::open(dir.0.clone()).unwrap();let d=l.import_path(&source).unwrap();let trash=l.trash(&d.id,d.revision).unwrap();
        let path=dir.0.join("library-blobs").join(format!("{}.pdf",d.id));fs::remove_file(&path).unwrap();fs::create_dir(&path).unwrap();
        assert!(l.purge(&d.id,trash.revision).unwrap().unwrap().contains("清理失败"));assert!(l.get(&d.id).is_err());
        assert!(path.is_dir());assert!(LibraryStore::open(dir.0.clone()).unwrap().view().documents.is_empty());assert!(source.exists());
    }
    #[test]
    fn draft_commit_restore_keep_visible_work_and_reject_stale_writers() {
        let dir = TestDir::new();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let a = l.create(note("note", "original")).unwrap();
        let b = l.change(change(&a, 1, "draft", "draft", None)).unwrap();
        assert!(b.revision > a.revision);
        assert_eq!(l.get(&a.id).unwrap().draft.unwrap().content, "draft");
        assert!(l.change(change(&a, 2, "commit", "stale", None)).is_err());
        let c = l.change(change(&b, 2, "commit", "second", None)).unwrap();
        assert_eq!(c.versions[0].content, "original");
        assert!(c.draft.is_none());
        let target = c.versions[0].id.clone();
        let d = l
            .change(change(
                &c,
                3,
                "restore",
                "unsaved visible text",
                Some(target),
            ))
            .unwrap();
        assert_eq!(d.content, "original");
        assert_eq!(d.versions.last().unwrap().content, "unsaved visible text");
        let mut old = change(&d, 2, "draft", "late", None);
        old.session_id = "editor-a".into();
        assert!(l.change(old).is_err());
        assert_eq!(l.get(&a.id).unwrap().content, "original");
        let mut other = change(&d, 1, "commit", "other window", None);
        other.session_id = "editor-b".into();
        let e = l.change(other).unwrap();
        assert!(l
            .change(change(&d, 4, "commit", "overwritten", None))
            .is_err());
        assert_eq!(l.get(&a.id).unwrap().content, e.content);
    }
    #[test]
    fn automatic_bucket_keeps_first_prior_body_and_manual_history_is_bounded() {
        let dir = TestDir::new();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let a = l.create(note("note", "bucket origin")).unwrap();
        let b = l.change(change(&a, 1, "automatic", "one", None)).unwrap();
        let c = l.change(change(&b, 2, "automatic", "two", None)).unwrap();
        assert_eq!(c.versions.len(), 1);
        assert_eq!(c.versions[0].content, "bucket origin");
        let unchanged = l.change(change(&c, 3, "commit", "two", None)).unwrap();
        assert_eq!(unchanged.versions.len(), 1);
        let mut d = unchanged;
        for seq in 4..29 {
            d = l
                .change(change(&d, seq, "commit", &format!("body {seq}"), None))
                .unwrap();
        }
        assert_eq!(d.versions.len(), 20);
        assert_eq!(d.versions.last().unwrap().content, "body 27");
    }
    #[test]
    fn damaged_or_unsupported_library_and_failed_write_preserve_original() {
        let dir = TestDir::new();
        let path = dir.0.join("library.json");
        fs::write(&path, "broken").unwrap();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        assert!(l.view().error.is_some());
        assert!(l.create(note("x", "x")).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken");
        assert!(l.export_snapshot(&l.view()).is_err());
        fs::write(&path, "{\"version\":99,\"documents\":[],\"groups\":[]}").unwrap();
        assert!(LibraryStore::open(dir.0.clone())
            .unwrap()
            .view()
            .error
            .is_some());
        fs::remove_file(&path).unwrap();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let d = l.create(note("before", "before")).unwrap();
        let bytes = fs::read(&path).unwrap();
        fs::rename(&path, dir.0.join("preserved")).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(l.change(change(&d, 1, "commit", "after", None)).is_err());
        assert_eq!(l.get(&d.id).unwrap().content, "before");
        assert_eq!(fs::read(dir.0.join("preserved")).unwrap(), bytes);
    }
    #[test]
    fn import_and_capacity_errors_leave_library_unchanged() {
        let dir = TestDir::new();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let invalid = dir.0.join("bad.md");
        fs::write(&invalid, [0xff, 0xfe]).unwrap();
        assert!(l.import_path(&invalid).is_err());
        assert!(l.import_path(&dir.0).is_err());
        assert!(l
            .create(note("too big", &"a".repeat(512 * 1024 + 1)))
            .is_err());
        assert!(l.view().documents.is_empty());
        for n in 0..100 {
            l.create(note(&format!("note {n}"), "")).unwrap();
        }
        assert!(l.create(note("overflow", "")).is_err());
        assert_eq!(l.view().documents.len(), 100);
    }
    #[test]
    fn smart_groups_persist_conditions_and_reject_stale_edits() {
        let dir = TestDir::new();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let group = l
            .save_group(SmartGroup {
                name: "研究分组".into(),
                kind: Some(Kind::Markdown),
                tag: " Research ".into(),
                keyword: "Rust".into(),
                updated_days: Some(7),
                ..Default::default()
            })
            .unwrap();
        let persisted = LibraryStore::open(dir.0.clone()).unwrap().view().groups[0].clone();
        assert_eq!(persisted.id, group.id);
        assert_eq!(persisted.tag, "research");
        assert_eq!(persisted.kind, Some(Kind::Markdown));
        assert_eq!(persisted.keyword, "Rust");
        assert_eq!(persisted.updated_days, Some(7));
        let mut edited = group.clone();
        edited.name = "新名称".into();
        let updated = l.save_group(edited).unwrap();
        assert_eq!(updated.revision, group.revision + 1);
        assert!(l.save_group(group).is_err());
        assert_eq!(l.view().groups[0].name, "新名称");
    }
    #[test]
    fn exhausted_group_revision_rejects_without_poisoning_library() {
        let dir = TestDir::new();
        let path = dir.0.join("library.json");
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let mut g = l
            .save_group(SmartGroup {
                name: "group".into(),
                ..Default::default()
            })
            .unwrap();
        g.revision = u64::MAX;
        fs::write(
            &path,
            encode(&Library {
                documents: vec![],
                groups: vec![g.clone()],
                ..Library::default()
            })
            .unwrap(),
        )
        .unwrap();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let before = fs::read(&path).unwrap();
        g.name = "changed".into();
        assert!(l.save_group(g).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(l.view().groups[0].name, "group");
        assert_eq!(l.view().groups[0].revision, u64::MAX);
        assert!(l.create(note("still works", "text")).is_ok());
    }
    #[test]
    fn web_text_is_inert_and_preserves_source_without_scripts() {
        let d=parse_web("https://example.com/article","<html><title>标题</title><body><script>alert('bad')</script><article><h1>正文</h1><p>Useful text.</p><img src='https://evil.test/tracker'><a href='javascript:bad()'>link</a></article></body></html>").unwrap();
        assert_eq!(d.kind, Kind::Web);
        assert_eq!(d.url.as_deref(), Some("https://example.com/article"));
        assert!(d.content.contains("Useful text."));
        assert!(!d.content.contains("alert"));
        assert!(!d.content.contains("evil.test"));
        assert!(!d.content.contains("javascript:"));
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://user:pass@example.com/",
        ] {
            assert!(parse_web(url, "<p>x</p>").is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn missing_or_redirected_private_blob_blocks_read_and_complete_export() {
        use std::os::unix::fs::symlink;
        let dir = TestDir::new();
        let src = dir.0.join("x.pdf");
        fs::write(&src, b"%PDF-1.4\nx").unwrap();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let d = l.import_path(&src).unwrap();
        let p = dir
            .0
            .join("library-blobs")
            .join(format!("{}.pdf", d.blob_id.unwrap()));
        fs::remove_file(&p).unwrap();
        symlink(&src, &p).unwrap();
        assert!(l.export_snapshot(&l.view()).is_err());
        assert!(LibraryStore::open(dir.0.clone())
            .unwrap()
            .view()
            .error
            .is_some());
    }
    #[test]
    fn metadata_budget_and_failed_restore_do_not_commit_partial_work() {
        let dir = TestDir::new();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let original = l.create(note("restore", "first")).unwrap();
        let second = l
            .change(change(&original, 1, "commit", "second", None))
            .unwrap();
        let before = serde_json::to_vec(&l.view()).unwrap();
        let file = dir.0.join("library.json");
        fs::rename(&file, dir.0.join("saved")).unwrap();
        fs::create_dir(&file).unwrap();
        assert!(l
            .change(change(
                &second,
                2,
                "restore",
                "visible unsaved",
                Some(second.versions[0].id.clone())
            ))
            .is_err());
        assert_eq!(serde_json::to_vec(&l.view()).unwrap(), before);
        fs::remove_dir(&file).unwrap();
        fs::rename(dir.0.join("saved"), &file).unwrap();
        let body = "a".repeat(MAX_TEXT);
        let mut rejected = false;
        let mut prior = fs::read(&file).unwrap();
        for n in 0..40 {
            match l.create(note(&format!("budget {n}"), &body)) {
                Ok(_) => prior = fs::read(&file).unwrap(),
                Err(_) => {
                    rejected = true;
                    break;
                }
            }
        }
        assert!(rejected);
        assert!(l.view().documents.len() < 40);
        assert_eq!(fs::read(&file).unwrap(), prior);
        assert!(prior.len() as u64 <= MAX_METADATA);
    }
    #[test]
    fn pdf_total_limit_and_truncated_blob_preserve_source_and_reject_export() {
        let dir = TestDir::new();
        let l = LibraryStore::open(dir.0.clone()).unwrap();
        let source = dir.0.join("large.pdf");
        let mut bytes = vec![b' '; 18 * 1024 * 1024];
        bytes[..8].copy_from_slice(b"%PDF-1.4");
        fs::write(&source, &bytes).unwrap();
        let mut docs = Vec::new();
        for _ in 0..3 {
            docs.push(l.import_path(&source).unwrap());
        }
        assert!(l.import_path(&source).is_err());
        assert_eq!(l.view().documents.len(), 3);
        assert_eq!(
            fs::read_dir(dir.0.join("library-blobs")).unwrap().count(),
            3
        );
        assert_eq!(fs::metadata(&source).unwrap().len(), 18 * 1024 * 1024);
        let blob = dir
            .0
            .join("library-blobs")
            .join(format!("{}.pdf", docs[0].id));
        fs::write(blob, b"%PDF-").unwrap();
        assert!(l.export_snapshot(&l.view()).is_err());
        assert!(LibraryStore::open(dir.0.clone())
            .unwrap()
            .view()
            .error
            .is_some());
    }
    #[test]
    fn web_fetch_enforces_http_errors_redirects_and_decompressed_size() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::{
                atomic::{AtomicBool, AtomicUsize, Ordering},
                Arc, Mutex,
            },
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let loops = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let stopping = stop.clone();
        let counted = loops.clone();
        let recorded = requests.clone();
        let worker = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            while !stopping.load(Ordering::SeqCst)
                && started.elapsed() < std::time::Duration::from_secs(10)
            {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket.set_nonblocking(false).unwrap();
                        socket
                            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                            .unwrap();
                        socket
                            .set_write_timeout(Some(std::time::Duration::from_secs(2)))
                            .unwrap();
                        let mut request = Vec::new();
                        let mut buf = [0u8; 1024];
                        while request.len() < 16 * 1024
                            && !request.windows(4).any(|w| w == b"\r\n\r\n")
                        {
                            match socket.read(&mut buf) {
                                Ok(0) => break,
                                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                                Err(e) => {
                                    recorded.lock().unwrap().push(format!(
                                        "read {:?}, {} bytes",
                                        e.kind(),
                                        request.len()
                                    ));
                                    break;
                                }
                                Ok(n) => request.extend_from_slice(&buf[..n]),
                            }
                        }
                        if !request.windows(4).any(|w| w == b"\r\n\r\n") {
                            continue;
                        }
                        let request = String::from_utf8_lossy(&request);
                        let line = request.lines().next().unwrap_or("").to_string();
                        recorded.lock().unwrap().push(line.clone());
                        let mut fields = line.split_whitespace();
                        let method = fields.next().unwrap_or("");
                        let target = fields.next().unwrap_or("");
                        let path = if target.starts_with('/') {
                            target.to_string()
                        } else {
                            reqwest::Url::parse(target)
                                .map(|u| u.path().to_string())
                                .unwrap_or_default()
                        };
                        let (status, extra, body) = if method != "GET" {
                            ("400 Bad Request", String::new(), vec![])
                        } else if path == "/large" {
                            (
                                "200 OK",
                                "Content-Encoding: gzip\r\n".to_string(),
                                include_bytes!("../fixtures/too-large-page.gz").to_vec(),
                            )
                        } else if path == "/loop" {
                            counted.fetch_add(1, Ordering::SeqCst);
                            ("302 Found", "Location: /loop\r\n".to_string(), vec![])
                        } else if path == "/redirect" {
                            ("302 Found", "Location: /article\r\n".to_string(), vec![])
                        } else if path.starts_with("/five/") || path.starts_with("/six/") {
                            let max = if path.starts_with("/five/") { 5 } else { 6 };
                            let n = path
                                .rsplit('/')
                                .next()
                                .unwrap()
                                .parse::<usize>()
                                .unwrap_or(99);
                            if n < max {
                                (
                                    "302 Found",
                                    format!(
                                        "Location: {}/{}\r\n",
                                        path.rsplit_once('/').unwrap().0,
                                        n + 1
                                    ),
                                    vec![],
                                )
                            } else {
                                (
                                    "200 OK",
                                    String::new(),
                                    b"<html><article>Chain complete</article></html>".to_vec(),
                                )
                            }
                        } else if path == "/article" {
                            ("200 OK",String::new(),b"<html><title>Fixture</title><article><p>Offline readable content</p></article></html>".to_vec())
                        } else {
                            (
                                "404 Not Found",
                                String::new(),
                                b"<html><article>Otherwise valid body</article></html>".to_vec(),
                            )
                        };
                        let headers=format!("HTTP/1.1 {status}\r\nContent-Type: text/html\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",body.len());
                        let _ = socket.write_all(headers.as_bytes());
                        let _ = socket.write_all(&body);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(1))
                    }
                    Err(_) => break,
                }
            }
        });
        let good = fetch_web(&format!("{base}/redirect"));
        let status = fetch_web(&format!("{base}/error"));
        let five = fetch_web(&format!("{base}/five/0"));
        let six = fetch_web(&format!("{base}/six/0"));
        let redirects = fetch_web(&format!("{base}/loop"));
        let large = fetch_web(&format!("{base}/large"));
        stop.store(true, Ordering::SeqCst);
        worker.join().unwrap();
        let trace = requests.lock().unwrap();
        let good = good.unwrap();
        assert!(
            good.content.contains("Offline readable content"),
            "{trace:?}"
        );
        assert_eq!(good.url.unwrap(), format!("{base}/article"));
        assert!(status.is_err());
        assert!(five.is_ok(), "{five:?} {trace:?}");
        assert!(six.is_err(), "{six:?} {trace:?}");
        assert!(redirects.is_err(), "{redirects:?} {trace:?}");
        assert_eq!(loops.load(Ordering::SeqCst), 6, "{trace:?}");
        assert!(large.unwrap_err().contains("4MiB"));
    }
}
