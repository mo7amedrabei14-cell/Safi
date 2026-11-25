//! Real PostgreSQL 18 Source-of-Truth Repository Methods for `PgControlPlaneStore`.
//!
//! Eliminates in-memory authority whenever `PgControlPlaneStore` is attached:
//! - Official Templates & Versions (`official_templates`, `official_template_versions`)
//! - Community Template Submissions (`community_template_submissions`, RLS + owner override)
//! - Signed Application Releases (`app_releases`)
//! - Owner Web Sessions (`owner_web_sessions`, RLS)
//! - Desktop Browser PKCE Codes (`desktop_auth_codes`, atomic single-use `UPDATE ... RETURNING`)
//! - Owner MFA (`platform_owner`, encrypted `owner_totp_credentials`, `owner_recovery_codes`,
//!   `owner_elevated_sessions`, `owner_web_sessions.elevated_*`)
//! - Single-Device Authority (`users.session_generation`, `devices`, `sessions`,
//!   `device_replacement_events`, advisory lock serialized)
//! - Audit Log (`owner_audit_log`)

use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::auth::{AuthMethod, AuthenticatedSession, Principal, Scope};
use crate::device_authority::{
    verify_device_ed25519_proof, DeviceReplacementEvent, DeviceStatus, RegisteredDeviceRecord,
};
use crate::error::AuthError;
use crate::log_center::{AuditLogEntry, AuditOutcome};
use crate::owner_mfa::{
    ElevatedOwnerSession, OwnerMfaSnapshot, PlatformOwnerRecord, StoredRecoveryCode,
    StoredTotpCredential,
};
use crate::pg_store::PgControlPlaneStore;
use crate::templates_hub::{
    scan_recipe_definition, validate_declarative_steps, AppReleaseRecord,
    CommunitySubmissionRecord, DeclarativeRecipeStep, OfficialTemplateRecord,
    OfficialTemplateVersion, PublishSubmissionRequest, SubmissionStatus,
};

fn parse_submission_status(raw: &str) -> SubmissionStatus {
    match raw {
        "under_review" => SubmissionStatus::UnderReview,
        "needs_changes" | "changes_requested" => SubmissionStatus::NeedsChanges,
        "approved" => SubmissionStatus::Approved,
        "rejected" => SubmissionStatus::Rejected,
        "published" => SubmissionStatus::Published,
        _ => SubmissionStatus::Pending,
    }
}

fn submission_status_to_sql(status: SubmissionStatus) -> &'static str {
    match status {
        SubmissionStatus::Pending => "pending",
        SubmissionStatus::UnderReview => "under_review",
        SubmissionStatus::NeedsChanges => "needs_changes",
        SubmissionStatus::Approved => "approved",
        SubmissionStatus::Rejected => "rejected",
        SubmissionStatus::Published => "published",
    }
}

fn parse_device_status(raw: &str) -> DeviceStatus {
    match raw {
        "replaced_by_new_device" => DeviceStatus::ReplacedByNewDevice,
        "revoked_by_owner" => DeviceStatus::RevokedByOwner,
        "signed_out" => DeviceStatus::SignedOut,
        _ => DeviceStatus::Active,
    }
}

impl PgControlPlaneStore {
    /// Seed default Official Templates into PostgreSQL ONLY when `official_templates` is empty.
    pub async fn seed_default_official_templates_if_empty(
        &self,
        defaults: &[OfficialTemplateRecord],
    ) -> Result<(), AuthError> {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM official_templates")
            .fetch_one(self.pool())
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        if count > 0 {
            return Ok(());
        }
        for tpl in defaults {
            self.upsert_official_template(tpl).await?;
        }
        Ok(())
    }

    /// Direct PostgreSQL read of Official Templates with their version histories.
    pub async fn list_official_templates(
        &self,
        include_inactive: bool,
        include_paid: bool,
    ) -> Result<Vec<OfficialTemplateRecord>, AuthError> {
        let rows = sqlx::query(
            r"
            SELECT
                id, slug, name, description, category, recipe_type, active,
                current_version, requires_paid_plan, source_submission_id, published_by,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs,
                EXTRACT(EPOCH FROM updated_at)::BIGINT AS updated_at_secs
            FROM official_templates
            WHERE ($1 OR active = TRUE)
              AND ($2 OR requires_paid_plan = FALSE)
            ORDER BY slug ASC
            ",
        )
        .bind(include_inactive)
        .bind(include_paid)
        .fetch_all(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let version_rows = sqlx::query(
            r"
            SELECT
                id, template_id, version_number, schema_version, compatible_app_version,
                release_notes, required_columns, steps_json, content_hash, published_by,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs
            FROM official_template_versions
            ORDER BY template_id ASC, version_number ASC
            ",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let mut versions_by_tpl: std::collections::HashMap<Uuid, Vec<OfficialTemplateVersion>> =
            std::collections::HashMap::new();
        for vr in version_rows {
            let tid: Uuid = vr
                .try_get("template_id")
                .map_err(|_| AuthError::BackendUnavailable)?;
            let cols_val: Value = vr
                .try_get("required_columns")
                .map_err(|_| AuthError::BackendUnavailable)?;
            let steps_val: Value = vr
                .try_get("steps_json")
                .map_err(|_| AuthError::BackendUnavailable)?;
            let required_columns: Vec<String> =
                serde_json::from_value(cols_val).unwrap_or_default();
            let steps: Vec<DeclarativeRecipeStep> =
                serde_json::from_value(steps_val).unwrap_or_default();
            versions_by_tpl
                .entry(tid)
                .or_default()
                .push(OfficialTemplateVersion {
                    id: vr
                        .try_get("id")
                        .map_err(|_| AuthError::BackendUnavailable)?,
                    template_id: tid,
                    version_number: u32::try_from(
                        vr.try_get::<i32, _>("version_number").unwrap_or(1),
                    )
                    .unwrap_or(1),
                    schema_version: u32::try_from(
                        vr.try_get::<i32, _>("schema_version").unwrap_or(1),
                    )
                    .unwrap_or(1),
                    compatible_app_version: vr
                        .try_get("compatible_app_version")
                        .unwrap_or_else(|_| "1.0.0".to_owned()),
                    release_notes: vr.try_get("release_notes").unwrap_or_default(),
                    required_columns,
                    steps,
                    content_hash: vr.try_get("content_hash").unwrap_or_default(),
                    published_by: vr.try_get("published_by").ok().flatten(),
                    created_at: vr.try_get("created_at_secs").unwrap_or(0),
                });
        }

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: Uuid = row
                .try_get("id")
                .map_err(|_| AuthError::BackendUnavailable)?;
            out.push(OfficialTemplateRecord {
                id,
                slug: row
                    .try_get("slug")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                name: row
                    .try_get("name")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                description: row
                    .try_get("description")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                category: row
                    .try_get("category")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                recipe_type: row
                    .try_get("recipe_type")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                active: row
                    .try_get("active")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                current_version: u32::try_from(
                    row.try_get::<i32, _>("current_version").unwrap_or(1),
                )
                .unwrap_or(1),
                requires_paid_plan: row
                    .try_get("requires_paid_plan")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                source_submission_id: row.try_get("source_submission_id").ok().flatten(),
                published_by: row.try_get("published_by").ok().flatten(),
                created_at: row.try_get("created_at_secs").unwrap_or(0),
                updated_at: row.try_get("updated_at_secs").unwrap_or(0),
                versions: versions_by_tpl.remove(&id).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    /// Fetch a single Official Template (with all versions) directly from PostgreSQL.
    pub async fn get_official_template_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<OfficialTemplateRecord>, AuthError> {
        let all = self.list_official_templates(true, true).await?;
        Ok(all.into_iter().find(|t| t.id == id))
    }

    /// Atomically update `official_templates.active` in PostgreSQL.
    pub async fn set_official_template_active_pg(
        &self,
        id: Uuid,
        active: bool,
    ) -> Result<OfficialTemplateRecord, AuthError> {
        let affected = sqlx::query(
            "UPDATE official_templates SET active = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(active)
        .execute(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .rows_affected();
        if affected == 0 {
            return Err(AuthError::InvalidRequest);
        }
        self.get_official_template_by_id(id)
            .await?
            .ok_or(AuthError::InvalidRequest)
    }

    /// Atomically roll back `official_templates.current_version` inside a PostgreSQL transaction.
    pub async fn rollback_official_template_pg(
        &self,
        id: Uuid,
        version_number: u32,
    ) -> Result<OfficialTemplateRecord, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM official_template_versions WHERE template_id = $1 AND version_number = $2)",
        )
        .bind(id)
        .bind(i32::try_from(version_number).unwrap_or(1))
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        if !exists {
            return Err(AuthError::InvalidRequest);
        }
        sqlx::query(
            "UPDATE official_templates SET current_version = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(i32::try_from(version_number).unwrap_or(1))
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        self.get_official_template_by_id(id)
            .await?
            .ok_or(AuthError::InvalidRequest)
    }

    /// Atomically validate monthly quota and insert a community submission inside PostgreSQL.
    pub async fn create_community_submission_atomic(
        &self,
        sub: &CommunitySubmissionRecord,
        monthly_limit: usize,
    ) -> Result<CommunitySubmissionRecord, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.current_workspace_id', $1, true)")
            .bind(sub.workspace_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(format!("community_sub:{}", sub.contributor_user_id))
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let used_this_month: i64 = sqlx::query_scalar(
            r"
            SELECT count(*)
            FROM community_template_submissions
            WHERE contributor_user_id = $1
              AND date_trunc('month', created_at) = date_trunc('month', now())
            ",
        )
        .bind(sub.contributor_user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        if usize::try_from(used_this_month).unwrap_or(usize::MAX) >= monthly_limit {
            return Err(AuthError::CommunitySubmissionLimitReached);
        }

        let cols_json =
            serde_json::to_value(&sub.required_columns).unwrap_or_else(|_| Value::Array(vec![]));
        let steps_json = serde_json::to_value(&sub.steps).unwrap_or_else(|_| Value::Array(vec![]));
        let warnings_json =
            serde_json::to_value(&sub.pii_warnings).unwrap_or_else(|_| Value::Array(vec![]));

        sqlx::query(
            r"
            INSERT INTO community_template_submissions (
                id, contributor_user_id, workspace_id, name, description, recipe_type,
                schema_version, app_version, required_columns, steps_json, content_hash,
                status, reviewer_notes, reviewed_by, consent_accepted, consent_terms_version,
                show_contributor_attribution, published_template_id, pii_warnings
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11,
                $12, $13, $14, $15, $16,
                $17, $18, $19
            )
            ",
        )
        .bind(sub.id)
        .bind(sub.contributor_user_id)
        .bind(sub.workspace_id)
        .bind(&sub.name)
        .bind(&sub.description)
        .bind(&sub.recipe_type)
        .bind(i32::try_from(sub.schema_version).unwrap_or(1))
        .bind(&sub.app_version)
        .bind(cols_json)
        .bind(steps_json)
        .bind(&sub.content_hash)
        .bind(submission_status_to_sql(sub.status))
        .bind(&sub.reviewer_notes)
        .bind(sub.reviewed_by)
        .bind(sub.consent_accepted)
        .bind(&sub.consent_terms_version)
        .bind(sub.show_contributor_attribution)
        .bind(sub.published_template_id)
        .bind(warnings_json)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(sub.clone())
    }

    fn decode_submission_row(
        row: &sqlx::postgres::PgRow,
    ) -> Result<CommunitySubmissionRecord, AuthError> {
        let cols_val: Value = row
            .try_get("required_columns")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let steps_val: Value = row
            .try_get("steps_json")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let warnings_val: Value = row
            .try_get("pii_warnings")
            .unwrap_or_else(|_| Value::Array(vec![]));
        let status_raw: String = row
            .try_get("status")
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(CommunitySubmissionRecord {
            id: row
                .try_get("id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            contributor_user_id: row
                .try_get("contributor_user_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            workspace_id: row
                .try_get("workspace_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            name: row
                .try_get("name")
                .map_err(|_| AuthError::BackendUnavailable)?,
            description: row
                .try_get("description")
                .map_err(|_| AuthError::BackendUnavailable)?,
            recipe_type: row
                .try_get("recipe_type")
                .map_err(|_| AuthError::BackendUnavailable)?,
            schema_version: u32::try_from(row.try_get::<i32, _>("schema_version").unwrap_or(1))
                .unwrap_or(1),
            app_version: row
                .try_get("app_version")
                .unwrap_or_else(|_| "1.0.0".to_owned()),
            required_columns: serde_json::from_value(cols_val).unwrap_or_default(),
            steps: serde_json::from_value(steps_val).unwrap_or_default(),
            content_hash: row.try_get("content_hash").unwrap_or_default(),
            status: parse_submission_status(&status_raw),
            reviewer_notes: row.try_get("reviewer_notes").unwrap_or_default(),
            reviewed_by: row.try_get("reviewed_by").ok().flatten(),
            consent_accepted: row.try_get("consent_accepted").unwrap_or(true),
            consent_terms_version: row.try_get("consent_terms_version").unwrap_or_default(),
            consent_at: row.try_get("consent_at_secs").unwrap_or(0),
            show_contributor_attribution: row
                .try_get("show_contributor_attribution")
                .unwrap_or(false),
            pii_warnings: serde_json::from_value(warnings_val).unwrap_or_default(),
            published_template_id: row.try_get("published_template_id").ok().flatten(),
            created_at: row.try_get("created_at_secs").unwrap_or(0),
            updated_at: row.try_get("updated_at_secs").unwrap_or(0),
        })
    }

    /// Direct PostgreSQL read of a regular user's own community submissions under workspace RLS.
    pub async fn list_my_community_submissions_pg(
        &self,
        contributor_user_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Vec<CommunitySubmissionRecord>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.current_workspace_id', $1, true)")
            .bind(workspace_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let rows = sqlx::query(
            r"
            SELECT
                id, contributor_user_id, workspace_id, name, description, recipe_type,
                schema_version, app_version, required_columns, steps_json, content_hash,
                status, reviewer_notes, reviewed_by, consent_accepted, consent_terms_version,
                EXTRACT(EPOCH FROM consent_at)::BIGINT AS consent_at_secs,
                show_contributor_attribution, pii_warnings, published_template_id,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs,
                EXTRACT(EPOCH FROM updated_at)::BIGINT AS updated_at_secs
            FROM community_template_submissions
            WHERE contributor_user_id = $1
            ORDER BY created_at DESC
            ",
        )
        .bind(contributor_user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        rows.iter().map(Self::decode_submission_row).collect()
    }

    /// Direct PostgreSQL read of ALL community submissions for Owner administration.
    pub async fn list_all_community_submissions_for_owner_pg(
        &self,
    ) -> Result<Vec<CommunitySubmissionRecord>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let rows = sqlx::query(
            r"
            SELECT
                id, contributor_user_id, workspace_id, name, description, recipe_type,
                schema_version, app_version, required_columns, steps_json, content_hash,
                status, reviewer_notes, reviewed_by, consent_accepted, consent_terms_version,
                EXTRACT(EPOCH FROM consent_at)::BIGINT AS consent_at_secs,
                show_contributor_attribution, pii_warnings, published_template_id,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs,
                EXTRACT(EPOCH FROM updated_at)::BIGINT AS updated_at_secs
            FROM community_template_submissions
            ORDER BY created_at DESC
            ",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        rows.iter().map(Self::decode_submission_row).collect()
    }

    /// Atomically update a community submission review status inside PostgreSQL.
    /// Enforces that `Published` cannot be set via Review (requires separate explicit Publish).
    pub async fn review_community_submission_pg(
        &self,
        submission_id: Uuid,
        reviewer_user_id: Uuid,
        status: SubmissionStatus,
        reviewer_notes: &str,
    ) -> Result<CommunitySubmissionRecord, AuthError> {
        if matches!(status, SubmissionStatus::Published) {
            return Err(AuthError::InvalidRequest);
        }
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let row = sqlx::query(
            r"
            UPDATE community_template_submissions
            SET status = $2,
                reviewer_notes = $3,
                reviewed_by = $4,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id, contributor_user_id, workspace_id, name, description, recipe_type,
                schema_version, app_version, required_columns, steps_json, content_hash,
                status, reviewer_notes, reviewed_by, consent_accepted, consent_terms_version,
                EXTRACT(EPOCH FROM consent_at)::BIGINT AS consent_at_secs,
                show_contributor_attribution, pii_warnings, published_template_id,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs,
                EXTRACT(EPOCH FROM updated_at)::BIGINT AS updated_at_secs
            ",
        )
        .bind(submission_id)
        .bind(submission_status_to_sql(status))
        .bind(reviewer_notes.trim())
        .bind(reviewer_user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .ok_or(AuthError::InvalidRequest)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Self::decode_submission_row(&row)
    }

    /// Atomically publish an `approved` community submission into `official_templates` +
    /// `official_template_versions` and transition the submission to `published` inside ONE
    /// PostgreSQL transaction (all-or-nothing rollback on any failure).
    pub async fn publish_community_submission_atomic_pg(
        &self,
        submission_id: Uuid,
        owner_user_id: Uuid,
        req: &PublishSubmissionRequest,
        now_secs: i64,
    ) -> Result<(OfficialTemplateRecord, CommunitySubmissionRecord), AuthError> {
        let slug = req.slug.trim().to_ascii_lowercase();
        if slug.is_empty()
            || slug.len() > 80
            || !slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(AuthError::InvalidRequest);
        }
        let official_name = req.official_name.trim();
        let official_desc = req.official_description.trim();
        if official_name.is_empty() || official_desc.is_empty() {
            return Err(AuthError::InvalidRequest);
        }

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let sub_row = sqlx::query(
            r"
            SELECT
                id, contributor_user_id, workspace_id, name, description, recipe_type,
                schema_version, app_version, required_columns, steps_json, content_hash,
                status, reviewer_notes, reviewed_by, consent_accepted, consent_terms_version,
                EXTRACT(EPOCH FROM consent_at)::BIGINT AS consent_at_secs,
                show_contributor_attribution, pii_warnings, published_template_id,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs,
                EXTRACT(EPOCH FROM updated_at)::BIGINT AS updated_at_secs
            FROM community_template_submissions
            WHERE id = $1
            FOR UPDATE
            ",
        )
        .bind(submission_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .ok_or(AuthError::InvalidRequest)?;

        let mut sub = Self::decode_submission_row(&sub_row)?;
        if sub.status != SubmissionStatus::Approved {
            return Err(AuthError::InvalidRequest);
        }

        let final_steps = req
            .edited_steps
            .clone()
            .unwrap_or_else(|| sub.steps.clone());
        validate_declarative_steps(&final_steps)?;
        let scan = scan_recipe_definition(official_name, official_desc, &final_steps);
        if !scan.blocked_secrets.is_empty() {
            return Err(AuthError::CommunitySubmissionRejected);
        }
        let content_hash = blake3::hash(
            serde_json::to_string(&final_steps)
                .unwrap_or_default()
                .as_bytes(),
        )
        .to_hex()
        .to_string();

        let existing_tpl = sqlx::query(
            "SELECT id, current_version FROM official_templates WHERE slug = $1 FOR UPDATE",
        )
        .bind(&slug)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let (template_id, next_version) = if let Some(row) = existing_tpl {
            let tid: Uuid = row
                .try_get("id")
                .map_err(|_| AuthError::BackendUnavailable)?;
            let cur: i32 = row.try_get("current_version").unwrap_or(1);
            let next = cur.saturating_add(1);
            sqlx::query(
                r"
                UPDATE official_templates
                SET name = $2, description = $3, category = $4, active = TRUE,
                    current_version = $5, requires_paid_plan = $6,
                    source_submission_id = $7, published_by = $8, updated_at = now()
                WHERE id = $1
                ",
            )
            .bind(tid)
            .bind(official_name)
            .bind(official_desc)
            .bind(req.category.trim())
            .bind(next)
            .bind(req.requires_paid_plan)
            .bind(submission_id)
            .bind(owner_user_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
            (tid, next)
        } else {
            let tid = Uuid::now_v7();
            sqlx::query(
                r"
                INSERT INTO official_templates (
                    id, slug, name, description, category, recipe_type,
                    active, current_version, requires_paid_plan, source_submission_id, published_by
                )
                VALUES ($1, $2, $3, $4, $5, $6, TRUE, 1, $7, $8, $9)
                ",
            )
            .bind(tid)
            .bind(&slug)
            .bind(official_name)
            .bind(official_desc)
            .bind(req.category.trim())
            .bind(&sub.recipe_type)
            .bind(req.requires_paid_plan)
            .bind(submission_id)
            .bind(owner_user_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
            (tid, 1)
        };

        let cols_json =
            serde_json::to_value(&sub.required_columns).unwrap_or_else(|_| Value::Array(vec![]));
        let steps_json =
            serde_json::to_value(&final_steps).unwrap_or_else(|_| Value::Array(vec![]));
        let version_id = Uuid::now_v7();
        sqlx::query(
            r"
            INSERT INTO official_template_versions (
                id, template_id, version_number, schema_version, compatible_app_version,
                release_notes, required_columns, steps_json, content_hash, published_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ",
        )
        .bind(version_id)
        .bind(template_id)
        .bind(next_version)
        .bind(i32::try_from(sub.schema_version).unwrap_or(1))
        .bind(&sub.app_version)
        .bind(req.release_notes.trim())
        .bind(cols_json)
        .bind(steps_json)
        .bind(&content_hash)
        .bind(owner_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            r"
            UPDATE community_template_submissions
            SET status = 'published',
                published_template_id = $2,
                reviewed_by = $3,
                updated_at = now()
            WHERE id = $1
            ",
        )
        .bind(submission_id)
        .bind(template_id)
        .bind(owner_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        sub.status = SubmissionStatus::Published;
        sub.published_template_id = Some(template_id);
        sub.reviewed_by = Some(owner_user_id);
        sub.updated_at = now_secs;

        let tpl = self
            .get_official_template_by_id(template_id)
            .await?
            .ok_or(AuthError::BackendUnavailable)?;
        Ok((tpl, sub))
    }

    /// Direct PostgreSQL read of all Application Releases (`app_releases`).
    pub async fn list_app_releases_pg(&self) -> Result<Vec<AppReleaseRecord>, AuthError> {
        let rows = sqlx::query(
            r"
            SELECT
                id, version_code, version_name, channel, platform, download_url,
                sha256_hex, file_size_bytes, min_required_version, release_notes,
                is_mandatory, active, published_by,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs
            FROM app_releases
            ORDER BY version_code DESC, created_at DESC
            ",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(AppReleaseRecord {
                id: row
                    .try_get("id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                version_code: u64::try_from(row.try_get::<i64, _>("version_code").unwrap_or(100))
                    .unwrap_or(100),
                version_name: row
                    .try_get("version_name")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                channel: row
                    .try_get("channel")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                platform: row
                    .try_get("platform")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                download_url: row
                    .try_get("download_url")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                sha256_hex: row
                    .try_get("sha256_hex")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                file_size_bytes: u64::try_from(
                    row.try_get::<i64, _>("file_size_bytes").unwrap_or(1),
                )
                .unwrap_or(1),
                min_required_version: u64::try_from(
                    row.try_get::<i64, _>("min_required_version").unwrap_or(100),
                )
                .unwrap_or(100),
                release_notes: row.try_get("release_notes").unwrap_or_default(),
                is_mandatory: row.try_get("is_mandatory").unwrap_or(false),
                active: row.try_get("active").unwrap_or(true),
                published_by: row.try_get("published_by").ok().flatten(),
                created_at: row.try_get("created_at_secs").unwrap_or(0),
            });
        }
        Ok(out)
    }

    /// Direct PostgreSQL query for the latest active release for `(channel, platform)`.
    pub async fn latest_active_release_pg(
        &self,
        channel: &str,
        platform: &str,
    ) -> Result<Option<AppReleaseRecord>, AuthError> {
        let row = sqlx::query(
            r"
            SELECT
                id, version_code, version_name, channel, platform, download_url,
                sha256_hex, file_size_bytes, min_required_version, release_notes,
                is_mandatory, active, published_by,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs
            FROM app_releases
            WHERE active = TRUE AND channel = $1 AND platform = $2
            ORDER BY version_code DESC, created_at DESC
            LIMIT 1
            ",
        )
        .bind(channel)
        .bind(platform)
        .fetch_optional(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(AppReleaseRecord {
            id: row
                .try_get("id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            version_code: u64::try_from(row.try_get::<i64, _>("version_code").unwrap_or(100))
                .unwrap_or(100),
            version_name: row
                .try_get("version_name")
                .map_err(|_| AuthError::BackendUnavailable)?,
            channel: row
                .try_get("channel")
                .map_err(|_| AuthError::BackendUnavailable)?,
            platform: row
                .try_get("platform")
                .map_err(|_| AuthError::BackendUnavailable)?,
            download_url: row
                .try_get("download_url")
                .map_err(|_| AuthError::BackendUnavailable)?,
            sha256_hex: row
                .try_get("sha256_hex")
                .map_err(|_| AuthError::BackendUnavailable)?,
            file_size_bytes: u64::try_from(row.try_get::<i64, _>("file_size_bytes").unwrap_or(1))
                .unwrap_or(1),
            min_required_version: u64::try_from(
                row.try_get::<i64, _>("min_required_version").unwrap_or(100),
            )
            .unwrap_or(100),
            release_notes: row.try_get("release_notes").unwrap_or_default(),
            is_mandatory: row.try_get("is_mandatory").unwrap_or(false),
            active: row.try_get("active").unwrap_or(true),
            published_by: row.try_get("published_by").ok().flatten(),
            created_at: row.try_get("created_at_secs").unwrap_or(0),
        }))
    }

    // =========================================================================
    // Owner Web Sessions (`owner_web_sessions`) — Multi-Instance Safe
    // =========================================================================

    /// Authenticate a dedicated `sfw_*` Owner Web session directly against PostgreSQL.
    pub async fn authenticate_owner_web_session_pg(
        &self,
        token_hash_bytes: &[u8],
        now_secs: i64,
    ) -> Result<AuthenticatedSession, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let row = sqlx::query(
            r"
            SELECT
                id, user_id, workspace_id, user_agent_hint,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs,
                EXTRACT(EPOCH FROM expires_at)::BIGINT AS expires_at_secs,
                revoked_at IS NOT NULL AS is_revoked
            FROM owner_web_sessions
            WHERE token_hash = $1
            ",
        )
        .bind(token_hash_bytes)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .ok_or(AuthError::InvalidCredentials)?;

        let is_revoked: bool = row.try_get("is_revoked").unwrap_or(true);
        if is_revoked {
            return Err(AuthError::Revoked);
        }
        let expires_at: i64 = row.try_get("expires_at_secs").unwrap_or(0);
        if now_secs >= expires_at {
            return Err(AuthError::Expired);
        }
        let id: Uuid = row
            .try_get("id")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let user_id: Uuid = row
            .try_get("user_id")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let workspace_id: Uuid = row
            .try_get("workspace_id")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let user_agent_hint: String = row
            .try_get("user_agent_hint")
            .unwrap_or_else(|_| "owner-web".to_owned());
        let created_at: i64 = row.try_get("created_at_secs").unwrap_or(now_secs);

        sqlx::query("UPDATE owner_web_sessions SET last_seen_at = to_timestamp($2) WHERE id = $1")
            .bind(id)
            .bind(now_secs)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let _ = created_at;
        Ok(AuthenticatedSession {
            session_id: id,
            token_hash: hex::encode(token_hash_bytes),
            principal: Principal {
                user_id,
                workspace_id,
                scopes: vec![Scope::Read, Scope::Write, Scope::Admin],
                auth_method: AuthMethod::Password,
            },
            device_name: user_agent_hint,
            device_installation_key_hash: None,
            expires_at,
        })
    }

    /// Revoke an Owner Web session directly in PostgreSQL (`owner_web_sessions`).
    pub async fn revoke_owner_web_session_pg(
        &self,
        token_hash_bytes: &[u8],
    ) -> Result<bool, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let affected = sqlx::query(
            "UPDATE owner_web_sessions SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token_hash_bytes)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .rows_affected();
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(affected > 0)
    }

    // =========================================================================
    // Desktop Browser PKCE (`desktop_auth_codes`) — Multi-Instance Safe
    // =========================================================================

    #[allow(clippy::too_many_arguments)]
    pub async fn start_desktop_pkce_pg(
        &self,
        request_id: Uuid,
        flow: &str,
        code_challenge: &str,
        state_hash: &[u8],
        installation_id: Uuid,
        platform: &str,
        device_name: &str,
        expires_at: i64,
    ) -> Result<(), AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query(
            r"
            INSERT INTO desktop_auth_codes (
                request_id, flow, code_challenge, code_challenge_method,
                state_hash, installation_id, platform, device_name, expires_at
            )
            VALUES ($1, $2, $3, 'S256', $4, $5, $6, $7, to_timestamp($8))
            ",
        )
        .bind(request_id)
        .bind(flow)
        .bind(code_challenge)
        .bind(state_hash)
        .bind(installation_id)
        .bind(platform)
        .bind(device_name)
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn approve_desktop_pkce_pg(
        &self,
        request_id: Uuid,
        state_hash: &[u8],
        user_id: Uuid,
        workspace_id: Uuid,
        email: &str,
        code_hash: &[u8],
        now_secs: i64,
    ) -> Result<bool, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let affected = sqlx::query(
            r"
            UPDATE desktop_auth_codes
            SET user_id = $3,
                workspace_id = $4,
                email = $5,
                code_hash = $6
            WHERE request_id = $1
              AND state_hash = $2
              AND consumed_at IS NULL
              AND code_hash IS NULL
              AND expires_at > to_timestamp($7)
            ",
        )
        .bind(request_id)
        .bind(state_hash)
        .bind(user_id)
        .bind(workspace_id)
        .bind(email)
        .bind(code_hash)
        .bind(now_secs)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .rows_affected();
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(affected == 1)
    }

    /// Atomically consume a single-use Desktop PKCE authorization code in PostgreSQL.
    /// Returns `(code_challenge, user_id, workspace_id, email, platform, device_name)` once and ONLY once
    /// across any number of concurrent server instances.
    #[allow(clippy::type_complexity)]
    pub async fn consume_desktop_pkce_atomic_pg(
        &self,
        code_hash: &[u8],
        state_hash: &[u8],
        installation_id: Uuid,
        now_secs: i64,
    ) -> Result<Option<(String, Uuid, Uuid, String, String, String)>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let row = sqlx::query(
            r"
            UPDATE desktop_auth_codes
            SET consumed_at = now()
            WHERE code_hash = $1
              AND state_hash = $2
              AND installation_id = $3
              AND consumed_at IS NULL
              AND user_id IS NOT NULL
              AND workspace_id IS NOT NULL
              AND expires_at > to_timestamp($4)
            RETURNING code_challenge, user_id, workspace_id, COALESCE(email, '') AS email, platform, device_name
            ",
        )
        .bind(code_hash)
        .bind(state_hash)
        .bind(installation_id)
        .bind(now_secs)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some((
            row.try_get("code_challenge")
                .map_err(|_| AuthError::BackendUnavailable)?,
            row.try_get("user_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            row.try_get("workspace_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            row.try_get("email").unwrap_or_default(),
            row.try_get("platform")
                .unwrap_or_else(|_| "windows".to_owned()),
            row.try_get("device_name")
                .unwrap_or_else(|_| "Desktop".to_owned()),
        )))
    }

    // =========================================================================
    // Owner MFA Durability (`platform_owner`, `owner_totp_credentials`,
    // `owner_recovery_codes`, `owner_elevated_sessions`, `owner_web_sessions`)
    // =========================================================================

    /// Load complete Owner MFA state directly from PostgreSQL (encrypted TOTP, recovery codes,
    /// and active elevated sessions for both Desktop and Owner Web sessions).
    pub async fn load_owner_mfa_snapshot_pg(&self) -> Result<OwnerMfaSnapshot, AuthError> {
        let owner_row = sqlx::query(
            r"
            SELECT
                owner_user_id, bootstrap_sealed,
                EXTRACT(EPOCH FROM bootstrapped_at)::BIGINT AS bootstrapped_at_secs
            FROM platform_owner
            LIMIT 1
            ",
        )
        .fetch_optional(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let owner = owner_row.map(|row| PlatformOwnerRecord {
            owner_user_id: row.try_get("owner_user_id").unwrap_or_else(|_| Uuid::nil()),
            bootstrap_sealed: row.try_get("bootstrap_sealed").unwrap_or(true),
            bootstrapped_at: row.try_get("bootstrapped_at_secs").unwrap_or(0),
        });

        let totp_rows = sqlx::query(
            r"
            SELECT
                user_id, secret_ciphertext, secret_nonce, key_version, confirmed,
                last_accepted_step, failed_attempts,
                EXTRACT(EPOCH FROM locked_until)::BIGINT AS locked_until_secs,
                EXTRACT(EPOCH FROM enrolled_at)::BIGINT AS enrolled_at_secs,
                EXTRACT(EPOCH FROM confirmed_at)::BIGINT AS confirmed_at_secs
            FROM owner_totp_credentials
            ",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let mut totp = Vec::with_capacity(totp_rows.len());
        for row in totp_rows {
            totp.push(StoredTotpCredential {
                user_id: row
                    .try_get("user_id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                secret_ciphertext: row
                    .try_get("secret_ciphertext")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                secret_nonce: row
                    .try_get("secret_nonce")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                key_version: row.try_get("key_version").unwrap_or(1),
                confirmed: row.try_get("confirmed").unwrap_or(false),
                last_accepted_step: row.try_get("last_accepted_step").ok().flatten(),
                failed_attempts: row.try_get("failed_attempts").unwrap_or(0),
                locked_until: row.try_get("locked_until_secs").ok().flatten(),
                enrolled_at: row.try_get("enrolled_at_secs").unwrap_or(0),
                confirmed_at: row.try_get("confirmed_at_secs").ok().flatten(),
            });
        }

        let rec_rows = sqlx::query(
            r"
            SELECT
                id, user_id, code_hash,
                EXTRACT(EPOCH FROM used_at)::BIGINT AS used_at_secs
            FROM owner_recovery_codes
            ",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let mut recovery_codes = Vec::with_capacity(rec_rows.len());
        for row in rec_rows {
            recovery_codes.push(StoredRecoveryCode {
                id: row
                    .try_get("id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                user_id: row
                    .try_get("user_id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                code_hash: row
                    .try_get("code_hash")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                used_at: row.try_get("used_at_secs").ok().flatten(),
            });
        }

        let mut elevated_sessions = Vec::new();
        let desk_elev_rows = sqlx::query(
            r"
            SELECT
                session_id, user_id,
                EXTRACT(EPOCH FROM verified_at)::BIGINT AS verified_at_secs,
                EXTRACT(EPOCH FROM expires_at)::BIGINT AS expires_at_secs,
                EXTRACT(EPOCH FROM last_step_up_at)::BIGINT AS last_step_up_at_secs
            FROM owner_elevated_sessions
            WHERE expires_at > now()
            ",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        for row in desk_elev_rows {
            elevated_sessions.push(ElevatedOwnerSession {
                session_id: row
                    .try_get("session_id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                user_id: row
                    .try_get("user_id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                verified_at: row.try_get("verified_at_secs").unwrap_or(0),
                expires_at: row.try_get("expires_at_secs").unwrap_or(0),
                last_step_up_at: row.try_get("last_step_up_at_secs").unwrap_or(0),
            });
        }

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let web_elev_rows = sqlx::query(
            r"
            SELECT
                id AS session_id, user_id,
                EXTRACT(EPOCH FROM elevated_verified_at)::BIGINT AS verified_at_secs,
                EXTRACT(EPOCH FROM elevated_expires_at)::BIGINT AS expires_at_secs,
                EXTRACT(EPOCH FROM last_step_up_at)::BIGINT AS last_step_up_at_secs
            FROM owner_web_sessions
            WHERE revoked_at IS NULL
              AND elevated_expires_at IS NOT NULL
              AND elevated_expires_at > now()
            ",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        for row in web_elev_rows {
            elevated_sessions.push(ElevatedOwnerSession {
                session_id: row
                    .try_get("session_id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                user_id: row
                    .try_get("user_id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                verified_at: row.try_get("verified_at_secs").unwrap_or(0),
                expires_at: row.try_get("expires_at_secs").unwrap_or(0),
                last_step_up_at: row.try_get("last_step_up_at_secs").unwrap_or(0),
            });
        }

        Ok(OwnerMfaSnapshot {
            owner,
            totp,
            recovery_codes,
            elevated_sessions,
        })
    }

    /// Persist full Owner MFA state (encrypted TOTP credentials, recovery codes, and elevated
    /// sessions for both Desktop and Owner Web sessions) atomically into PostgreSQL.
    pub async fn persist_owner_mfa_snapshot_pg(
        &self,
        snap: &OwnerMfaSnapshot,
    ) -> Result<(), AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        if let Some(owner) = &snap.owner {
            sqlx::query(
                r"
                INSERT INTO platform_owner (singleton, owner_user_id, bootstrap_sealed, bootstrapped_at)
                VALUES (TRUE, $1, $2, to_timestamp($3))
                ON CONFLICT (singleton) DO NOTHING
                ",
            )
            .bind(owner.owner_user_id)
            .bind(owner.bootstrap_sealed)
            .bind(owner.bootstrapped_at)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        }

        for cred in &snap.totp {
            sqlx::query(
                r"
                INSERT INTO owner_totp_credentials (
                    user_id, secret_ciphertext, secret_nonce, key_version, confirmed,
                    last_accepted_step, failed_attempts, locked_until, enrolled_at, confirmed_at
                )
                VALUES (
                    $1, $2, $3, $4, $5,
                    $6, $7, to_timestamp($8), to_timestamp($9), to_timestamp($10)
                )
                ON CONFLICT (user_id) DO UPDATE
                SET secret_ciphertext = EXCLUDED.secret_ciphertext,
                    secret_nonce = EXCLUDED.secret_nonce,
                    key_version = EXCLUDED.key_version,
                    confirmed = EXCLUDED.confirmed,
                    last_accepted_step = EXCLUDED.last_accepted_step,
                    failed_attempts = EXCLUDED.failed_attempts,
                    locked_until = EXCLUDED.locked_until,
                    confirmed_at = EXCLUDED.confirmed_at,
                    updated_at = now()
                ",
            )
            .bind(cred.user_id)
            .bind(&cred.secret_ciphertext)
            .bind(&cred.secret_nonce)
            .bind(cred.key_version)
            .bind(cred.confirmed)
            .bind(cred.last_accepted_step)
            .bind(cred.failed_attempts)
            .bind(cred.locked_until)
            .bind(cred.enrolled_at)
            .bind(cred.confirmed_at)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        }

        for rec in &snap.recovery_codes {
            sqlx::query(
                r"
                INSERT INTO owner_recovery_codes (id, user_id, code_hash, used_at)
                VALUES ($1, $2, $3, to_timestamp($4))
                ON CONFLICT (code_hash) DO UPDATE
                SET used_at = COALESCE(owner_recovery_codes.used_at, EXCLUDED.used_at)
                ",
            )
            .bind(rec.id)
            .bind(rec.user_id)
            .bind(&rec.code_hash)
            .bind(rec.used_at)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        }

        for elev in &snap.elevated_sessions {
            // Update owner_web_sessions if this is a Web Owner session; otherwise upsert owner_elevated_sessions.
            let web_updated = sqlx::query(
                r"
                UPDATE owner_web_sessions
                SET elevated_verified_at = to_timestamp($2),
                    elevated_expires_at = to_timestamp($3),
                    last_step_up_at = to_timestamp($4)
                WHERE id = $1
                ",
            )
            .bind(elev.session_id)
            .bind(elev.verified_at)
            .bind(elev.expires_at)
            .bind(elev.last_step_up_at)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?
            .rows_affected();

            if web_updated == 0 {
                let desk_exists: bool =
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = $1)")
                        .bind(elev.session_id)
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(|_| AuthError::BackendUnavailable)?;
                if desk_exists {
                    sqlx::query(
                        r"
                        INSERT INTO owner_elevated_sessions (
                            session_id, user_id, verified_at, expires_at, last_step_up_at
                        )
                        VALUES ($1, $2, to_timestamp($3), to_timestamp($4), to_timestamp($5))
                        ON CONFLICT (session_id) DO UPDATE
                        SET verified_at = EXCLUDED.verified_at,
                            expires_at = EXCLUDED.expires_at,
                            last_step_up_at = EXCLUDED.last_step_up_at
                        ",
                    )
                    .bind(elev.session_id)
                    .bind(elev.user_id)
                    .bind(elev.verified_at)
                    .bind(elev.expires_at)
                    .bind(elev.last_step_up_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AuthError::BackendUnavailable)?;
                }
            }
        }

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    // =========================================================================
    // Single-Device Authority (`users.session_generation`, `devices`, `sessions`,
    // `device_replacement_events`) — Multi-Instance Safe & Advisory Locked
    // =========================================================================

    #[allow(clippy::too_many_arguments)]
    pub async fn activate_device_atomically_pg(
        &self,
        user_id: Uuid,
        workspace_id: Uuid,
        device_name: &str,
        platform: &str,
        installation_id: Uuid,
        public_key_hex: Option<&str>,
        new_session_id: Uuid,
        now_secs: i64,
    ) -> Result<(RegisteredDeviceRecord, Option<Uuid>, DeviceReplacementEvent), AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.workspace_id', $1, true)")
            .bind(workspace_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(format!("device_auth:{user_id}"))
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let next_gen_i64: i64 = sqlx::query_scalar(
            "UPDATE users SET session_generation = session_generation + 1 WHERE id = $1 RETURNING session_generation",
        )
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        let next_gen = u64::try_from(next_gen_i64).unwrap_or(1);

        let canonical_inst_hash = crate::auth::device::DeviceRegistration::from_installation_id(
            user_id,
            installation_id,
            crate::auth::device::DevicePlatform::Windows,
        )?
        .installation_key_hash;

        let prev_device_id: Option<Uuid> = sqlx::query_scalar(
            r"
            SELECT id FROM devices
            WHERE user_id = $1
              AND installation_key_hash <> $2
              AND replaced_by_device_id IS NULL
            ORDER BY last_seen_at DESC
            LIMIT 1
            ",
        )
        .bind(user_id)
        .bind(canonical_inst_hash.as_slice())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        // Revoke all previous active sessions for this user EXCEPT the newly minted session.
        sqlx::query(
            r"
            UPDATE sessions
            SET revoked_at = COALESCE(revoked_at, to_timestamp($2)),
                revocation_reason = COALESCE(revocation_reason, 'replaced_by_new_device')
            WHERE user_id = $1
              AND id <> $3
              AND revoked_at IS NULL
            ",
        )
        .bind(user_id)
        .bind(now_secs)
        .bind(new_session_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        // Revoke any OTHER active device for this user (preserving same-installation login).
        sqlx::query(
            r"
            UPDATE devices
            SET revoked_at = to_timestamp($2),
                status = 'replaced_by_new_device',
                active_session_id = NULL
            WHERE user_id = $1
              AND installation_key_hash <> $3
              AND revoked_at IS NULL
            ",
        )
        .bind(user_id)
        .bind(now_secs)
        .bind(canonical_inst_hash.as_slice())
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let norm_platform = match platform.trim().to_ascii_lowercase().as_str() {
            "windows" | "linux" | "macos" | "web" => platform.trim().to_ascii_lowercase(),
            _ => "windows".to_owned(),
        };
        let clean_name = if device_name.trim().is_empty() {
            "Desktop"
        } else {
            device_name.trim()
        };
        let norm_pub_key = public_key_hex.map(|s| s.trim().to_ascii_lowercase());

        let existing_same_inst_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM devices WHERE user_id = $1 AND installation_key_hash = $2 LIMIT 1",
        )
        .bind(user_id)
        .bind(canonical_inst_hash.as_slice())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let new_device_id = if let Some(existing_id) = existing_same_inst_id {
            sqlx::query(
                r"
                UPDATE devices
                SET device_name = $2,
                    platform = $3,
                    last_seen_at = to_timestamp($4),
                    revoked_at = NULL,
                    public_key_hex = COALESCE($5, public_key_hex),
                    installation_id = $6,
                    active_session_id = $7,
                    session_generation = $8,
                    status = 'active',
                    replaced_by_device_id = NULL
                WHERE id = $1
                ",
            )
            .bind(existing_id)
            .bind(clean_name)
            .bind(&norm_platform)
            .bind(now_secs)
            .bind(norm_pub_key.as_deref())
            .bind(installation_id)
            .bind(new_session_id)
            .bind(next_gen_i64)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
            existing_id
        } else {
            let created_id = Uuid::now_v7();
            sqlx::query(
                r"
                INSERT INTO devices (
                    id, workspace_id, user_id, device_name, platform, installation_key_hash,
                    registered_at, last_seen_at, public_key_hex, installation_id,
                    active_session_id, session_generation, status
                )
                VALUES (
                    $1, $2, $3, $4, $5, $6,
                    to_timestamp($7), to_timestamp($7), $8, $9,
                    $10, $11, 'active'
                )
                ",
            )
            .bind(created_id)
            .bind(workspace_id)
            .bind(user_id)
            .bind(clean_name)
            .bind(&norm_platform)
            .bind(canonical_inst_hash.as_slice())
            .bind(now_secs)
            .bind(norm_pub_key.as_deref())
            .bind(installation_id)
            .bind(new_session_id)
            .bind(next_gen_i64)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
            created_id
        };

        if let Some(prev_id) = prev_device_id {
            sqlx::query("UPDATE devices SET replaced_by_device_id = $2 WHERE id = $1")
                .bind(prev_id)
                .bind(new_device_id)
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
        }

        // Bind the new session row to this device and generation.
        sqlx::query("UPDATE sessions SET device_id = $2, session_generation = $3 WHERE id = $1")
            .bind(new_session_id)
            .bind(new_device_id)
            .bind(next_gen_i64)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let reason = if prev_device_id.is_some() {
            "automatic_single_device_replacement"
        } else {
            "initial_device_registration"
        };
        let event = DeviceReplacementEvent {
            id: Uuid::now_v7(),
            user_id,
            previous_device_id: prev_device_id,
            new_device_id: Some(new_device_id),
            trigger_kind: "new_device_login".to_owned(),
            actor_user_id: Some(user_id),
            reason: reason.to_owned(),
            new_session_generation: next_gen,
            created_at: now_secs,
        };
        sqlx::query(
            r"
            INSERT INTO device_replacement_events (
                id, user_id, previous_device_id, new_device_id, trigger_kind,
                actor_user_id, reason, new_session_generation
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(event.id)
        .bind(event.user_id)
        .bind(event.previous_device_id)
        .bind(event.new_device_id)
        .bind(&event.trigger_kind)
        .bind(event.actor_user_id)
        .bind(&event.reason)
        .bind(next_gen_i64)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let rec = RegisteredDeviceRecord {
            device_id: new_device_id,
            user_id,
            workspace_id,
            device_name: clean_name.to_owned(),
            installation_id,
            public_key_hex: public_key_hex.map(|s| s.trim().to_ascii_lowercase()),
            active_session_id: Some(new_session_id),
            session_generation: next_gen,
            status: DeviceStatus::Active,
            registered_at: now_secs,
            last_seen_at: now_secs,
            revoked_at: None,
            replaced_by_device_id: None,
        };
        Ok((rec, prev_device_id, event))
    }

    /// Verify active sole device & session generation directly against PostgreSQL.
    /// Fails closed whenever the session was revoked/replaced or no active device matches!
    pub async fn verify_active_session_device_pg(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        session_token_hash: &str,
        device_sig_header: Option<(&str, i64)>,
        now_secs: i64,
    ) -> Result<(Option<RegisteredDeviceRecord>, u64), AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let user_gen_i64: i64 =
            sqlx::query_scalar("SELECT session_generation FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?
                .ok_or(AuthError::Revoked)?;
        let current_gen = u64::try_from(user_gen_i64).unwrap_or(1);

        let sess_row = sqlx::query(
            "SELECT revoked_at IS NOT NULL AS is_revoked, revocation_reason, session_generation FROM sessions WHERE id = $1",
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .ok_or(AuthError::Revoked)?;

        let is_revoked: bool = sess_row.try_get("is_revoked").unwrap_or(true);
        let rev_reason: Option<String> = sess_row.try_get("revocation_reason").ok().flatten();
        if is_revoked {
            return Err(match rev_reason.as_deref() {
                Some("replaced_by_new_device") => AuthError::DeviceReplaced,
                _ => AuthError::Revoked,
            });
        }

        let dev_row = sqlx::query(
            r"
            SELECT
                id, workspace_id, device_name, installation_id, public_key_hex,
                active_session_id, session_generation, status,
                EXTRACT(EPOCH FROM registered_at)::BIGINT AS registered_at_secs,
                EXTRACT(EPOCH FROM last_seen_at)::BIGINT AS last_seen_at_secs
            FROM devices
            WHERE user_id = $1 AND revoked_at IS NULL
            LIMIT 1
            ",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        // Fail closed if no active device exists in PostgreSQL!
        let Some(row) = dev_row else {
            return Err(AuthError::Revoked);
        };

        let dev_session_id: Option<Uuid> = row.try_get("active_session_id").ok().flatten();
        let dev_gen_i64: i64 = row.try_get("session_generation").unwrap_or(0);
        let dev_gen = u64::try_from(dev_gen_i64).unwrap_or(0);
        if dev_session_id != Some(session_id) || dev_gen != current_gen {
            return Err(AuthError::DeviceReplaced);
        }

        let public_key_hex: Option<String> = row.try_get("public_key_hex").ok().flatten();
        if let Some(ref pk_hex) = public_key_hex {
            let Some((sig_hex, ts_secs)) = device_sig_header else {
                return Err(AuthError::DeviceProofInvalid);
            };
            verify_device_ed25519_proof(pk_hex, session_token_hash, ts_secs, sig_hex, now_secs)?;
        }

        let device_id: Uuid = row
            .try_get("id")
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("UPDATE devices SET last_seen_at = to_timestamp($2) WHERE id = $1")
            .bind(device_id)
            .bind(now_secs)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let status_raw: String = row
            .try_get("status")
            .unwrap_or_else(|_| "active".to_owned());
        Ok((
            Some(RegisteredDeviceRecord {
                device_id,
                user_id,
                workspace_id: row
                    .try_get("workspace_id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                device_name: row.try_get("device_name").unwrap_or_default(),
                installation_id: row
                    .try_get::<Option<Uuid>, _>("installation_id")
                    .ok()
                    .flatten()
                    .unwrap_or_else(Uuid::nil),
                public_key_hex,
                active_session_id: dev_session_id,
                session_generation: dev_gen,
                status: parse_device_status(&status_raw),
                registered_at: row.try_get("registered_at_secs").unwrap_or(0),
                last_seen_at: now_secs,
                revoked_at: None,
                replaced_by_device_id: None,
            }),
            current_gen,
        ))
    }

    /// Atomically revoke all active devices & sessions for a user via Owner reset in PostgreSQL.
    pub async fn owner_reset_devices_pg(
        &self,
        owner_user_id: Uuid,
        target_user_id: Uuid,
        reason: &str,
        now_secs: i64,
    ) -> Result<DeviceReplacementEvent, AuthError> {
        let trimmed = reason.trim();
        if trimmed.is_empty() || trimmed.len() > 240 {
            return Err(AuthError::InvalidRequest);
        }
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(format!("device_auth:{target_user_id}"))
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let next_gen_i64: i64 = sqlx::query_scalar(
            "UPDATE users SET session_generation = session_generation + 1 WHERE id = $1 RETURNING session_generation",
        )
        .bind(target_user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        let prev_device_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM devices WHERE user_id = $1 AND revoked_at IS NULL LIMIT 1",
        )
        .bind(target_user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            r"
            UPDATE devices
            SET revoked_at = to_timestamp($2),
                status = 'revoked_by_owner',
                active_session_id = NULL
            WHERE user_id = $1 AND revoked_at IS NULL
            ",
        )
        .bind(target_user_id)
        .bind(now_secs)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            r"
            UPDATE sessions
            SET revoked_at = COALESCE(revoked_at, to_timestamp($2)),
                revocation_reason = COALESCE(revocation_reason, 'revoked_by_owner')
            WHERE user_id = $1 AND revoked_at IS NULL
            ",
        )
        .bind(target_user_id)
        .bind(now_secs)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let event = DeviceReplacementEvent {
            id: Uuid::now_v7(),
            user_id: target_user_id,
            previous_device_id: prev_device_id,
            new_device_id: None,
            trigger_kind: "owner_reset".to_owned(),
            actor_user_id: Some(owner_user_id),
            reason: trimmed.to_owned(),
            new_session_generation: u64::try_from(next_gen_i64).unwrap_or(1),
            created_at: now_secs,
        };
        sqlx::query(
            r"
            INSERT INTO device_replacement_events (
                id, user_id, previous_device_id, new_device_id, trigger_kind,
                actor_user_id, reason, new_session_generation
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(event.id)
        .bind(event.user_id)
        .bind(event.previous_device_id)
        .bind(event.new_device_id)
        .bind(&event.trigger_kind)
        .bind(event.actor_user_id)
        .bind(&event.reason)
        .bind(next_gen_i64)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(event)
    }

    /// Direct PostgreSQL query for Owner Devices Overview (`devices` + `device_replacement_events`).
    pub async fn list_devices_and_replacements_pg(
        &self,
        filter_user_id: Option<Uuid>,
    ) -> Result<(Vec<RegisteredDeviceRecord>, Vec<DeviceReplacementEvent>), AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let dev_rows = sqlx::query(
            r"
            SELECT
                id, user_id, workspace_id, device_name, installation_id, public_key_hex,
                active_session_id, session_generation, status, replaced_by_device_id,
                EXTRACT(EPOCH FROM registered_at)::BIGINT AS registered_at_secs,
                EXTRACT(EPOCH FROM last_seen_at)::BIGINT AS last_seen_at_secs,
                EXTRACT(EPOCH FROM revoked_at)::BIGINT AS revoked_at_secs
            FROM devices
            WHERE ($1::UUID IS NULL OR user_id = $1)
            ORDER BY registered_at DESC
            ",
        )
        .bind(filter_user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let rep_rows = sqlx::query(
            r"
            SELECT
                id, user_id, previous_device_id, new_device_id, trigger_kind,
                actor_user_id, reason, new_session_generation,
                EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs
            FROM device_replacement_events
            WHERE ($1::UUID IS NULL OR user_id = $1)
            ORDER BY created_at DESC
            ",
        )
        .bind(filter_user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let devices = dev_rows
            .iter()
            .map(|row| RegisteredDeviceRecord {
                device_id: row.try_get("id").unwrap_or_else(|_| Uuid::nil()),
                user_id: row.try_get("user_id").unwrap_or_else(|_| Uuid::nil()),
                workspace_id: row.try_get("workspace_id").unwrap_or_else(|_| Uuid::nil()),
                device_name: row.try_get("device_name").unwrap_or_default(),
                installation_id: row
                    .try_get::<Option<Uuid>, _>("installation_id")
                    .ok()
                    .flatten()
                    .unwrap_or_else(Uuid::nil),
                public_key_hex: row.try_get("public_key_hex").ok().flatten(),
                active_session_id: row.try_get("active_session_id").ok().flatten(),
                session_generation: u64::try_from(
                    row.try_get::<i64, _>("session_generation").unwrap_or(1),
                )
                .unwrap_or(1),
                status: parse_device_status(
                    &row.try_get::<String, _>("status")
                        .unwrap_or_else(|_| "active".to_owned()),
                ),
                registered_at: row.try_get("registered_at_secs").unwrap_or(0),
                last_seen_at: row.try_get("last_seen_at_secs").unwrap_or(0),
                revoked_at: row.try_get("revoked_at_secs").ok().flatten(),
                replaced_by_device_id: row.try_get("replaced_by_device_id").ok().flatten(),
            })
            .collect();

        let replacements = rep_rows
            .iter()
            .map(|row| DeviceReplacementEvent {
                id: row.try_get("id").unwrap_or_else(|_| Uuid::nil()),
                user_id: row.try_get("user_id").unwrap_or_else(|_| Uuid::nil()),
                previous_device_id: row.try_get("previous_device_id").ok().flatten(),
                new_device_id: row.try_get("new_device_id").ok().flatten(),
                trigger_kind: row.try_get("trigger_kind").unwrap_or_default(),
                actor_user_id: row.try_get("actor_user_id").ok().flatten(),
                reason: row.try_get("reason").unwrap_or_default(),
                new_session_generation: u64::try_from(
                    row.try_get::<i64, _>("new_session_generation").unwrap_or(1),
                )
                .unwrap_or(1),
                created_at: row.try_get("created_at_secs").unwrap_or(0),
            })
            .collect();

        Ok((devices, replacements))
    }

    /// Inspect session revocation reason directly from PostgreSQL (`replaced_by_new_device` -> `SF-AUTH-019`).
    pub async fn revocation_reason_for_token_hash_pg(
        &self,
        token_hash_bytes: &[u8],
    ) -> Result<Option<DeviceStatus>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let reason: Option<String> = sqlx::query_scalar(
            "SELECT revocation_reason FROM sessions WHERE token_hash = $1 AND revoked_at IS NOT NULL",
        )
        .bind(token_hash_bytes)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .flatten();
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(reason.as_deref().map(parse_device_status))
    }

    /// Load hash-chained Owner audit log entries directly from PostgreSQL.
    pub async fn load_audit_entries_pg(&self) -> Result<Vec<AuditLogEntry>, AuthError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, seq, actor_user_id, action, target_type, target_id,
                outcome, before_state, after_state, previous_hash, current_hash,
                to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS ts_iso
            FROM owner_audit_log
            ORDER BY seq ASC
            "#,
        )
        .fetch_all(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let outcome_raw: String = row
                .try_get("outcome")
                .unwrap_or_else(|_| "allowed".to_owned());
            let outcome = match outcome_raw.as_str() {
                "denied" => AuditOutcome::Denied,
                "error" => AuditOutcome::Error,
                _ => AuditOutcome::Allowed,
            };
            out.push(AuditLogEntry {
                id: row
                    .try_get("id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                seq: u64::try_from(row.try_get::<i64, _>("seq").unwrap_or(1)).unwrap_or(1),
                timestamp: row.try_get("ts_iso").unwrap_or_default(),
                actor_user_id: row.try_get("actor_user_id").ok().flatten(),
                action: row.try_get("action").unwrap_or_default(),
                target_type: row.try_get("target_type").unwrap_or_default(),
                target_id: row.try_get("target_id").unwrap_or_default(),
                outcome,
                before_state: row.try_get("before_state").ok().flatten(),
                after_state: row.try_get("after_state").ok().flatten(),
                previous_hash: row.try_get("previous_hash").unwrap_or_default(),
                current_hash: row.try_get("current_hash").unwrap_or_default(),
            });
        }
        Ok(out)
    }
}
