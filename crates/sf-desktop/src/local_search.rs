//! Embedded Local Full-Text Search Engine for SafiData Desktop (`Section 10`).
//!
//! Provides fast, Elasticsearch-like local BM25 full-text search across user-selected folders,
//! local SafiData projects, datasets, spreadsheets, documents, presentations, PDFs, and text/HTML
//! files — running 100% locally on the user's computer with zero external search daemon and zero
//! automatic cloud upload.
//!
//! Privacy & Security Guarantees:
//! - Never logs file paths, file names, cell values, or search queries (`AGENTS.md` Rule 4).
//! - Persists the index atomically and encrypted at rest using `ChaCha20-Poly1305` via `crate::vault`.
//! - Works in Offline Read-Only Mode without mutating the index or contacting any server.
//! - Zero network access (`tools/check-no-network.sh`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::ZlibDecoder;
use quick_xml::Reader as XmlReader;
use quick_xml::events::Event as XmlEvent;
use serde::{Deserialize, Serialize};
use sf_arabic::{NormOptions, normalize_text};
use sf_core::limits::PlatformLimits;
use sf_io::ingest_workbook_universal;
use zip::ZipArchive;

use crate::vault::{VaultKey, decrypt_vault_payload, encrypt_vault_payload};

/// Maximum single file size indexed (`50 MiB`).
pub const MAX_INDEXABLE_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// Maximum uncompressed XML member size read inside DOCX/PPTX containers (`16 MiB`).
const MAX_ZIP_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
/// Maximum extracted character budget stored per indexed file (`2 MiB`).
const MAX_EXTRACTED_TEXT_BYTES: usize = 2 * 1024 * 1024;
/// Maximum spreadsheet rows indexed per file to guarantee bounded memory.
const MAX_SPREADSHEET_ROWS_PER_FILE: u64 = 250_000;

/// Supported local document format categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexedFileType {
    /// Excel OpenXML (`xlsx`, `xlsm`).
    Xlsx,
    /// Excel Binary (`xlsb`).
    Xlsb,
    /// Legacy Excel (`xls`).
    Xls,
    /// OpenDocument Spreadsheet (`ods`).
    Ods,
    /// Delimited tabular text (`csv`, `tsv`).
    Csv,
    /// Microsoft Word (`docx`).
    Docx,
    /// Microsoft PowerPoint (`pptx`).
    Pptx,
    /// Portable Document Format (`pdf` — extractable text only; scanned PDFs are flagged without OCR).
    Pdf,
    /// Plain text / Markdown (`txt`, `md`).
    Text,
    /// HTML document (`html`, `htm`).
    Html,
    /// Local SafiData project/report JSON (`safi`, `json`).
    SafiProject,
}

impl IndexedFileType {
    /// Detects the supported file type from a file extension.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "xlsx" | "xlsm" => Some(Self::Xlsx),
            "xlsb" => Some(Self::Xlsb),
            "xls" => Some(Self::Xls),
            "ods" => Some(Self::Ods),
            "csv" | "tsv" => Some(Self::Csv),
            "docx" => Some(Self::Docx),
            "pptx" => Some(Self::Pptx),
            "pdf" => Some(Self::Pdf),
            "txt" | "md" => Some(Self::Text),
            "html" | "htm" => Some(Self::Html),
            "safi" | "json" => Some(Self::SafiProject),
            _ => None,
        }
    }
}

/// An indexed structural unit inside a document (e.g. worksheet, slide, paragraph, or text section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedSection {
    /// Human-readable location label (`Sheet: Sales`, `Slide 3`, `Document Body`, `Line 1`).
    pub location: String,
    /// Original un-normalized text excerpt preserved for display snippets.
    pub text: String,
}

/// Indexed metadata and sections for a single local file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedDocument {
    /// Stable integer document identifier inside the index.
    pub doc_id: u32,
    /// Canonical local file path.
    pub path: String,
    /// File name without parent directory.
    pub file_name: String,
    /// Parent directory path for folder filtering.
    pub folder: String,
    /// Detected file format.
    pub file_type: IndexedFileType,
    /// Optional SafiData project scope identifier.
    pub project_id: Option<String>,
    /// File byte size at index time.
    pub size_bytes: u64,
    /// Last modification timestamp in Unix milliseconds.
    pub modified_ms: u64,
    /// `BLAKE3` content digest used for incremental change detection.
    pub content_blake3: String,
    /// Total normalized token count in this document (for BM25 length normalization).
    pub token_count: u32,
    /// Whether this document had extractable text (false for scanned image-only PDFs).
    pub has_extractable_text: bool,
    /// Optional non-fatal extraction notice (e.g., scanned PDF without OCR).
    pub extraction_notice: Option<String>,
    /// Extracted sections preserving original text and locations.
    pub sections: Vec<IndexedSection>,
}

/// Progress snapshot emitted while indexing local folders or files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    /// Files examined so far.
    pub scanned_files: usize,
    /// Total candidate files discovered.
    pub total_files: usize,
    /// Files newly indexed or updated.
    pub indexed_files: usize,
    /// Unchanged files skipped via incremental fingerprint.
    pub skipped_unchanged: usize,
    /// Files skipped due to size limit, unsupported format, or extraction error.
    pub skipped_errors: usize,
}

/// Summary returned after an indexing or incremental re-indexing run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRunSummary {
    /// Newly indexed files.
    pub added: usize,
    /// Re-indexed modified files.
    pub updated: usize,
    /// Unchanged files skipped.
    pub unchanged: usize,
    /// Deleted files purged from the index.
    pub removed: usize,
    /// Files skipped due to limits or unreadable bytes.
    pub skipped: usize,
    /// Whether indexing stopped early due to user cancellation.
    pub cancelled: bool,
    /// Total active documents remaining in the index.
    pub total_documents: usize,
}

/// Optional filters applied during local search.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilters {
    /// Restrict results to one or more file types.
    #[serde(default)]
    pub file_types: Vec<IndexedFileType>,
    /// Restrict results to paths starting with this folder prefix (case-insensitive).
    pub folder_prefix: Option<String>,
    /// Restrict results to a specific SafiData project ID.
    pub project_id: Option<String>,
    /// Minimum file modification timestamp (inclusive, Unix ms).
    pub modified_after_ms: Option<u64>,
    /// Maximum file modification timestamp (inclusive, Unix ms).
    pub modified_before_ms: Option<u64>,
}

/// Query request executed against [`LocalSearchIndex`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    /// Raw query supporting multi-word, `"exact phrases"`, `prefix*`, Arabic & English.
    pub query: String,
    /// Structured filters.
    #[serde(default)]
    pub filters: SearchFilters,
    /// Enable edit-distance-1 typo tolerance for terms of length >= 4.
    #[serde(default = "default_true")]
    pub enable_fuzzy: bool,
    /// Maximum number of ranked results to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

const fn default_true() -> bool {
    true
}

const fn default_limit() -> usize {
    25
}

/// Single ranked search hit returned by [`LocalSearchIndex::search_readonly`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// Indexed document path.
    pub path: String,
    /// File name.
    pub file_name: String,
    /// Detected file type.
    pub file_type: IndexedFileType,
    /// Optional project ID.
    pub project_id: Option<String>,
    /// Location of the best-matching section (`Sheet: Sales`, `Slide 2`, etc.).
    pub match_location: String,
    /// Highlighted snippet with `[[...]]` markers around matched tokens.
    pub snippet: String,
    /// BM25 relevance score.
    pub score: f64,
    /// Last modification timestamp in Unix ms.
    pub modified_ms: u64,
}

/// Embedded local BM25 full-text search index (zero external server, zero cloud upload).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSearchIndex {
    schema_version: u32,
    next_doc_id: u32,
    documents: BTreeMap<u32, IndexedDocument>,
    path_to_doc_id: BTreeMap<String, u32>,
    /// Inverted index: normalized term -> (`doc_id` -> term frequency).
    postings: BTreeMap<String, BTreeMap<u32, u32>>,
    /// Total tokens across all documents for BM25 `avgdl`.
    total_tokens: u64,
}

impl Default for LocalSearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalSearchIndex {
    /// Creates an empty local search index.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            next_doc_id: 1,
            documents: BTreeMap::new(),
            path_to_doc_id: BTreeMap::new(),
            postings: BTreeMap::new(),
            total_tokens: 0,
        }
    }

    /// Returns the number of indexed documents.
    #[must_use]
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// Returns the number of unique normalized vocabulary terms.
    #[must_use]
    pub fn term_count(&self) -> usize {
        self.postings.len()
    }

    /// Returns a metadata snapshot of an indexed document by canonical path.
    #[must_use]
    pub fn get_document_by_path(&self, path: &Path) -> Option<&IndexedDocument> {
        let key = canonical_path_key(path);
        let doc_id = self.path_to_doc_id.get(&key)?;
        self.documents.get(doc_id)
    }

    /// Indexes or incrementally updates all supported files inside the given user-selected folders.
    ///
    /// - Skips unchanged files using `(size_bytes, modified_ms, blake3)` fingerprints.
    /// - Re-indexes modified files automatically.
    /// - Removes files that were previously indexed under these roots but have since been deleted.
    /// - Supports cancellation via `cancel_flag` and live progress updates via `on_progress`.
    ///
    /// # Errors
    /// Returns `SF-SYS-001` only on fatal I/O errors; unreadable individual files are counted in `skipped`.
    pub fn index_folders<F>(
        &mut self,
        roots: &[PathBuf],
        project_id: Option<&str>,
        cancel_flag: Option<&AtomicBool>,
        mut on_progress: F,
    ) -> Result<IndexRunSummary, String>
    where
        F: FnMut(&IndexProgress),
    {
        let mut discovered = Vec::new();
        for root in roots {
            collect_supported_files(root, &mut discovered, cancel_flag);
            if is_cancelled(cancel_flag) {
                break;
            }
        }
        discovered.sort();
        discovered.dedup();

        let total_files = discovered.len();
        let mut seen_paths = BTreeSet::new();
        let mut added = 0usize;
        let mut updated = 0usize;
        let mut unchanged = 0usize;
        let mut skipped = 0usize;
        let mut cancelled = false;

        for (idx, file_path) in discovered.iter().enumerate() {
            if is_cancelled(cancel_flag) {
                cancelled = true;
                break;
            }
            let path_key = canonical_path_key(file_path);
            let _ = seen_paths.insert(path_key.clone());

            match self.index_single_file(file_path, project_id, cancel_flag) {
                Ok(FileIndexOutcome::Added) => added += 1,
                Ok(FileIndexOutcome::Updated) => updated += 1,
                Ok(FileIndexOutcome::Unchanged) => unchanged += 1,
                Ok(FileIndexOutcome::Skipped) | Err(_) => skipped += 1,
            }

            on_progress(&IndexProgress {
                scanned_files: idx + 1,
                total_files,
                indexed_files: added + updated,
                skipped_unchanged: unchanged,
                skipped_errors: skipped,
            });
        }

        // Purge deleted files that belonged to the scanned root directories (unless cancelled mid-scan).
        let mut removed = 0usize;
        if !cancelled {
            let root_prefixes: Vec<String> = roots
                .iter()
                .map(|root| canonical_path_key(root).to_ascii_lowercase())
                .collect();
            let stale_paths: Vec<String> = self
                .path_to_doc_id
                .keys()
                .filter(|existing| {
                    let lower = existing.to_ascii_lowercase();
                    let under_roots = root_prefixes.iter().any(|prefix| {
                        lower == *prefix
                            || lower.starts_with(&format!("{prefix}/"))
                            || lower.starts_with(&format!("{prefix}\\"))
                    });
                    under_roots && !seen_paths.contains(*existing)
                })
                .cloned()
                .collect();
            for stale in stale_paths {
                if self.remove_document_by_key(&stale) {
                    removed += 1;
                }
            }
        }

        Ok(IndexRunSummary {
            added,
            updated,
            unchanged,
            removed,
            skipped,
            cancelled,
            total_documents: self.documents.len(),
        })
    }

    /// Indexes or updates a single local file.
    ///
    /// # Errors
    /// Returns `SF-IMP-003` when the file exceeds [`MAX_INDEXABLE_FILE_BYTES`], or `SF-IMP-002`
    /// if cancelled or unreadable.
    pub fn index_single_file(
        &mut self,
        path: &Path,
        project_id: Option<&str>,
        cancel_flag: Option<&AtomicBool>,
    ) -> Result<FileIndexOutcome, String> {
        if is_cancelled(cancel_flag) {
            return Err("SF-IMP-002".to_owned());
        }
        let Some(file_type) = IndexedFileType::from_path(path) else {
            return Ok(FileIndexOutcome::Skipped);
        };
        let meta = fs::metadata(path).map_err(|_| "SF-IMP-002".to_owned())?;
        if !meta.is_file() {
            return Ok(FileIndexOutcome::Skipped);
        }
        let size_bytes = meta.len();
        if size_bytes > MAX_INDEXABLE_FILE_BYTES {
            return Err("SF-IMP-003".to_owned());
        }
        let modified_ms = metadata_modified_ms(&meta);
        let path_key = canonical_path_key(path);

        // Fast incremental check on (size, mtime) first; verify BLAKE3 before skipping or updating.
        let bytes = fs::read(path).map_err(|_| "SF-IMP-002".to_owned())?;
        let content_blake3 = sf_recipe::blake3_hex(&bytes);
        if let Some(&existing_id) = self.path_to_doc_id.get(&path_key)
            && let Some(existing) = self.documents.get(&existing_id)
            && existing.content_blake3 == content_blake3
            && existing.size_bytes == size_bytes
            && existing.project_id.as_deref() == project_id
        {
            return Ok(FileIndexOutcome::Unchanged);
        }

        let extracted = extract_sections(path, file_type, &bytes, cancel_flag)?;
        let was_existing = self.remove_document_by_key(&path_key);

        let doc_id = self.next_doc_id;
        self.next_doc_id = self.next_doc_id.saturating_add(1);

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_owned();
        let folder = path.parent().map(canonical_path_key).unwrap_or_default();

        // Tokenize file name + location labels + section content for unified local recall.
        let mut term_freqs: BTreeMap<String, u32> = BTreeMap::new();
        let mut token_count = 0u32;
        for token in tokenize_text(&file_name) {
            *term_freqs.entry(token).or_insert(0) += 2; // Boost file-name match slightly
            token_count = token_count.saturating_add(1);
        }
        for section in &extracted.sections {
            for token in tokenize_text(&section.location) {
                *term_freqs.entry(token).or_insert(0) += 1;
                token_count = token_count.saturating_add(1);
            }
            for token in tokenize_text(&section.text) {
                *term_freqs.entry(token).or_insert(0) += 1;
                token_count = token_count.saturating_add(1);
            }
        }

        for (term, freq) in term_freqs {
            let _ = self.postings.entry(term).or_default().insert(doc_id, freq);
        }
        self.total_tokens = self.total_tokens.saturating_add(u64::from(token_count));

        let doc = IndexedDocument {
            doc_id,
            path: path_key.clone(),
            file_name,
            folder,
            file_type,
            project_id: project_id.map(str::to_owned),
            size_bytes,
            modified_ms,
            content_blake3,
            token_count,
            has_extractable_text: extracted.has_extractable_text,
            extraction_notice: extracted.notice,
            sections: extracted.sections,
        };
        let _ = self.documents.insert(doc_id, doc);
        let _ = self.path_to_doc_id.insert(path_key, doc_id);

        Ok(if was_existing {
            FileIndexOutcome::Updated
        } else {
            FileIndexOutcome::Added
        })
    }

    /// Removes a document from the index by path.
    pub fn remove_document(&mut self, path: &Path) -> bool {
        self.remove_document_by_key(&canonical_path_key(path))
    }

    fn remove_document_by_key(&mut self, path_key: &str) -> bool {
        let Some(doc_id) = self.path_to_doc_id.remove(path_key) else {
            return false;
        };
        let Some(doc) = self.documents.remove(&doc_id) else {
            return false;
        };
        self.total_tokens = self.total_tokens.saturating_sub(u64::from(doc.token_count));
        let mut empty_terms = Vec::new();
        for (term, posting) in &mut self.postings {
            let _ = posting.remove(&doc_id);
            if posting.is_empty() {
                empty_terms.push(term.clone());
            }
        }
        for term in empty_terms {
            let _ = self.postings.remove(&term);
        }
        true
    }

    /// Executes a local BM25 search query in strict Read-Only mode (works 100% offline with
    /// zero mutations and zero network access).
    #[must_use]
    pub fn search_readonly(&self, request: &SearchQuery) -> Vec<SearchHit> {
        let parsed = parse_query(&request.query);
        if parsed.is_empty() || self.documents.is_empty() {
            return Vec::new();
        }

        let n_docs = self.documents.len() as f64;
        let avgdl = (self.total_tokens as f64 / n_docs).max(1.0);
        const K1: f64 = 1.2;
        const B: f64 = 0.75;

        // Collect candidate term expansions for each query clause.
        let mut doc_scores: BTreeMap<u32, f64> = BTreeMap::new();
        let mut doc_clause_hits: BTreeMap<u32, usize> = BTreeMap::new();
        let mut highlight_terms: Vec<String> = Vec::new();

        for term in &parsed.terms {
            highlight_terms.push(term.clone());
            let mut matched_docs: BTreeMap<u32, f64> = BTreeMap::new();

            if let Some(posting) = self.postings.get(term) {
                score_posting(
                    posting,
                    n_docs,
                    avgdl,
                    K1,
                    B,
                    1.0,
                    &self.documents,
                    &mut matched_docs,
                );
            } else if request.enable_fuzzy && term.chars().count() >= 4 {
                for (vocab_term, posting) in &self.postings {
                    if damerau_levenshtein_le_one(term, vocab_term) {
                        score_posting(
                            posting,
                            n_docs,
                            avgdl,
                            K1,
                            B,
                            0.72,
                            &self.documents,
                            &mut matched_docs,
                        );
                    }
                }
            }

            for (doc_id, score) in matched_docs {
                *doc_scores.entry(doc_id).or_insert(0.0) += score;
                *doc_clause_hits.entry(doc_id).or_insert(0) += 1;
            }
        }

        for prefix in &parsed.prefixes {
            highlight_terms.push(prefix.clone());
            let mut matched_docs: BTreeMap<u32, f64> = BTreeMap::new();
            for (vocab_term, posting) in self.postings.range(prefix.clone()..) {
                if !vocab_term.starts_with(prefix) {
                    break;
                }
                score_posting(
                    posting,
                    n_docs,
                    avgdl,
                    K1,
                    B,
                    0.9,
                    &self.documents,
                    &mut matched_docs,
                );
            }
            for (doc_id, score) in matched_docs {
                *doc_scores.entry(doc_id).or_insert(0.0) += score;
                *doc_clause_hits.entry(doc_id).or_insert(0) += 1;
            }
        }

        for phrase in &parsed.phrases {
            for token in phrase {
                highlight_terms.push(token.clone());
            }
            // Intersect postings for phrase tokens, then verify contiguous normalized phrase.
            let mut candidate_ids: Option<BTreeSet<u32>> = None;
            for token in phrase {
                let ids: BTreeSet<u32> = self
                    .postings
                    .get(token)
                    .map(|p| p.keys().copied().collect())
                    .unwrap_or_default();
                candidate_ids = Some(match candidate_ids {
                    None => ids,
                    Some(prev) => prev.intersection(&ids).copied().collect(),
                });
            }
            let phrase_str = phrase.join(" ");
            for doc_id in candidate_ids.unwrap_or_default() {
                if let Some(doc) = self.documents.get(&doc_id)
                    && document_contains_phrase(doc, &phrase_str)
                {
                    *doc_scores.entry(doc_id).or_insert(0.0) += 3.5;
                    *doc_clause_hits.entry(doc_id).or_insert(0) += 1;
                }
            }
        }

        let required_clauses = parsed.terms.len() + parsed.prefixes.len() + parsed.phrases.len();
        let mut hits = Vec::new();
        for (doc_id, score) in doc_scores {
            let Some(doc) = self.documents.get(&doc_id) else {
                continue;
            };
            if !matches_filters(doc, &request.filters) {
                continue;
            }
            // Enforce all exact phrases strictly; for multi-word queries boost full matches.
            let phrases_ok = parsed
                .phrases
                .iter()
                .all(|phrase| document_contains_phrase(doc, &phrase.join(" ")));
            if !phrases_ok {
                continue;
            }
            let clause_hits = doc_clause_hits.get(&doc_id).copied().unwrap_or(0);
            let coverage_bonus = if required_clauses > 1 && clause_hits >= required_clauses {
                1.35
            } else {
                1.0
            };
            let (match_location, snippet) = build_best_snippet(doc, &highlight_terms);
            hits.push(SearchHit {
                path: doc.path.clone(),
                file_name: doc.file_name.clone(),
                file_type: doc.file_type,
                project_id: doc.project_id.clone(),
                match_location,
                snippet,
                score: ((score * coverage_bonus) * 10_000.0).round() / 10_000.0,
                modified_ms: doc.modified_ms,
            });
        }

        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.path.cmp(&right.path))
        });
        hits.truncate(request.limit.clamp(1, 200));
        hits
    }

    /// Saves the index atomically and encrypted at rest using `ChaCha20-Poly1305` (`crate::vault`).
    ///
    /// # Errors
    /// Returns `SF-SYS-023` on encryption failure or `SF-SYS-001` on disk write failure.
    pub fn save_encrypted(&self, path: &Path, vault_key: &VaultKey) -> Result<(), String> {
        let json = serde_json::to_vec(self).map_err(|_| "SF-SYS-001".to_owned())?;
        let encrypted =
            encrypt_vault_payload(vault_key, &json).map_err(|_| "SF-SYS-023".to_owned())?;
        atomic_write_bytes(path, &encrypted)
    }

    /// Loads and decrypts an encrypted local search index from disk.
    ///
    /// # Errors
    /// Returns `SF-SYS-024` if the vault key is wrong or the index was tampered with, or
    /// `SF-SYS-001` on I/O or deserialization error.
    pub fn load_encrypted(path: &Path, vault_key: &VaultKey) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|_| "SF-SYS-001".to_owned())?;
        let decrypted =
            decrypt_vault_payload(vault_key, &bytes).map_err(|_| "SF-SYS-024".to_owned())?;
        serde_json::from_slice(&decrypted).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Saves the index atomically into `storage_dir`, encrypted with `vault_key` when configured.
    ///
    /// # Errors
    /// Returns `SF-SYS-023`, `SF-SYS-016`, or `SF-SYS-001`.
    pub fn save_to_dir(
        &self,
        storage_dir: &Path,
        vault_key: Option<&VaultKey>,
    ) -> Result<(), String> {
        fs::create_dir_all(storage_dir).map_err(|_| "SF-SYS-016".to_owned())?;
        if let Some(key) = vault_key {
            self.save_encrypted(&storage_dir.join("local_search_index.safi.enc"), key)
        } else {
            let json = serde_json::to_vec(self).map_err(|_| "SF-SYS-001".to_owned())?;
            atomic_write_bytes(&storage_dir.join("local_search_index.json"), &json)
        }
    }

    /// Loads an existing persisted index from `storage_dir`, or returns an empty index if none exists.
    ///
    /// # Errors
    /// Returns `SF-SYS-024` or `SF-SYS-001` when persisted data is corrupt or cannot be decrypted.
    pub fn load_from_dir(storage_dir: &Path, vault_key: Option<&VaultKey>) -> Result<Self, String> {
        let enc_path = storage_dir.join("local_search_index.safi.enc");
        let json_path = storage_dir.join("local_search_index.json");
        if enc_path.exists() {
            let key = vault_key.ok_or_else(|| "SF-SYS-024".to_owned())?;
            Self::load_encrypted(&enc_path, key)
        } else if json_path.exists() {
            let bytes = fs::read(&json_path).map_err(|_| "SF-SYS-001".to_owned())?;
            serde_json::from_slice(&bytes).map_err(|_| "SF-SYS-001".to_owned())
        } else {
            Ok(Self::new())
        }
    }

    /// Returns lightweight aggregate stats about indexed local documents (zero file contents).
    #[must_use]
    pub fn stats_snapshot(&self) -> serde_json::Value {
        let mut folders: BTreeSet<String> = BTreeSet::new();
        let mut scanned_pdf_notice_count = 0usize;
        for doc in self.documents.values() {
            if !doc.folder.is_empty() {
                let _ = folders.insert(doc.folder.clone());
            }
            if !doc.has_extractable_text {
                scanned_pdf_notice_count = scanned_pdf_notice_count.saturating_add(1);
            }
        }
        serde_json::json!({
            "totalDocuments": self.documents.len(),
            "totalTerms": self.postings.len(),
            "scannedPdfNoticeCount": scanned_pdf_notice_count,
            "indexedFolders": folders.into_iter().collect::<Vec<_>>(),
        })
    }
}

/// Outcome of indexing one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileIndexOutcome {
    /// File was added to the index for the first time.
    Added,
    /// File changed on disk and was re-indexed.
    Updated,
    /// File content digest matched the existing index entry and was skipped.
    Unchanged,
    /// File was skipped (unsupported extension or non-file).
    Skipped,
}

#[derive(Debug)]
struct ExtractedDocument {
    sections: Vec<IndexedSection>,
    has_extractable_text: bool,
    notice: Option<String>,
}

#[derive(Debug, Default)]
struct ParsedQuery {
    terms: Vec<String>,
    prefixes: Vec<String>,
    phrases: Vec<Vec<String>>,
}

impl ParsedQuery {
    fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.prefixes.is_empty() && self.phrases.is_empty()
    }
}

fn is_cancelled(cancel_flag: Option<&AtomicBool>) -> bool {
    cancel_flag.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

fn canonical_path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn metadata_modified_ms(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn collect_supported_files(root: &Path, out: &mut Vec<PathBuf>, cancel_flag: Option<&AtomicBool>) {
    if is_cancelled(cancel_flag) {
        return;
    }
    if root.is_file() {
        if IndexedFileType::from_path(root).is_some() {
            out.push(root.to_path_buf());
        }
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if is_cancelled(cancel_flag) {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_supported_files(&path, out, cancel_flag);
        } else if path.is_file() && IndexedFileType::from_path(&path).is_some() {
            out.push(path);
        }
    }
}

fn extract_sections(
    path: &Path,
    file_type: IndexedFileType,
    bytes: &[u8],
    cancel_flag: Option<&AtomicBool>,
) -> Result<ExtractedDocument, String> {
    match file_type {
        IndexedFileType::Xlsx
        | IndexedFileType::Xlsb
        | IndexedFileType::Xls
        | IndexedFileType::Ods
        | IndexedFileType::Csv => extract_spreadsheet(path, bytes, cancel_flag),
        IndexedFileType::Docx => extract_docx(bytes),
        IndexedFileType::Pptx => extract_pptx(bytes),
        IndexedFileType::Pdf => Ok(extract_pdf_text(bytes)),
        IndexedFileType::Html => Ok(extract_html(bytes)),
        IndexedFileType::Text | IndexedFileType::SafiProject => Ok(extract_plain_text(bytes)),
    }
}

fn extract_spreadsheet(
    path: &Path,
    bytes: &[u8],
    cancel_flag: Option<&AtomicBool>,
) -> Result<ExtractedDocument, String> {
    let limits = PlatformLimits {
        max_file_bytes: MAX_INDEXABLE_FILE_BYTES,
        max_rows: MAX_SPREADSHEET_ROWS_PER_FILE,
    };
    let hint = path.file_name().and_then(|s| s.to_str());
    let report = ingest_workbook_universal(Cursor::new(bytes), limits, hint, cancel_flag)
        .map_err(|_| "SF-IMP-002".to_owned())?;

    let mut sections = Vec::new();
    let mut total_bytes = 0usize;
    for sheet in &report.worksheets {
        if total_bytes >= MAX_EXTRACTED_TEXT_BYTES {
            break;
        }
        let mut sheet_text = String::new();
        for table in &sheet.tables {
            if !table.title.trim().is_empty() {
                sheet_text.push_str(table.title.trim());
                sheet_text.push('\n');
            }
            if !table.headers.is_empty() {
                sheet_text.push_str(&table.headers.join(" | "));
                sheet_text.push('\n');
            }
            for sep in &table.separated_rows {
                if !sep.values.is_empty() {
                    sheet_text.push_str(&sep.values.join(" | "));
                    sheet_text.push('\n');
                }
            }
            for row in &table.data_rows {
                if sheet_text.len() + total_bytes >= MAX_EXTRACTED_TEXT_BYTES {
                    break;
                }
                let joined = row
                    .iter()
                    .map(|c| c.trim())
                    .filter(|c| !c.is_empty())
                    .collect::<Vec<_>>()
                    .join(" | ");
                if !joined.is_empty() {
                    sheet_text.push_str(&joined);
                    sheet_text.push('\n');
                }
            }
        }
        let trimmed = sheet_text.trim().to_owned();
        if !trimmed.is_empty() {
            total_bytes = total_bytes.saturating_add(trimmed.len());
            sections.push(IndexedSection {
                location: format!("Sheet: {}", sheet.name),
                text: trimmed,
            });
        }
    }

    let has_text = !sections.is_empty();
    Ok(ExtractedDocument {
        sections,
        has_extractable_text: has_text,
        notice: if has_text {
            None
        } else {
            Some("Workbook contains no searchable text cells.".to_owned())
        },
    })
}

fn extract_docx(bytes: &[u8]) -> Result<ExtractedDocument, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| "SF-IMP-002".to_owned())?;
    let xml = read_zip_entry_bounded(&mut archive, "word/document.xml")?;
    let paragraphs = extract_xml_paragraphs(&xml, b"p", b"t");
    let mut sections = Vec::new();
    let mut total_bytes = 0usize;
    for (idx, para) in paragraphs.into_iter().enumerate() {
        let trimmed = para.trim();
        if trimmed.is_empty() {
            continue;
        }
        if total_bytes + trimmed.len() > MAX_EXTRACTED_TEXT_BYTES {
            break;
        }
        total_bytes += trimmed.len();
        sections.push(IndexedSection {
            location: format!("Paragraph {}", idx + 1),
            text: trimmed.to_owned(),
        });
    }
    let has_text = !sections.is_empty();
    Ok(ExtractedDocument {
        sections,
        has_extractable_text: has_text,
        notice: None,
    })
}

fn extract_pptx(bytes: &[u8]) -> Result<ExtractedDocument, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| "SF-IMP-002".to_owned())?;
    let mut slide_entries: Vec<(u32, String)> = Vec::new();
    for idx in 0..archive.len() {
        if let Ok(file) = archive.by_index(idx) {
            let name = file.name().to_owned();
            if let Some(rest) = name.strip_prefix("ppt/slides/slide")
                && let Some(num_str) = rest.strip_suffix(".xml")
                && let Ok(num) = num_str.parse::<u32>()
            {
                slide_entries.push((num, name));
            }
        }
    }
    slide_entries.sort_by_key(|(num, _)| *num);

    let mut sections = Vec::new();
    let mut total_bytes = 0usize;
    for (slide_num, entry_name) in slide_entries {
        if total_bytes >= MAX_EXTRACTED_TEXT_BYTES {
            break;
        }
        let xml = read_zip_entry_bounded(&mut archive, &entry_name)?;
        let texts = extract_xml_paragraphs(&xml, b"p", b"t");
        let joined = texts
            .into_iter()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !joined.is_empty() {
            total_bytes = total_bytes.saturating_add(joined.len());
            sections.push(IndexedSection {
                location: format!("Slide {slide_num}"),
                text: joined,
            });
        }
    }
    let has_text = !sections.is_empty();
    Ok(ExtractedDocument {
        sections,
        has_extractable_text: has_text,
        notice: None,
    })
}

fn read_zip_entry_bounded(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    entry_name: &str,
) -> Result<String, String> {
    let file = archive
        .by_name(entry_name)
        .map_err(|_| "SF-IMP-002".to_owned())?;
    if file.size() > MAX_ZIP_ENTRY_BYTES {
        return Err("SF-IMP-005".to_owned());
    }
    let mut buf = Vec::new();
    file.take(MAX_ZIP_ENTRY_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|_| "SF-IMP-002".to_owned())?;
    if buf.len() as u64 > MAX_ZIP_ENTRY_BYTES {
        return Err("SF-IMP-005".to_owned());
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn extract_xml_paragraphs(xml: &str, para_local: &[u8], text_local: &[u8]) -> Vec<String> {
    let mut reader = XmlReader::from_str(xml);
    let _ = reader.trim_text(false);
    let mut paragraphs = Vec::new();
    let mut current_para = String::new();
    let mut in_text = false;

    loop {
        match reader.read_event() {
            Ok(XmlEvent::Start(ref e)) => {
                let local = e.local_name();
                if local.as_ref() == text_local {
                    in_text = true;
                }
            }
            Ok(XmlEvent::End(ref e)) => {
                let local = e.local_name();
                if local.as_ref() == text_local {
                    in_text = false;
                } else if local.as_ref() == para_local && !current_para.trim().is_empty() {
                    paragraphs.push(std::mem::take(&mut current_para));
                }
            }
            Ok(XmlEvent::Text(ref e)) if in_text => {
                if let Ok(decoded) = e.unescape() {
                    if !current_para.is_empty() && !current_para.ends_with(' ') {
                        current_para.push(' ');
                    }
                    current_para.push_str(&decoded);
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    if !current_para.trim().is_empty() {
        paragraphs.push(current_para);
    }
    paragraphs
}

/// Maximum decompressed bytes read from PDF `/FlateDecode` streams (`8 MiB`, zlib-bomb guard).
const MAX_PDF_DECOMPRESSED_BYTES: u64 = 8 * 1024 * 1024;

/// Extracts PDF text from both uncompressed and `/FlateDecode`-compressed content streams
/// (supporting literal `(...)` strings and hex/UTF-16BE `<...>` strings).
///
/// Scanned image-only PDFs that require OCR are flagged transparently via
/// `has_extractable_text = false` and an explicit notice (OCR is never claimed or faked).
fn extract_pdf_text(bytes: &[u8]) -> ExtractedDocument {
    let mut chunks = Vec::new();
    let mut total_bytes = 0usize;

    // 1. Scan PDF `stream ... endstream` blocks and decompress `/FlateDecode` streams safely.
    let mut pos = 0usize;
    let mut total_decompressed = 0u64;
    while pos < bytes.len() && total_bytes < MAX_EXTRACTED_TEXT_BYTES {
        let Some(rel_start) = find_subslice(&bytes[pos..], b"stream") else {
            break;
        };
        let stream_kw = pos + rel_start;
        // Inspect the preceding object dictionary (up to 512 bytes back) for `/FlateDecode`.
        let dict_start = stream_kw.saturating_sub(512);
        let dict_slice = &bytes[dict_start..stream_kw];
        let is_flate = find_subslice(dict_slice, b"/FlateDecode").is_some()
            || find_subslice(dict_slice, b"/Fl").is_some();

        let mut data_start = stream_kw + b"stream".len();
        if data_start < bytes.len() && bytes[data_start] == b'\r' {
            data_start += 1;
        }
        if data_start < bytes.len() && bytes[data_start] == b'\n' {
            data_start += 1;
        }
        let Some(rel_end) = find_subslice(&bytes[data_start..], b"endstream") else {
            break;
        };
        let raw_stream = trim_trailing_crlf(&bytes[data_start..data_start + rel_end]);
        pos = data_start + rel_end + b"endstream".len();

        if is_flate {
            let budget = MAX_PDF_DECOMPRESSED_BYTES.saturating_sub(total_decompressed);
            if budget == 0 {
                break;
            }
            let mut decoder = ZlibDecoder::new(raw_stream).take(budget);
            let mut inflated = Vec::new();
            if decoder.read_to_end(&mut inflated).is_ok() && !inflated.is_empty() {
                total_decompressed = total_decompressed.saturating_add(inflated.len() as u64);
                collect_pdf_text_operators(&inflated, &mut chunks, &mut total_bytes);
            }
        } else {
            collect_pdf_text_operators(raw_stream, &mut chunks, &mut total_bytes);
        }
    }

    // 2. Fallback scan for simple inline PDF text objects if no stream block yielded text.
    if chunks.is_empty() {
        collect_pdf_text_operators(bytes, &mut chunks, &mut total_bytes);
    }

    if chunks.is_empty() {
        return ExtractedDocument {
            sections: Vec::new(),
            has_extractable_text: false,
            notice: Some(
                "No extractable PDF text found (scanned image-only PDFs require OCR, which is not enabled)."
                    .to_owned(),
            ),
        };
    }

    ExtractedDocument {
        sections: vec![IndexedSection {
            location: "PDF Text Stream".to_owned(),
            text: chunks.join(" "),
        }],
        has_extractable_text: true,
        notice: None,
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn trim_trailing_crlf(slice: &[u8]) -> &[u8] {
    let mut end = slice.len();
    while end > 0 && matches!(slice[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    &slice[..end]
}

fn collect_pdf_text_operators(bytes: &[u8], chunks: &mut Vec<String>, total_bytes: &mut usize) {
    let mut idx = 0usize;
    while idx < bytes.len() && *total_bytes < MAX_EXTRACTED_TEXT_BYTES {
        if bytes[idx] == b'(' {
            idx += 1;
            let mut depth = 1u32;
            let mut raw = Vec::new();
            while idx < bytes.len() && depth > 0 {
                let ch = bytes[idx];
                if ch == b'\\' && idx + 1 < bytes.len() {
                    idx += 1;
                    let esc = match bytes[idx] {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        other => other,
                    };
                    raw.push(esc);
                } else if ch == b'(' {
                    depth += 1;
                    raw.push(ch);
                } else if ch == b')' {
                    depth -= 1;
                    if depth > 0 {
                        raw.push(ch);
                    }
                } else {
                    raw.push(ch);
                }
                idx += 1;
            }
            push_pdf_text_chunk(&decode_pdf_bytes(&raw), chunks, total_bytes);
        } else if bytes[idx] == b'<' && bytes.get(idx + 1).copied() != Some(b'<') {
            idx += 1;
            let start = idx;
            while idx < bytes.len() && bytes[idx] != b'>' {
                idx += 1;
            }
            if idx < bytes.len() {
                let hex_str: String = bytes[start..idx]
                    .iter()
                    .copied()
                    .filter(|b| !b.is_ascii_whitespace())
                    .map(char::from)
                    .collect();
                idx += 1;
                if hex_str.len() >= 4
                    && hex_str.len() % 2 == 0
                    && hex_str.bytes().all(|b| b.is_ascii_hexdigit())
                    && let Ok(decoded_bytes) = hex::decode(&hex_str)
                {
                    push_pdf_text_chunk(&decode_pdf_bytes(&decoded_bytes), chunks, total_bytes);
                }
            }
        } else {
            idx += 1;
        }
    }
}

fn decode_pdf_bytes(raw: &[u8]) -> String {
    // Handle UTF-16BE BOM (`FE FF`) commonly emitted by Arabic/Unicode PDF generators.
    if raw.len() >= 4 && raw[0] == 0xFE && raw[1] == 0xFF {
        let units: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(raw).into_owned()
}

fn push_pdf_text_chunk(text: &str, chunks: &mut Vec<String>, total_bytes: &mut usize) {
    let cleaned = text.trim();
    if cleaned.chars().count() >= 2
        && cleaned
            .chars()
            .any(|c| c.is_alphanumeric() || ('\u{0600}'..='\u{06FF}').contains(&c))
    {
        *total_bytes = total_bytes.saturating_add(cleaned.len());
        chunks.push(cleaned.to_owned());
    }
}

fn extract_html(bytes: &[u8]) -> ExtractedDocument {
    let raw = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(raw.len().min(MAX_EXTRACTED_TEXT_BYTES));
    let mut in_tag = false;
    let mut tag_buf = String::new();
    let mut skip_block: Option<&'static str> = None;

    for ch in raw.chars() {
        if out.len() >= MAX_EXTRACTED_TEXT_BYTES {
            break;
        }
        if ch == '<' {
            in_tag = true;
            tag_buf.clear();
            continue;
        }
        if ch == '>' && in_tag {
            in_tag = false;
            let tag_lower = tag_buf.trim().to_ascii_lowercase();
            if tag_lower.starts_with("script") {
                skip_block = Some("script");
            } else if tag_lower.starts_with("style") {
                skip_block = Some("style");
            } else if let Some(active) = skip_block
                && tag_lower.starts_with(&format!("/{active}"))
            {
                skip_block = None;
            }
            if !out.ends_with(' ') {
                out.push(' ');
            }
            continue;
        }
        if in_tag {
            if tag_buf.len() < 32 {
                tag_buf.push(ch);
            }
            continue;
        }
        if skip_block.is_none() {
            out.push(ch);
        }
    }

    let cleaned = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let has_text = !cleaned.is_empty();
    let sections = if has_text {
        vec![IndexedSection {
            location: "HTML Body".to_owned(),
            text: cleaned,
        }]
    } else {
        Vec::new()
    };
    ExtractedDocument {
        sections,
        has_extractable_text: has_text,
        notice: None,
    }
}

fn extract_plain_text(bytes: &[u8]) -> ExtractedDocument {
    let raw = String::from_utf8_lossy(bytes);
    let truncated: String = raw.chars().take(MAX_EXTRACTED_TEXT_BYTES / 2).collect();
    let trimmed = truncated.trim().to_owned();
    let has_text = !trimmed.is_empty();
    let sections = if has_text {
        vec![IndexedSection {
            location: "Document Body".to_owned(),
            text: trimmed,
        }]
    } else {
        Vec::new()
    };
    ExtractedDocument {
        sections,
        has_extractable_text: has_text,
        notice: None,
    }
}

/// Normalizes Arabic and English text for deterministic search matching while never modifying
/// the source document.
#[must_use]
pub fn normalize_for_search(input: &str) -> String {
    let arabic_norm = normalize_text(input, NormOptions::default());
    arabic_norm.to_lowercase()
}

/// Tokenizes normalized Arabic + English text into searchable words/numbers.
#[must_use]
pub fn tokenize_text(input: &str) -> Vec<String> {
    let normalized = normalize_for_search(input);
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in normalized.chars() {
        if ch.is_alphanumeric() || ('\u{0600}'..='\u{06FF}').contains(&ch) {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn parse_query(raw: &str) -> ParsedQuery {
    let mut parsed = ParsedQuery::default();
    let mut chars = raw.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            let _ = chars.next();
            continue;
        }
        if ch == '"' {
            let _ = chars.next();
            let mut phrase_raw = String::new();
            for inner in chars.by_ref() {
                if inner == '"' {
                    break;
                }
                phrase_raw.push(inner);
            }
            let phrase_tokens = tokenize_text(&phrase_raw);
            if phrase_tokens.len() >= 2 {
                parsed.phrases.push(phrase_tokens);
            } else if let Some(single) = phrase_tokens.into_iter().next() {
                parsed.terms.push(single);
            }
            continue;
        }

        let mut word = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '"' {
                break;
            }
            word.push(c);
            let _ = chars.next();
        }
        if let Some(stripped) = word.strip_suffix('*') {
            for tok in tokenize_text(stripped) {
                parsed.prefixes.push(tok);
            }
        } else {
            for tok in tokenize_text(&word) {
                parsed.terms.push(tok);
            }
        }
    }
    parsed
}

#[allow(clippy::too_many_arguments)]
fn score_posting(
    posting: &BTreeMap<u32, u32>,
    n_docs: f64,
    avgdl: f64,
    k1: f64,
    b: f64,
    weight: f64,
    documents: &BTreeMap<u32, IndexedDocument>,
    out: &mut BTreeMap<u32, f64>,
) {
    let df = posting.len() as f64;
    let idf = ((n_docs - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.1);
    for (&doc_id, &tf_u32) in posting {
        let dl = documents
            .get(&doc_id)
            .map_or(avgdl, |d| f64::from(d.token_count.max(1)));
        let tf = f64::from(tf_u32);
        let bm25 = idf * ((tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (dl / avgdl))));
        let candidate = bm25 * weight;
        let entry = out.entry(doc_id).or_insert(0.0);
        if candidate > *entry {
            *entry = candidate;
        }
    }
}

fn document_contains_phrase(doc: &IndexedDocument, normalized_phrase: &str) -> bool {
    if normalized_phrase.is_empty() {
        return true;
    }
    let name_tokens = tokenize_text(&doc.file_name).join(" ");
    if name_tokens.contains(normalized_phrase) {
        return true;
    }
    doc.sections.iter().any(|section| {
        let joined = tokenize_text(&section.text).join(" ");
        joined.contains(normalized_phrase)
    })
}

fn matches_filters(doc: &IndexedDocument, filters: &SearchFilters) -> bool {
    if !filters.file_types.is_empty() && !filters.file_types.contains(&doc.file_type) {
        return false;
    }
    if let Some(prefix) = &filters.folder_prefix {
        let norm_prefix = prefix.replace('\\', "/").to_ascii_lowercase();
        if !doc.folder.to_ascii_lowercase().starts_with(&norm_prefix) {
            return false;
        }
    }
    if let Some(pid) = &filters.project_id
        && doc.project_id.as_deref() != Some(pid.as_str())
    {
        return false;
    }
    if let Some(after) = filters.modified_after_ms
        && doc.modified_ms < after
    {
        return false;
    }
    if let Some(before) = filters.modified_before_ms
        && doc.modified_ms > before
    {
        return false;
    }
    true
}

fn build_best_snippet(doc: &IndexedDocument, highlight_terms: &[String]) -> (String, String) {
    let mut best_location = "File Name".to_owned();
    let mut best_text = doc.file_name.clone();
    let mut best_hits = 0usize;

    for section in &doc.sections {
        let words: Vec<&str> = section.text.split_whitespace().collect();
        let mut section_hits = 0usize;
        let mut first_hit_idx: Option<usize> = None;
        for (idx, word) in words.iter().enumerate() {
            let norm = normalize_for_search(word);
            let matched = highlight_terms.iter().any(|t| {
                !t.is_empty()
                    && (norm.contains(t.as_str())
                        || (t.chars().count() >= 4
                            && tokenize_text(word)
                                .iter()
                                .any(|wt| damerau_levenshtein_le_one(t, wt))))
            });
            if matched {
                section_hits += 1;
                if first_hit_idx.is_none() {
                    first_hit_idx = Some(idx);
                }
            }
        }
        if section_hits > best_hits {
            best_hits = section_hits;
            best_location.clone_from(&section.location);
            let start = first_hit_idx.unwrap_or(0).saturating_sub(6);
            let end = (start + 24).min(words.len());
            let highlighted = words[start..end]
                .iter()
                .map(|word| {
                    let norm = normalize_for_search(word);
                    let matched = highlight_terms.iter().any(|t| {
                        !t.is_empty()
                            && (norm.contains(t.as_str())
                                || (t.chars().count() >= 4
                                    && tokenize_text(word)
                                        .iter()
                                        .any(|wt| damerau_levenshtein_le_one(t, wt))))
                    });
                    if matched {
                        format!("[[{word}]]")
                    } else {
                        (*word).to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            best_text = highlighted;
        }
    }

    (best_location, best_text)
}

/// Fast Damerau-Levenshtein <= 1 check (insertion, deletion, substitution, or adjacent swap).
fn damerau_levenshtein_le_one(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let al = ac.len();
    let bl = bc.len();
    if al.abs_diff(bl) > 1 {
        return false;
    }
    if al == bl {
        let mut diffs = Vec::new();
        for idx in 0..al {
            if ac[idx] != bc[idx] {
                diffs.push(idx);
                if diffs.len() > 2 {
                    return false;
                }
            }
        }
        return diffs.len() == 1
            || (diffs.len() == 2
                && diffs[1] == diffs[0] + 1
                && ac[diffs[0]] == bc[diffs[1]]
                && ac[diffs[1]] == bc[diffs[0]]);
    }

    let (shorter, longer) = if al < bl { (&ac, &bc) } else { (&bc, &ac) };
    let mut si = 0usize;
    let mut li = 0usize;
    let mut skipped = false;
    while si < shorter.len() && li < longer.len() {
        if shorter[si] == longer[li] {
            si += 1;
            li += 1;
        } else if skipped {
            return false;
        } else {
            skipped = true;
            li += 1;
        }
    }
    true
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "SF-SYS-001".to_owned())?;
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let temp_path = path.with_extension(format!("tmp.{nonce}"));
    {
        let mut file = fs::File::create(&temp_path).map_err(|_| "SF-SYS-001".to_owned())?;
        file.write_all(bytes).map_err(|_| "SF-SYS-001".to_owned())?;
        file.sync_all().map_err(|_| "SF-SYS-001".to_owned())?;
    }
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&temp_path, path).map_err(|_| "SF-SYS-001".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    fn temp_test_dir(tag: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!(
            "safi-local-search-{tag}-{}-{nonce}",
            std::process::id()
        ));
        crate::test_result(fs::create_dir_all(&dir));
        dir
    }

    fn write_zip_fixture(path: &Path, entries: &[(&str, &str)]) {
        let file = crate::test_result(fs::File::create(path));
        let mut writer = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, body) in entries {
            crate::test_result(writer.start_file(*name, opts));
            crate::test_result(writer.write_all(body.as_bytes()));
        }
        let _ = crate::test_result(writer.finish());
    }

    fn write_minimal_pdf(path: &Path, text: &str) {
        let content = format!(
            "%PDF-1.4\n1 0 obj<<>>endobj\n2 0 obj<< /Length 44 >>stream\nBT /F1 12 Tf ({text}) Tj ET\nendstream\nendobj\n%%EOF\n"
        );
        crate::test_result(fs::write(path, content.as_bytes()));
    }

    #[test]
    fn indexes_and_searches_all_formats_arabic_english_phrase_prefix_fuzzy_and_encrypted_persistence()
     {
        let root = temp_test_dir("formats");

        // 1. CSV (Arabic + English)
        crate::test_result(fs::write(
            root.join("sales_cairo.csv"),
            "Region,Customer,Revenue\nCairo,أحمد إبراهيم,154000\nAlexandria,Fatima Ali,92500\n",
        ));

        // 2. DOCX fixture
        write_zip_fixture(
            &root.join("board_memo.docx"),
            &[(
                "word/document.xml",
                r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Quarterly net profit margin improved in Cairo warehouse.</w:t></w:r></w:p><w:p><w:r><w:t>تقرير الإيرادات الشهرية لمجموعة صافي البيانات</w:t></w:r></w:p></w:body></w:document>"#,
            )],
        );

        // 3. PPTX fixture (2 slides)
        write_zip_fixture(
            &root.join("q4_deck.pptx"),
            &[
                (
                    "ppt/slides/slide1.xml",
                    r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Executive Overview Q4</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
                ),
                (
                    "ppt/slides/slide2.xml",
                    r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Forecasting anomaly detector reduced churn by 18 percent</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
                ),
            ],
        );

        // 4. PDF with extractable text + scanned PDF without text
        write_minimal_pdf(
            &root.join("audit_certificate.pdf"),
            "Annual compliance verification signed by internal auditor",
        );
        crate::test_result(fs::write(
            root.join("scanned_image_only.pdf"),
            b"%PDF-1.4\n1 0 obj<< /Type /XObject /Subtype /Image >>endobj\n%%EOF\n",
        ));

        // 5. HTML & TXT
        crate::test_result(fs::write(
            root.join("report.html"),
            "<html><head><style>.hidden{display:none}</style></head><body><h1>Consolidated Balance Sheet</h1><p>Operating cashflow remained positive.</p></body></html>",
        ));
        crate::test_result(fs::write(
            root.join("notes.txt"),
            "فواتير الموردين للربع الثالث في القاهرة\nSupplier reconciliation checklist",
        ));

        let mut index = LocalSearchIndex::new();
        let mut progress_events = 0usize;
        let summary = crate::test_result(index.index_folders(
            std::slice::from_ref(&root),
            Some("proj_alpha"),
            None,
            |_| progress_events += 1,
        ));
        assert_eq!(summary.added, 7);
        assert_eq!(summary.updated, 0);
        assert_eq!(summary.unchanged, 0);
        assert!(progress_events >= 7);

        // Arabic Alif/diacritics normalization search: "ايرادات" matches "الإيرادات"
        let ar_hits = index.search_readonly(&SearchQuery {
            query: "ايرادات صافي".to_owned(),
            filters: SearchFilters::default(),
            enable_fuzzy: true,
            limit: 10,
        });
        assert!(ar_hits.iter().any(|h| h.file_name == "board_memo.docx"));

        // Exact phrase search
        let phrase_hits = index.search_readonly(&SearchQuery {
            query: "\"net profit margin\"".to_owned(),
            filters: SearchFilters::default(),
            enable_fuzzy: false,
            limit: 10,
        });
        assert_eq!(phrase_hits.len(), 1);
        assert_eq!(phrase_hits[0].file_name, "board_memo.docx");
        assert!(phrase_hits[0].snippet.contains("[[profit]]"));

        // Prefix search on PPTX slide 2 preserving exact slide location
        let prefix_hits = index.search_readonly(&SearchQuery {
            query: "forecast*".to_owned(),
            filters: SearchFilters {
                file_types: vec![IndexedFileType::Pptx],
                ..SearchFilters::default()
            },
            enable_fuzzy: false,
            limit: 10,
        });
        assert_eq!(prefix_hits.len(), 1);
        assert_eq!(prefix_hits[0].match_location, "Slide 2");

        // Typo-tolerant fuzzy search ("complaince" -> "compliance" in PDF)
        let fuzzy_hits = index.search_readonly(&SearchQuery {
            query: "complaince".to_owned(),
            filters: SearchFilters::default(),
            enable_fuzzy: true,
            limit: 10,
        });
        assert!(
            fuzzy_hits
                .iter()
                .any(|h| h.file_name == "audit_certificate.pdf")
        );

        // Scanned PDF honestly marked non-extractable (no fake OCR)
        let scanned_doc = index
            .get_document_by_path(&root.join("scanned_image_only.pdf"))
            .unwrap_or_else(|| panic!("scanned pdf should be inventoried"));
        assert!(!scanned_doc.has_extractable_text);
        assert!(scanned_doc.extraction_notice.is_some());

        // Incremental re-index: modify one file, delete another, leave 5 unchanged.
        crate::test_result(fs::write(
            root.join("notes.txt"),
            "Updated treasury liquidity ledger",
        ));
        crate::test_result(fs::remove_file(root.join("report.html")));
        let inc = crate::test_result(index.index_folders(
            std::slice::from_ref(&root),
            Some("proj_alpha"),
            None,
            |_| {},
        ));
        assert_eq!(inc.added, 0);
        assert_eq!(inc.updated, 1);
        assert_eq!(inc.unchanged, 5);
        assert_eq!(inc.removed, 1);
        assert!(
            index
                .search_readonly(&SearchQuery {
                    query: "cashflow".to_owned(),
                    filters: SearchFilters::default(),
                    enable_fuzzy: false,
                    limit: 5,
                })
                .is_empty(),
            "deleted HTML file must no longer appear in search results"
        );

        // Save encrypted index and reload offline read-only; verify zero plaintext on disk.
        let key = crate::test_result(VaultKey::derive_from_passphrase(
            "search-index-vault-pass-2026",
            &[7u8; 16],
        ));
        let index_file = root.join("local_search.safi.enc");
        crate::test_result(index.save_encrypted(&index_file, &key));
        let raw_disk = crate::test_result(fs::read(&index_file));
        assert!(
            !raw_disk
                .windows(b"liquidity".len())
                .any(|w| w == b"liquidity")
        );

        let reloaded = crate::test_result(LocalSearchIndex::load_encrypted(&index_file, &key));
        let offline_hits = reloaded.search_readonly(&SearchQuery {
            query: "liquidity".to_owned(),
            filters: SearchFilters {
                project_id: Some("proj_alpha".to_owned()),
                ..SearchFilters::default()
            },
            enable_fuzzy: false,
            limit: 5,
        });
        assert_eq!(offline_hits.len(), 1);
        assert_eq!(offline_hits[0].file_name, "notes.txt");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scale_benchmark_100_and_1000_local_files_with_incremental_skip() {
        let root = temp_test_dir("scale-1000");

        for i in 0..1000usize {
            let content = if i == 777 {
                format!("Invoice #{i} Cairo Regional Treasury SpecialAuditToken777 amount 48200")
            } else {
                format!(
                    "Monthly ledger record #{i} branch Alexandria revenue {}",
                    i * 13 + 7
                )
            };
            crate::test_result(fs::write(root.join(format!("doc_{i:04}.txt")), content));
        }

        // Benchmark 100-file subset first
        let mut index_100 = LocalSearchIndex::new();
        let t100 = Instant::now();
        for i in 0..100usize {
            let _ = crate::test_result(index_100.index_single_file(
                &root.join(format!("doc_{i:04}.txt")),
                None,
                None,
            ));
        }
        let elapsed_100_ms = t100.elapsed().as_millis();
        assert_eq!(index_100.document_count(), 100);
        assert!(
            elapsed_100_ms < 10_000,
            "100-file index took {elapsed_100_ms}ms"
        );

        // Benchmark full 1,000-file folder indexing + search + incremental re-index
        let mut index_1000 = LocalSearchIndex::new();
        let t1000 = Instant::now();
        let summary = crate::test_result(index_1000.index_folders(
            std::slice::from_ref(&root),
            None,
            None,
            |_| {},
        ));
        let elapsed_1000_ms = t1000.elapsed().as_millis();
        assert_eq!(summary.added, 1000);
        assert_eq!(index_1000.document_count(), 1000);

        let t_query = Instant::now();
        let hits = index_1000.search_readonly(&SearchQuery {
            query: "SpecialAuditToken777".to_owned(),
            filters: SearchFilters::default(),
            enable_fuzzy: false,
            limit: 10,
        });
        let query_ms = t_query.elapsed().as_millis();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file_name, "doc_0777.txt");
        assert!(query_ms < 2_000, "1000-file search took {query_ms}ms");

        let inc = crate::test_result(index_1000.index_folders(
            std::slice::from_ref(&root),
            None,
            None,
            |_| {},
        ));
        assert_eq!(inc.added, 0);
        assert_eq!(inc.updated, 0);
        assert_eq!(inc.unchanged, 1000);

        // Keep benchmark timings asserted sensibly without leaking any user data.
        assert!(
            elapsed_1000_ms < 30_000,
            "1000-file index took {elapsed_1000_ms}ms"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn extracts_flatedecode_compressed_pdf_text_and_flags_scanned_pdf_honestly() {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;

        let root = temp_test_dir("pdf-flate");

        // Build a real FlateDecode-compressed PDF stream (ASCII + UTF-16BE Arabic hex string)
        let stream_plain = b"BT /F1 12 Tf (Quarterly Revenue Cairo FlateDecode2026) Tj <FEFF063506270641064A> Tj ET";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        crate::test_result(encoder.write_all(stream_plain));
        let compressed = crate::test_result(encoder.finish());

        let mut pdf_bytes = Vec::new();
        pdf_bytes.extend_from_slice(b"%PDF-1.4\n1 0 obj << /Filter /FlateDecode /Length ");
        pdf_bytes.extend_from_slice(compressed.len().to_string().as_bytes());
        pdf_bytes.extend_from_slice(b" >>\nstream\n");
        pdf_bytes.extend_from_slice(&compressed);
        pdf_bytes.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        let flate_pdf = root.join("compressed_report.pdf");
        crate::test_result(fs::write(&flate_pdf, &pdf_bytes));

        let scanned_pdf = root.join("scanned_only.pdf");
        crate::test_result(fs::write(
            &scanned_pdf,
            b"%PDF-1.4\n1 0 obj << /Type /XObject /Subtype /Image >> endobj\n%%EOF\n",
        ));

        let mut index = LocalSearchIndex::new();
        let summary = crate::test_result(index.index_folders(
            std::slice::from_ref(&root),
            None,
            None,
            |_| {},
        ));
        assert_eq!(summary.added, 2);

        let hits_en = index.search_readonly(&SearchQuery {
            query: "FlateDecode2026".to_owned(),
            filters: SearchFilters {
                file_types: vec![IndexedFileType::Pdf],
                ..SearchFilters::default()
            },
            enable_fuzzy: false,
            limit: 5,
        });
        assert_eq!(hits_en.len(), 1);
        assert_eq!(hits_en[0].file_name, "compressed_report.pdf");

        let hits_ar = index.search_readonly(&SearchQuery {
            query: "صافي".to_owned(),
            filters: SearchFilters {
                file_types: vec![IndexedFileType::Pdf],
                ..SearchFilters::default()
            },
            enable_fuzzy: false,
            limit: 5,
        });
        assert_eq!(hits_ar.len(), 1);
        assert_eq!(hits_ar[0].file_name, "compressed_report.pdf");

        let scanned_doc = index
            .get_document_by_path(&scanned_pdf)
            .unwrap_or_else(|| panic!("scanned pdf should be catalogued"));
        assert!(!scanned_doc.has_extractable_text);
        assert!(scanned_doc.extraction_notice.is_some());

        let _ = fs::remove_dir_all(&root);
    }
}
