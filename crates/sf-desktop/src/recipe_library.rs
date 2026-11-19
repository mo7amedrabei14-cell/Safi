//! Local Recipe Library, Official Template Customization (`Save as New Recipe`),
//! Premium `.safitemplate` Export / Import, and `Share with SafiData` Contribution Builder
//! (Sections 11, 12, 13).
//!
//! Built directly on `sf_recipe::SafiRecipe` (zero duplicate recipe schemas).
//!
//! Key guarantees:
//! - Editing an Official Template creates a separate user-owned working copy and NEVER modifies
//!   the original Official Template.
//! - Duplicate recipe names NEVER silently overwrite: caller must choose `Rename`, `Skip`,
//!   or `ConfirmedReplace`.
//! - Premium `.safitemplate` Export/Import enforces server-verified entitlement (`SF-RCP-005`),
//!   strictly validates schema version + allowlisted operations + BLAKE3 integrity + size cap,
//!   and NEVER embeds dataset rows, tokens, passwords, or personal local paths.
//! - Community submission (`Share with SafiData`) blocks secrets & dataset rows (`SF-RCP-006`)
//!   and mandates explicit consent.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sf_recipe::recipe::{RecipeMatchReport, RecipeOperation, RecipeStep, SafiRecipe};

use crate::vault::{VaultKey, decrypt_vault_payload, encrypt_vault_payload};

/// Maximum permitted `.safitemplate` file size (512 KiB).
pub const MAX_SAFI_TEMPLATE_BYTES: usize = 512 * 1024;
/// Current portable `.safitemplate` envelope schema version.
pub const SAFI_TEMPLATE_SCHEMA_VERSION: u32 = 1;
/// Current Desktop app compatibility string.
pub const DESKTOP_COMPATIBLE_VERSION: &str = "1.0.0";

/// Distinguishes the 5 recipe/template provenance classes required by Section 11.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeOriginClass {
    OfficialTemplate,
    MyRecipe,
    ImportedRecipe,
    CommunityContribution,
    PublishedCommunityTemplate,
}

/// Duplicate-name conflict policy (NEVER silent overwrite).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameConflictPolicy {
    FailOnDuplicate,
    Rename,
    Skip,
    ConfirmedReplace,
}

/// Stored recipe record inside the local Desktop library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalRecipeRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub recipe_type: String,
    pub origin: RecipeOriginClass,
    pub source_template_id: Option<String>,
    pub required_columns: Vec<String>,
    pub recipe: SafiRecipe,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

/// Standalone portable `.safitemplate` file structure (ZERO dataset rows, ZERO credentials).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortableSafiTemplateFile {
    pub format: String,
    pub schema_version: u32,
    pub compatible_app_version: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub recipe_type: String,
    pub required_columns: Vec<String>,
    pub steps: Vec<RecipeStep>,
    pub checksum_blake3: String,
}

/// Sanitized payload produced for `Share with SafiData` community review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunityContributionPayload {
    pub name: String,
    pub description: String,
    pub recipe_type: String,
    pub schema_version: u32,
    pub app_version: String,
    pub required_columns: Vec<String>,
    pub steps: Vec<RecipeStep>,
    pub consent_accepted: bool,
    pub consent_terms_version: String,
    pub show_contributor_attribution: bool,
    pub pii_warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedLibrary {
    schema_version: u32,
    my_recipes: Vec<LocalRecipeRecord>,
    cached_official: Vec<LocalRecipeRecord>,
}

/// Persistent local Desktop recipe & template manager (strictly on-device).
#[derive(Debug)]
pub struct LocalRecipeLibrary {
    storage_dir: PathBuf,
    vault_key: Option<VaultKey>,
    official_templates: BTreeMap<String, LocalRecipeRecord>,
    my_recipes: BTreeMap<String, LocalRecipeRecord>,
}

impl LocalRecipeLibrary {
    /// Open or initialize the local recipe library inside `storage_dir`.
    ///
    /// # Errors
    /// Returns `SF-SYS-001` or `SF-SYS-024` if existing persisted data is corrupt or fails decryption.
    pub fn open(storage_dir: PathBuf, vault_key: Option<VaultKey>) -> Result<Self, String> {
        fs::create_dir_all(&storage_dir).map_err(|_| "SF-SYS-016".to_owned())?;
        let mut official_templates = BTreeMap::new();
        for tpl in default_official_templates() {
            let _ = official_templates.insert(tpl.id.clone(), tpl);
        }
        let mut my_recipes = BTreeMap::new();

        let enc_path = storage_dir.join("recipe_library.safi.enc");
        let json_path = storage_dir.join("recipe_library.json");
        let raw_opt = if enc_path.exists() {
            let key = vault_key.as_ref().ok_or_else(|| "SF-SYS-024".to_owned())?;
            let cipher = fs::read(&enc_path).map_err(|_| "SF-SYS-001".to_owned())?;
            let plain = decrypt_vault_payload(key, &cipher).map_err(|_| "SF-SYS-024".to_owned())?;
            Some(String::from_utf8(plain).map_err(|_| "SF-SYS-024".to_owned())?)
        } else if json_path.exists() {
            Some(fs::read_to_string(&json_path).map_err(|_| "SF-SYS-001".to_owned())?)
        } else {
            None
        };

        if let Some(raw) = raw_opt {
            let persisted: PersistedLibrary =
                serde_json::from_str(&raw).map_err(|_| "SF-RCP-001".to_owned())?;
            for rec in persisted.cached_official {
                let _ = official_templates.insert(rec.id.clone(), rec);
            }
            for rec in persisted.my_recipes {
                let _ = my_recipes.insert(rec.id.clone(), rec);
            }
        }

        Ok(Self {
            storage_dir,
            vault_key,
            official_templates,
            my_recipes,
        })
    }

    /// List immutable official templates cached locally.
    #[must_use]
    pub fn list_official_templates(&self) -> Vec<LocalRecipeRecord> {
        self.official_templates.values().cloned().collect()
    }

    /// Synchronize server-authoritative official templates into the local read-only cache
    /// without ever mutating any user-owned recipes in `My Recipes`.
    ///
    /// # Errors
    /// Returns `SF-SYS-016` / `SF-SYS-001` if atomic persistence fails.
    pub fn sync_official_templates(
        &mut self,
        incoming: Vec<LocalRecipeRecord>,
    ) -> Result<usize, String> {
        for mut rec in incoming {
            if rec.id.trim().is_empty() || rec.name.trim().is_empty() || rec.recipe.steps.is_empty()
            {
                continue;
            }
            if rec.origin != RecipeOriginClass::PublishedCommunityTemplate {
                rec.origin = RecipeOriginClass::OfficialTemplate;
            }
            let _ = self.official_templates.insert(rec.id.clone(), rec);
        }
        self.persist_atomic()?;
        Ok(self.official_templates.len())
    }

    /// List all user-owned recipes (`My Recipes` and `Imported Recipes`).
    #[must_use]
    pub fn list_my_recipes(&self) -> Vec<LocalRecipeRecord> {
        self.my_recipes.values().cloned().collect()
    }

    /// Fetch a single official template by ID (immutable read-only copy).
    #[must_use]
    pub fn get_official_template(&self, id: &str) -> Option<&LocalRecipeRecord> {
        self.official_templates.get(id)
    }

    /// Fetch a single user recipe by ID.
    #[must_use]
    pub fn get_my_recipe(&self, id: &str) -> Option<&LocalRecipeRecord> {
        self.my_recipes.get(id)
    }

    /// Open an official template into an editable working copy WITHOUT modifying the official original.
    ///
    /// # Errors
    /// Returns `SF-RCP-001` if `template_id` does not exist.
    pub fn clone_official_for_editing(
        &self,
        template_id: &str,
        now_ms: u64,
    ) -> Result<LocalRecipeRecord, String> {
        let orig = self
            .official_templates
            .get(template_id)
            .ok_or_else(|| "SF-RCP-001".to_owned())?;
        let mut copy = orig.clone();
        copy.id = format!("draft_{now_ms}_{}", orig.id);
        copy.origin = RecipeOriginClass::MyRecipe;
        copy.source_template_id = Some(orig.id.clone());
        copy.recipe.id = copy.id.clone();
        copy.created_at_ms = now_ms;
        copy.updated_at_ms = now_ms;
        Ok(copy)
    }

    /// Customize a working copy: add a step, remove a step, or reorder steps without mutating any official template.
    pub fn customize_working_copy_steps(
        working_copy: &mut LocalRecipeRecord,
        new_steps: Vec<RecipeStep>,
        now_ms: u64,
    ) -> Result<(), String> {
        if new_steps.is_empty() || new_steps.len() > 64 {
            return Err("SF-RCP-001".to_owned());
        }
        working_copy.recipe.steps = new_steps;
        working_copy.recipe.content_hash = Some(
            working_copy
                .recipe
                .compute_content_hash()
                .map_err(|_| "SF-RCP-001".to_owned())?,
        );
        working_copy.updated_at_ms = now_ms;
        Ok(())
    }

    /// Save a customized working copy or user recipe into `My Recipes` as an independent recipe
    /// (`Save as New Recipe`), handling duplicate names explicitly (NEVER silently overwriting).
    ///
    /// # Errors
    /// Returns `SF-RCP-001` on invalid name/steps or `"DUPLICATE_RECIPE_NAME"` if duplicate and `FailOnDuplicate`.
    pub fn save_as_new_recipe(
        &mut self,
        mut working_copy: LocalRecipeRecord,
        new_name: &str,
        description: &str,
        category: &str,
        conflict_policy: NameConflictPolicy,
        now_ms: u64,
    ) -> Result<Option<LocalRecipeRecord>, String> {
        let base_name = new_name.trim();
        if base_name.is_empty() || base_name.len() > 160 || working_copy.recipe.steps.is_empty() {
            return Err("SF-RCP-001".to_owned());
        }
        let existing_id = self
            .my_recipes
            .values()
            .find(|r| r.name.eq_ignore_ascii_case(base_name))
            .map(|r| r.id.clone());

        let final_name = match (existing_id.as_ref(), conflict_policy) {
            (Some(_), NameConflictPolicy::FailOnDuplicate) => {
                return Err("DUPLICATE_RECIPE_NAME".to_owned());
            }
            (Some(_), NameConflictPolicy::Skip) => return Ok(None),
            (Some(_), NameConflictPolicy::Rename) => self.next_unique_name(base_name),
            (Some(old_id), NameConflictPolicy::ConfirmedReplace) => {
                let _ = self.my_recipes.remove(old_id);
                base_name.to_owned()
            }
            (None, _) => base_name.to_owned(),
        };

        let new_id = format!(
            "rcp_{}_{}",
            now_ms,
            &sf_recipe::blake3_hex(
                format!("{final_name}:{now_ms}:{}", self.my_recipes.len()).as_bytes()
            )[..12]
        );
        working_copy.id = new_id.clone();
        working_copy.name = final_name.clone();
        working_copy.description = description.trim().to_owned();
        working_copy.category = category.trim().to_owned();
        if working_copy.origin == RecipeOriginClass::OfficialTemplate {
            working_copy.origin = RecipeOriginClass::MyRecipe;
        }
        working_copy.recipe.id = new_id.clone();
        working_copy.recipe.name = final_name;
        working_copy.recipe.content_hash = Some(
            working_copy
                .recipe
                .compute_content_hash()
                .map_err(|_| "SF-RCP-001".to_owned())?,
        );
        working_copy.created_at_ms = now_ms;
        working_copy.updated_at_ms = now_ms;

        let _ = self.my_recipes.insert(new_id, working_copy.clone());
        self.persist_atomic()?;
        Ok(Some(working_copy))
    }

    /// Edit an existing user-owned recipe in place (never permitted on Official Templates).
    ///
    /// # Errors
    /// Returns `SF-RCP-001` if recipe does not exist or steps are invalid.
    pub fn update_my_recipe(
        &mut self,
        id: &str,
        new_steps: Vec<RecipeStep>,
        new_description: Option<&str>,
        now_ms: u64,
    ) -> Result<LocalRecipeRecord, String> {
        let rec = self
            .my_recipes
            .get_mut(id)
            .ok_or_else(|| "SF-RCP-001".to_owned())?;
        Self::customize_working_copy_steps(rec, new_steps, now_ms)?;
        if let Some(desc) = new_description {
            rec.description = desc.trim().to_owned();
        }
        let cloned = rec.clone();
        self.persist_atomic()?;
        Ok(cloned)
    }

    /// Rename a user recipe (rejects colliding names).
    ///
    /// # Errors
    /// Returns `SF-RCP-001` or `"DUPLICATE_RECIPE_NAME"`.
    pub fn rename_my_recipe(
        &mut self,
        id: &str,
        new_name: &str,
        now_ms: u64,
    ) -> Result<LocalRecipeRecord, String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() || trimmed.len() > 160 {
            return Err("SF-RCP-001".to_owned());
        }
        if self
            .my_recipes
            .values()
            .any(|r| r.id != id && r.name.eq_ignore_ascii_case(trimmed))
        {
            return Err("DUPLICATE_RECIPE_NAME".to_owned());
        }
        let rec = self
            .my_recipes
            .get_mut(id)
            .ok_or_else(|| "SF-RCP-001".to_owned())?;
        rec.name = trimmed.to_owned();
        rec.recipe.name = trimmed.to_owned();
        rec.updated_at_ms = now_ms;
        let cloned = rec.clone();
        self.persist_atomic()?;
        Ok(cloned)
    }

    /// Duplicate an existing user recipe under a distinct generated copy name.
    ///
    /// # Errors
    /// Returns `SF-RCP-001` if `id` is missing.
    pub fn duplicate_my_recipe(
        &mut self,
        id: &str,
        now_ms: u64,
    ) -> Result<LocalRecipeRecord, String> {
        let orig = self
            .my_recipes
            .get(id)
            .cloned()
            .ok_or_else(|| "SF-RCP-001".to_owned())?;
        let copy_name = self.next_unique_name(&format!("{} (Copy)", orig.name));
        self.save_as_new_recipe(
            orig.clone(),
            &copy_name,
            &orig.description,
            &orig.category,
            NameConflictPolicy::Rename,
            now_ms,
        )?
        .ok_or_else(|| "SF-RCP-001".to_owned())
    }

    /// Delete a user recipe only when explicitly confirmed.
    ///
    /// # Errors
    /// Returns `SF-RCP-001` if unconfirmed or missing.
    pub fn delete_my_recipe(&mut self, id: &str, confirmed: bool) -> Result<(), String> {
        if !confirmed || self.my_recipes.remove(id).is_none() {
            return Err("SF-RCP-001".to_owned());
        }
        self.persist_atomic()
    }

    /// Match a recipe's required columns against a new target dataset's columns (supports applying
    /// saved/imported recipes to completely different datasets).
    ///
    /// # Errors
    /// Returns `SF-RCP-001` if `id` is missing.
    pub fn match_recipe_to_dataset(
        &self,
        id: &str,
        dataset_columns: &[String],
    ) -> Result<RecipeMatchReport, String> {
        let rec = self
            .my_recipes
            .get(id)
            .or_else(|| self.official_templates.get(id))
            .ok_or_else(|| "SF-RCP-001".to_owned())?;
        Ok(rec
            .recipe
            .match_columns_on_new_file(&rec.required_columns, dataset_columns))
    }

    /// Premium Feature (Section 12): Export a recipe as a standalone `.safitemplate` file.
    /// Strictly requires `premium_entitlement_verified == true` (`SF-RCP-005` otherwise).
    /// Never writes dataset rows, tokens, passwords, or local file paths.
    ///
    /// # Errors
    /// Returns `SF-RCP-005`, `SF-RCP-001`, or `SF-SYS-016`.
    pub fn export_safitemplate(
        &self,
        recipe_id: &str,
        target_path: &Path,
        premium_entitlement_verified: bool,
    ) -> Result<PortableSafiTemplateFile, String> {
        if !premium_entitlement_verified {
            return Err("SF-RCP-005".to_owned());
        }
        if target_path.extension().and_then(|s| s.to_str()) != Some("safitemplate") {
            return Err("SF-RCP-001".to_owned());
        }
        let rec = self
            .my_recipes
            .get(recipe_id)
            .ok_or_else(|| "SF-RCP-001".to_owned())?;

        // Strip any accidental secrets before export.
        let (blocked, _) =
            scan_local_steps_for_secrets(&rec.name, &rec.description, &rec.recipe.steps);
        if !blocked.is_empty() {
            return Err("SF-RCP-006".to_owned());
        }

        let checksum_blake3 = compute_portable_checksum(
            SAFI_TEMPLATE_SCHEMA_VERSION,
            DESKTOP_COMPATIBLE_VERSION,
            &rec.name,
            &rec.description,
            &rec.category,
            &rec.recipe_type,
            &rec.required_columns,
            &rec.recipe.steps,
        )?;
        let portable = PortableSafiTemplateFile {
            format: "safitemplate".to_owned(),
            schema_version: SAFI_TEMPLATE_SCHEMA_VERSION,
            compatible_app_version: DESKTOP_COMPATIBLE_VERSION.to_owned(),
            name: rec.name.clone(),
            description: rec.description.clone(),
            category: rec.category.clone(),
            recipe_type: rec.recipe_type.clone(),
            required_columns: rec.required_columns.clone(),
            steps: rec.recipe.steps.clone(),
            checksum_blake3,
        };
        let encoded = serde_json::to_vec_pretty(&portable).map_err(|_| "SF-RCP-001".to_owned())?;
        if encoded.len() > MAX_SAFI_TEMPLATE_BYTES {
            return Err("SF-RCP-001".to_owned());
        }
        write_atomic_bytes(target_path, &encoded)?;
        Ok(portable)
    }

    /// Premium Feature (Section 12): Validate and import a `.safitemplate` file into `My Recipes`.
    /// Rejects non-entitled accounts (`SF-RCP-005`), unknown fields, oversized files, tampered
    /// checksums (`SF-CAL-008`), scripts/secrets (`SF-RCP-006`), and never leaves partial state on failure.
    ///
    /// # Errors
    /// Returns `SF-RCP-005`, `SF-RCP-001`, `SF-RCP-006`, `SF-CAL-008`, or `"DUPLICATE_RECIPE_NAME"`.
    pub fn import_safitemplate(
        &mut self,
        source_path: &Path,
        premium_entitlement_verified: bool,
        conflict_policy: NameConflictPolicy,
        now_ms: u64,
    ) -> Result<Option<LocalRecipeRecord>, String> {
        if !premium_entitlement_verified {
            return Err("SF-RCP-005".to_owned());
        }
        // Reject path traversal components.
        if source_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("SF-RCP-001".to_owned());
        }
        let meta = fs::metadata(source_path).map_err(|_| "SF-RCP-001".to_owned())?;
        if meta.len() == 0 || meta.len() > MAX_SAFI_TEMPLATE_BYTES as u64 {
            return Err("SF-RCP-001".to_owned());
        }
        let raw = fs::read(source_path).map_err(|_| "SF-RCP-001".to_owned())?;
        let portable: PortableSafiTemplateFile =
            serde_json::from_slice(&raw).map_err(|_| "SF-RCP-001".to_owned())?;

        if portable.format != "safitemplate"
            || portable.schema_version != SAFI_TEMPLATE_SCHEMA_VERSION
            || portable.steps.is_empty()
            || portable.steps.len() > 64
        {
            return Err("SF-RCP-001".to_owned());
        }
        let expected_hash = compute_portable_checksum(
            portable.schema_version,
            &portable.compatible_app_version,
            &portable.name,
            &portable.description,
            &portable.category,
            &portable.recipe_type,
            &portable.required_columns,
            &portable.steps,
        )?;
        if expected_hash != portable.checksum_blake3 {
            return Err("SF-CAL-008".to_owned());
        }
        let (blocked, _) =
            scan_local_steps_for_secrets(&portable.name, &portable.description, &portable.steps);
        if !blocked.is_empty() {
            return Err("SF-RCP-006".to_owned());
        }

        let imported = LocalRecipeRecord {
            id: String::new(),
            name: portable.name.clone(),
            description: portable.description.clone(),
            category: portable.category.clone(),
            recipe_type: portable.recipe_type.clone(),
            origin: RecipeOriginClass::ImportedRecipe,
            source_template_id: None,
            required_columns: portable.required_columns,
            recipe: SafiRecipe {
                schema_version: 1,
                id: String::new(),
                name: portable.name.clone(),
                created_with: Some(portable.compatible_app_version),
                region: None,
                column_rules: None,
                steps: portable.steps,
                content_hash: None,
            },
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        self.save_as_new_recipe(
            imported,
            &portable.name,
            &portable.description,
            &portable.category,
            conflict_policy,
            now_ms,
        )
    }

    /// Build a sanitized `Share with SafiData` community contribution payload with mandatory consent
    /// and local secret/PII inspection (Section 13).
    ///
    /// # Errors
    /// Returns `SF-RCP-006` when consent is missing or embedded secrets/scripts/data rows are found.
    pub fn prepare_community_contribution(
        &self,
        recipe_id: &str,
        consent_accepted: bool,
        consent_terms_version: &str,
        show_contributor_attribution: bool,
    ) -> Result<CommunityContributionPayload, String> {
        if !consent_accepted || consent_terms_version.trim().is_empty() {
            return Err("SF-RCP-006".to_owned());
        }
        let rec = self
            .my_recipes
            .get(recipe_id)
            .ok_or_else(|| "SF-RCP-001".to_owned())?;
        let (blocked, pii_warnings) =
            scan_local_steps_for_secrets(&rec.name, &rec.description, &rec.recipe.steps);
        if !blocked.is_empty() {
            return Err("SF-RCP-006".to_owned());
        }
        Ok(CommunityContributionPayload {
            name: rec.name.clone(),
            description: rec.description.clone(),
            recipe_type: rec.recipe_type.clone(),
            schema_version: 1,
            app_version: DESKTOP_COMPATIBLE_VERSION.to_owned(),
            required_columns: rec.required_columns.clone(),
            steps: rec.recipe.steps.clone(),
            consent_accepted: true,
            consent_terms_version: consent_terms_version.trim().to_owned(),
            show_contributor_attribution,
            pii_warnings,
        })
    }

    fn next_unique_name(&self, base: &str) -> String {
        let mut idx = 2usize;
        loop {
            let candidate = format!("{base} ({idx})");
            if !self
                .my_recipes
                .values()
                .any(|r| r.name.eq_ignore_ascii_case(&candidate))
            {
                return candidate;
            }
            idx = idx.saturating_add(1);
        }
    }

    fn persist_atomic(&self) -> Result<(), String> {
        let data = PersistedLibrary {
            schema_version: 1,
            my_recipes: self.my_recipes.values().cloned().collect(),
            cached_official: self.official_templates.values().cloned().collect(),
        };
        let encoded = serde_json::to_vec_pretty(&data).map_err(|_| "SF-SYS-001".to_owned())?;
        if let Some(ref key) = self.vault_key {
            let cipher =
                encrypt_vault_payload(key, &encoded).map_err(|_| "SF-SYS-023".to_owned())?;
            write_atomic_bytes(&self.storage_dir.join("recipe_library.safi.enc"), &cipher)
        } else {
            write_atomic_bytes(&self.storage_dir.join("recipe_library.json"), &encoded)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_portable_checksum(
    schema_version: u32,
    compatible_app_version: &str,
    name: &str,
    description: &str,
    category: &str,
    recipe_type: &str,
    required_columns: &[String],
    steps: &[RecipeStep],
) -> Result<String, String> {
    let payload = serde_json::json!({
        "schema_version": schema_version,
        "compatible_app_version": compatible_app_version,
        "name": name,
        "description": description,
        "category": category,
        "recipe_type": recipe_type,
        "required_columns": required_columns,
        "steps": steps,
    });
    let bytes = serde_json::to_vec(&payload).map_err(|_| "SF-RCP-001".to_owned())?;
    Ok(sf_recipe::blake3_hex(&bytes))
}

fn scan_local_steps_for_secrets(
    name: &str,
    description: &str,
    steps: &[RecipeStep],
) -> (Vec<String>, Vec<String>) {
    let mut blocked = Vec::new();
    let mut warnings = Vec::new();
    let serialized = format!(
        "{name}\n{description}\n{}",
        serde_json::to_string(steps).unwrap_or_default()
    );
    let lower = serialized.to_ascii_lowercase();
    for needle in [
        "sfs_",
        "sfw_",
        "sfk_live_",
        "sk-",
        "bearer ",
        "-----begin",
        "postgres://",
        "postgresql://",
        "<script",
        "javascript:",
        "eval(",
        "powershell",
        "cmd.exe",
        "\"rows\"",
        "\"data_rows\"",
        "\"password\"",
        "\"api_key\"",
        "\"secret\"",
    ] {
        if lower.contains(needle) {
            blocked.push(format!("Blocked sensitive/unsafe pattern: {needle}"));
        }
    }
    if serialized.contains('@') && serialized.contains('.') {
        warnings.push("Possible email address in recipe parameters".to_owned());
    }
    if lower.contains("c:\\users\\") || lower.contains("/home/") || lower.contains("/users/") {
        warnings.push("Possible local personal path in recipe parameters".to_owned());
    }
    (blocked, warnings)
}

fn write_atomic_bytes(target: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|_| "SF-SYS-016".to_owned())?;
    }
    let tmp = target.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp).map_err(|_| "SF-SYS-016".to_owned())?;
        file.write_all(bytes).map_err(|_| "SF-SYS-016".to_owned())?;
        file.sync_all().map_err(|_| "SF-SYS-016".to_owned())?;
    }
    fs::rename(&tmp, target).map_err(|_| "SF-SYS-016".to_owned())?;
    Ok(())
}

fn default_official_templates() -> Vec<LocalRecipeRecord> {
    let steps_clean = vec![
        RecipeStep {
            id: "s1".to_owned(),
            op: RecipeOperation::NormalizeText,
            columns: Some(vec!["full_name".to_owned()]),
            params: serde_json::json!({ "trim": true }),
            confidence: Some(1.0),
            reversible: true,
        },
        RecipeStep {
            id: "s2".to_owned(),
            op: RecipeOperation::Dedupe,
            columns: None,
            params: serde_json::json!({ "exact": true }),
            confidence: Some(1.0),
            reversible: true,
        },
    ];
    let rec = SafiRecipe {
        schema_version: 1,
        id: "official-arabic-cleaning".to_owned(),
        name: "Arabic & Multilingual Customer Directory Cleaner".to_owned(),
        created_with: Some(DESKTOP_COMPATIBLE_VERSION.to_owned()),
        region: None,
        column_rules: None,
        steps: steps_clean,
        content_hash: None,
    };
    vec![LocalRecipeRecord {
        id: "official-arabic-cleaning".to_owned(),
        name: "Arabic & Multilingual Customer Directory Cleaner".to_owned(),
        description: "Normalizes names, trims whitespace, and removes exact duplicates.".to_owned(),
        category: "cleaning".to_owned(),
        recipe_type: "cleaning".to_owned(),
        origin: RecipeOriginClass::OfficialTemplate,
        source_template_id: None,
        required_columns: vec!["full_name".to_owned(), "city".to_owned()],
        recipe: rec,
        created_at_ms: 1_700_000_000_000,
        updated_at_ms: 1_700_000_000_000,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_official_template_save_as_new_restart_duplicate_and_safitemplate_exchange() {
        let dir_a = crate::test_result(tempfile::tempdir());
        let dir_b = crate::test_result(tempfile::tempdir());
        let salt = [0x21u8; 16];
        let vault_a = crate::test_result(VaultKey::derive_from_passphrase("vault-pass-a", &salt));
        let vault_b = crate::test_result(VaultKey::derive_from_passphrase("vault-pass-b", &salt));

        let mut lib_a = crate::test_result(LocalRecipeLibrary::open(
            dir_a.path().to_path_buf(),
            Some(vault_a.clone()),
        ));

        // 1. Open official template, edit steps, Save as New Recipe -> original official template untouched!
        let mut draft = crate::test_result(
            lib_a.clone_official_for_editing("official-arabic-cleaning", 10_000),
        );
        let mut custom_steps = draft.recipe.steps.clone();
        custom_steps.push(RecipeStep {
            id: "s3".to_owned(),
            op: RecipeOperation::FillBlank,
            columns: Some(vec!["city".to_owned()]),
            params: serde_json::json!({ "value": "Cairo" }),
            confidence: Some(1.0),
            reversible: true,
        });
        crate::test_result(LocalRecipeLibrary::customize_working_copy_steps(
            &mut draft,
            custom_steps,
            10_100,
        ));
        let saved_opt = crate::test_result(lib_a.save_as_new_recipe(
            draft,
            "My Cairo CRM Cleaner",
            "Custom CRM recipe",
            "crm",
            NameConflictPolicy::FailOnDuplicate,
            10_200,
        ));
        let saved = match saved_opt {
            Some(r) => r,
            None => panic!("expected saved recipe"),
        };
        assert_eq!(saved.recipe.steps.len(), 3);
        // Official template STILL has 2 steps (completely unmodified!).
        let orig = lib_a.get_official_template("official-arabic-cleaning");
        assert_eq!(orig.map(|r| r.recipe.steps.len()), Some(2));

        // 2. Duplicate name never silently overwrites.
        assert_eq!(
            lib_a.save_as_new_recipe(
                saved.clone(),
                "My Cairo CRM Cleaner",
                "dup",
                "crm",
                NameConflictPolicy::FailOnDuplicate,
                10_300,
            ),
            Err("DUPLICATE_RECIPE_NAME".to_owned())
        );
        let renamed_opt = crate::test_result(lib_a.save_as_new_recipe(
            saved.clone(),
            "My Cairo CRM Cleaner",
            "dup renamed",
            "crm",
            NameConflictPolicy::Rename,
            10_350,
        ));
        assert_eq!(
            renamed_opt.as_ref().map(|r| r.name.as_str()),
            Some("My Cairo CRM Cleaner (2)")
        );

        // 3. Restart app (reopen encrypted library from disk) -> recipes persist cleanly!
        drop(lib_a);
        let mut reopened_a = crate::test_result(LocalRecipeLibrary::open(
            dir_a.path().to_path_buf(),
            Some(vault_a),
        ));
        assert_eq!(reopened_a.list_my_recipes().len(), 2);
        let dup_copy = crate::test_result(reopened_a.duplicate_my_recipe(&saved.id, 11_000));
        assert!(dup_copy.name.contains("Copy"));

        // 4. Premium `.safitemplate` Export / Import enforcement:
        let tpl_file = dir_a.path().join("crm_cleaner.safitemplate");
        // Free / Trial user blocked with SF-RCP-005!
        assert_eq!(
            reopened_a.export_safitemplate(&saved.id, &tpl_file, false),
            Err("SF-RCP-005".to_owned())
        );
        // Paid / VIP user exports clean standalone `.safitemplate` (no dataset rows).
        let exported =
            crate::test_result(reopened_a.export_safitemplate(&saved.id, &tpl_file, true));
        assert_eq!(exported.steps.len(), 3);

        // Account B imports into its separate local library:
        let mut lib_b = crate::test_result(LocalRecipeLibrary::open(
            dir_b.path().to_path_buf(),
            Some(vault_b),
        ));
        // Free Account B blocked with SF-RCP-005!
        assert_eq!(
            lib_b.import_safitemplate(
                &tpl_file,
                false,
                NameConflictPolicy::FailOnDuplicate,
                12_000
            ),
            Err("SF-RCP-005".to_owned())
        );
        // Paid Account B imports & matches onto a different dataset!
        let imported_opt = crate::test_result(lib_b.import_safitemplate(
            &tpl_file,
            true,
            NameConflictPolicy::FailOnDuplicate,
            12_100,
        ));
        let imported = match imported_opt {
            Some(r) => r,
            None => panic!("expected imported recipe"),
        };
        assert_eq!(imported.origin, RecipeOriginClass::ImportedRecipe);
        let report = crate::test_result(lib_b.match_recipe_to_dataset(
            &imported.id,
            &[
                "full_name".to_owned(),
                "city".to_owned(),
                "revenue".to_owned(),
            ],
        ));
        assert_eq!(report.matches.len(), 2);

        // Tampered `.safitemplate` file rejected with SF-CAL-008 without creating partial state!
        let bad_file = dir_a.path().join("tampered.safitemplate");
        let raw_good = crate::test_result(fs::read_to_string(&tpl_file));
        let tampered = raw_good.replace("My Cairo CRM Cleaner", "Tampered Name");
        crate::test_result(fs::write(&bad_file, tampered));
        assert_eq!(
            lib_b.import_safitemplate(&bad_file, true, NameConflictPolicy::Rename, 12_200),
            Err("SF-CAL-008".to_owned())
        );
        assert_eq!(lib_b.list_my_recipes().len(), 1);

        // 5. Share with SafiData requires consent & blocks secrets!
        assert_eq!(
            reopened_a.prepare_community_contribution(&saved.id, false, "v1.0", false),
            Err("SF-RCP-006".to_owned())
        );
        let contrib = crate::test_result(
            reopened_a.prepare_community_contribution(&saved.id, true, "v1.0", true),
        );
        assert_eq!(contrib.steps.len(), 3);
    }
}
