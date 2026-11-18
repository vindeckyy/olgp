//! Local private RAG with keyword retrieval and citations. No network calls.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One ingested document inside a knowledge base.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub filename: String,
    pub mime: String,
    pub text: String,
}

/// A private collection of documents.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub id: String,
    pub name: String,
    pub documents: Vec<Document>,
}

/// A cited document returned by a query.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub document_id: String,
    pub filename: String,
    pub excerpt: String,
    pub score: u32,
}

/// Deterministic query answer and its citations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryResult {
    pub answer: String,
    pub sources: Vec<Source>,
    pub timing_ms: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct StoreState {
    next_id: u64,
    bases: Vec<KnowledgeBase>,
}

/// JSON-backed store of private knowledge bases.
pub struct RagStore {
    path: PathBuf,
    state: StoreState,
}

impl RagStore {
    /// Open a store at `path`, starting empty when the file does not exist.
    ///
    /// # Errors
    /// Returns a description when an existing file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, String> {
        let state = if path.exists() {
            let text = fs::read_to_string(path).map_err(|e| format!("rag read: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("rag parse: {e}"))?
        } else {
            StoreState::default()
        };
        Ok(Self { path: path.to_path_buf(), state })
    }

    /// Create an empty knowledge base and persist it.
    ///
    /// # Errors
    /// Returns a description when the store cannot be written.
    pub fn create(&mut self, name: &str) -> Result<KnowledgeBase, String> {
        let id = self.take_id("kb");
        let base = KnowledgeBase { id, name: name.to_owned(), documents: Vec::new() };
        self.state.bases.push(base.clone());
        self.persist()?;
        Ok(base)
    }

    /// Look up a knowledge base by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&KnowledgeBase> {
        self.state.bases.iter().find(|base| base.id == id)
    }

    /// List all knowledge bases.
    #[must_use]
    pub fn list(&self) -> &[KnowledgeBase] {
        &self.state.bases
    }

    /// Delete a knowledge base and all of its documents.
    ///
    /// # Errors
    /// Returns a description when the store cannot be written.
    pub fn delete(&mut self, id: &str) -> Result<bool, String> {
        let Some(pos) = self.state.bases.iter().position(|base| base.id == id) else {
            return Ok(false);
        };
        self.state.bases.remove(pos);
        self.persist()?;
        Ok(true)
    }

    /// Ingest plain text into a knowledge base.
    ///
    /// # Errors
    /// Returns a description when the base is missing or the store cannot be written.
    pub fn add_text(&mut self, id: &str, filename: &str, text: &str) -> Result<Document, String> {
        self.add(id, filename, "text/plain", text.to_owned())
    }

    /// Ingest bytes, decoding them as UTF-8 lossily.
    ///
    /// # Errors
    /// Returns a description when the base is missing or the store cannot be written.
    pub fn add_file(
        &mut self,
        id: &str,
        filename: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<Document, String> {
        self.add(id, filename, mime, String::from_utf8_lossy(bytes).into_owned())
    }

    /// Ingest an image data URL as text.
    ///
    /// # Errors
    /// Returns a description when the base is missing or the store cannot be written.
    pub fn add_image(&mut self, id: &str, filename: &str, value: &str) -> Result<Document, String> {
        let mime = value
            .strip_prefix("data:")
            .and_then(|rest| rest.split(';').next())
            .filter(|mime| !mime.is_empty())
            .unwrap_or("image");
        self.add(id, filename, mime, value.to_owned())
    }

    /// List the documents in a knowledge base.
    ///
    /// # Errors
    /// Returns a description when the base is missing.
    pub fn documents(&self, id: &str) -> Result<&[Document], String> {
        let base = self.get(id).ok_or_else(|| format!("knowledge base not found: {id}"))?;
        Ok(&base.documents)
    }

    /// Rank documents by lowercase token overlap with `question`.
    ///
    /// # Errors
    /// Returns a description when the base is missing.
    pub fn query(&self, id: &str, question: &str, top_k: usize) -> Result<QueryResult, String> {
        let base = self.get(id).ok_or_else(|| format!("knowledge base not found: {id}"))?;
        let query_tokens = tokens(question);
        let mut ranked: Vec<(&Document, u32)> = base
            .documents
            .iter()
            .map(|doc| (doc, overlap(&query_tokens, &doc.text)))
            .filter(|(_, score)| *score > 0)
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
        ranked.truncate(top_k);
        let sources: Vec<Source> = ranked
            .into_iter()
            .map(|(doc, score)| Source {
                document_id: doc.id.clone(),
                filename: doc.filename.clone(),
                excerpt: excerpt(&doc.text),
                score,
            })
            .collect();
        let answer = if sources.is_empty() {
            "No matching documents.".to_owned()
        } else {
            format!("Found {} matching documents.", sources.len())
        };
        Ok(QueryResult { answer, sources, timing_ms: 0 })
    }

    fn take_id(&mut self, prefix: &str) -> String {
        let n = self.state.next_id;
        self.state.next_id += 1;
        format!("{prefix}-{n}")
    }

    fn add(&mut self, id: &str, filename: &str, mime: &str, text: String) -> Result<Document, String> {
        let doc_id = self.take_id("doc");
        let base = self
            .state
            .bases
            .iter_mut()
            .find(|base| base.id == id)
            .ok_or_else(|| format!("knowledge base not found: {id}"))?;
        let doc = Document { id: doc_id, filename: filename.to_owned(), mime: mime.to_owned(), text };
        base.documents.push(doc.clone());
        self.persist()?;
        Ok(doc)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| format!("rag parent dir: {e}"))?;
            }
        }
        let json = serde_json::to_vec(&self.state).map_err(|e| format!("rag encode: {e}"))?;
        let mut options = fs::OpenOptions::new();
        options.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&self.path).map_err(|e| format!("rag open: {e}"))?;
        file.write_all(&json).map_err(|e| format!("rag write: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("rag permissions: {e}"))?;
        }
        Ok(())
    }
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn overlap(query: &[String], text: &str) -> u32 {
    let unique: HashSet<&str> = query.iter().map(String::as_str).collect();
    let doc = tokens(text);
    unique
        .iter()
        .filter(|token| doc.iter().any(|word| word.as_str() == **token))
        .count() as u32
}

fn excerpt(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(200).collect();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn temp_store(name: &str) -> (PathBuf, PathBuf) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("olgp-rag-{}-{}-{n}", std::process::id(), name));
        let _ = fs::remove_dir_all(&root);
        (root.clone(), root.join("rag.json"))
    }

    #[test]
    fn create_ingest_query_and_reload() {
        let (root, path) = temp_store("ingest");
        let mut store = RagStore::load(&path).unwrap();
        let base = store.create("notes").unwrap();

        let text = store.add_text(&base.id, "a.txt", "GPU pairing uses noise keys").unwrap();
        assert_eq!(text.mime, "text/plain");
        let file = store
            .add_file(&base.id, "b.bin", "application/octet-stream", b"cuda kernel \xFF bytes")
            .unwrap();
        assert!(file.text.contains('\u{fffd}'));
        let image = store
            .add_image(&base.id, "pic.png", "data:image/png;base64,AAAA")
            .unwrap();
        assert_eq!(image.mime, "image/png");
        assert_eq!(image.text, "data:image/png;base64,AAAA");
        assert_eq!(store.documents(&base.id).unwrap().len(), 3);

        let result = store.query(&base.id, "GPU PAIRING", 2).unwrap();
        assert_eq!(result.answer, "Found 1 matching documents.");
        assert_eq!(result.sources[0].document_id, text.id);
        assert_eq!(result.sources[0].filename, "a.txt");
        assert!(result.sources[0].score > 0);
        assert!(result.sources[0].excerpt.contains("GPU"));
        assert_eq!(result.timing_ms, 0);

        let reloaded = RagStore::load(&path).unwrap();
        assert_eq!(reloaded.list().len(), 1);
        assert_eq!(reloaded.documents(&base.id).unwrap().len(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_removes_documents_and_persists() {
        let (root, path) = temp_store("delete");
        let mut store = RagStore::load(&path).unwrap();
        let base = store.create("temp").unwrap();
        store.add_text(&base.id, "x.txt", "secret token").unwrap();

        assert!(store.delete(&base.id).unwrap());
        assert!(store.get(&base.id).is_none());
        assert!(store.list().is_empty());
        assert!(!store.delete(&base.id).unwrap());
        assert!(store.documents(&base.id).is_err());
        assert!(RagStore::load(&path).unwrap().list().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_query_returns_no_sources() {
        let (root, path) = temp_store("empty");
        let mut store = RagStore::load(&path).unwrap();
        let base = store.create("notes").unwrap();
        store.add_text(&base.id, "a.txt", "some words here").unwrap();

        let result = store.query(&base.id, "", 5).unwrap();
        assert_eq!(result.answer, "No matching documents.");
        assert!(result.sources.is_empty());

        let miss = store.query(&base.id, "unrelated", 5).unwrap();
        assert_eq!(miss.answer, "No matching documents.");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn top_k_limits_sources_and_ranks_by_score() {
        let (root, path) = temp_store("ranking");
        let mut store = RagStore::load(&path).unwrap();
        let base = store.create("notes").unwrap();
        let weak = store.add_text(&base.id, "weak.txt", "alpha").unwrap();
        let strong = store.add_text(&base.id, "strong.txt", "alpha beta gamma").unwrap();

        let result = store.query(&base.id, "alpha beta", 1).unwrap();
        assert_eq!(result.answer, "Found 1 matching documents.");
        assert_eq!(result.sources[0].document_id, strong.id);
        assert!(result.sources[0].score > 1);

        let both = store.query(&base.id, "alpha beta", 5).unwrap();
        assert_eq!(both.sources.len(), 2);
        assert_eq!(both.sources[1].document_id, weak.id);
        fs::remove_dir_all(root).unwrap();
    }
}
