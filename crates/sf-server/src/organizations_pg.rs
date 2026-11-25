//! Authoritative PostgreSQL 18 SQLx Repository for Organizations, Shared Workspaces,
//! Verified-Email Invitations, 4-Role RBAC, and X25519 Key Directory (Migrations `0009` + `0017`).

use base64::Engine;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::error::AuthError;
use crate::organizations::{
    InvitationStatus, MemberDevicePublicKey, OrgRole, Organization, OrganizationInvitation,
    OrganizationInvitationView, OrganizationMember, OrganizationMembershipSummary,
    PendingUserInvitationView, ValidatedWrappedKeyBytes, WrappedWorkspaceKeyRecord,
};
use crate::pg_store::PgControlPlaneStore;

const ORG_SELECT_COLUMNS: &str = r"
    id, workspace_id, billing_workspace_id, name, purchased_seats,
    billing_cycle, owner_user_id, active_key_epoch,
    EXTRACT(EPOCH FROM created_at)::bigint AS created_at_secs,
    EXTRACT(EPOCH FROM deleted_at)::bigint AS deleted_at_secs
";

const INVITE_SELECT_COLUMNS: &str = r"
    id, organization_id, workspace_id, invited_email_norm,
    role, token_hash, invited_by,
    EXTRACT(EPOCH FROM expires_at)::bigint AS expires_at_secs,
    EXTRACT(EPOCH FROM accepted_at)::bigint AS accepted_at_secs,
    EXTRACT(EPOCH FROM declined_at)::bigint AS declined_at_secs,
    EXTRACT(EPOCH FROM revoked_at)::bigint AS revoked_at_secs,
    EXTRACT(EPOCH FROM created_at)::bigint AS created_at_secs
";

async fn set_tx_workspace_context(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: Option<Uuid>,
    user_id: Option<Uuid>,
    user_email_norm: Option<&str>,
    invitation_token_hash: Option<&str>,
) -> Result<(), AuthError> {
    if let Some(ws) = workspace_id {
        let ws_s = ws.to_string();
        sqlx::query(
            "SELECT set_config('app.current_workspace_id', $1, true), set_config('app.workspace_id', $1, true)",
        )
        .bind(&ws_s)
        .execute(&mut **tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    }
    if let Some(uid) = user_id {
        sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
            .bind(uid.to_string())
            .execute(&mut **tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
    }
    if let Some(email) = user_email_norm {
        sqlx::query("SELECT set_config('app.current_user_email', $1, true)")
            .bind(email)
            .execute(&mut **tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
    }
    if let Some(th) = invitation_token_hash {
        sqlx::query("SELECT set_config('app.invitation_token_hash', $1, true)")
            .bind(th)
            .execute(&mut **tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
    }
    Ok(())
}

fn row_to_organization(row: &sqlx::postgres::PgRow) -> Result<Organization, AuthError> {
    let created_secs: i64 = row
        .try_get("created_at_secs")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let deleted_secs: Option<i64> = row.try_get("deleted_at_secs").ok().flatten();
    let seats_i32: i32 = row
        .try_get("purchased_seats")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let epoch_i32: i32 = row.try_get("active_key_epoch").unwrap_or(1);
    Ok(Organization {
        id: row
            .try_get("id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        workspace_id: row
            .try_get("workspace_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        billing_workspace_id: row.try_get("billing_workspace_id").ok().flatten(),
        name: row
            .try_get("name")
            .map_err(|_| AuthError::BackendUnavailable)?,
        purchased_seats: u32::try_from(seats_i32).unwrap_or(3),
        billing_cycle: row
            .try_get("billing_cycle")
            .unwrap_or_else(|_| "annual_per_seat".to_owned()),
        owner_user_id: row
            .try_get("owner_user_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        active_key_epoch: u32::try_from(epoch_i32).unwrap_or(1),
        created_at: created_secs,
        deleted_at: deleted_secs,
    })
}

fn row_to_member(row: &sqlx::postgres::PgRow) -> Result<OrganizationMember, AuthError> {
    let role_str: String = row
        .try_get("role")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let joined_secs: i64 = row
        .try_get("joined_at_secs")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let removed_secs: Option<i64> = row.try_get("removed_at_secs").ok().flatten();
    Ok(OrganizationMember {
        organization_id: row
            .try_get("organization_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        workspace_id: row
            .try_get("workspace_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        user_id: row
            .try_get("user_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        email: row.try_get("email").ok().flatten(),
        display_name: row.try_get("display_name").ok().flatten(),
        role: OrgRole::parse_str(&role_str)?,
        joined_at: joined_secs,
        removed_at: removed_secs,
    })
}

fn row_to_invitation(row: &sqlx::postgres::PgRow) -> Result<OrganizationInvitation, AuthError> {
    let role_str: String = row
        .try_get("role")
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(OrganizationInvitation {
        id: row
            .try_get("id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        organization_id: row
            .try_get("organization_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        workspace_id: row
            .try_get("workspace_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        invited_email: row
            .try_get("invited_email_norm")
            .map_err(|_| AuthError::BackendUnavailable)?,
        role: OrgRole::parse_str(&role_str)?,
        token_hash: row
            .try_get("token_hash")
            .map_err(|_| AuthError::BackendUnavailable)?,
        invited_by: row
            .try_get("invited_by")
            .map_err(|_| AuthError::BackendUnavailable)?,
        expires_at: row
            .try_get("expires_at_secs")
            .map_err(|_| AuthError::BackendUnavailable)?,
        accepted_at: row.try_get("accepted_at_secs").ok().flatten(),
        declined_at: row.try_get("declined_at_secs").ok().flatten(),
        revoked_at: row.try_get("revoked_at_secs").ok().flatten(),
        created_at: row
            .try_get("created_at_secs")
            .map_err(|_| AuthError::BackendUnavailable)?,
    })
}

async fn lookup_org_and_lock(
    tx: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    actor_user_id: Uuid,
    owner_override: bool,
) -> Result<Organization, AuthError> {
    if owner_override {
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut **tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
    } else {
        set_tx_workspace_context(tx, None, Some(actor_user_id), None, None).await?;
    }
    let query_sql = format!(
        "SELECT {ORG_SELECT_COLUMNS} FROM organizations WHERE id = $1 AND deleted_at IS NULL FOR UPDATE"
    );
    let row = sqlx::query(&query_sql)
        .bind(organization_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .ok_or(AuthError::OrgPermissionDenied)?;

    let org = row_to_organization(&row)?;
    set_tx_workspace_context(tx, Some(org.workspace_id), Some(actor_user_id), None, None).await?;
    Ok(org)
}

async fn lookup_active_actor_role(
    tx: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    actor_user_id: Uuid,
) -> Result<OrgRole, AuthError> {
    let role_opt: Option<String> = sqlx::query_scalar(
        r"
        SELECT role
        FROM organization_members
        WHERE organization_id = $1 AND user_id = $2 AND removed_at IS NULL
        ",
    )
    .bind(organization_id)
    .bind(actor_user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;
    let role_str = role_opt.ok_or(AuthError::OrgPermissionDenied)?;
    OrgRole::parse_str(&role_str)
}

pub async fn create_organization_pg(
    pg: &PgControlPlaneStore,
    billing_workspace_id: Uuid,
    owner_user_id: Uuid,
    name: &str,
    purchased_seats: u32,
    billing_cycle: &str,
) -> Result<Organization, AuthError> {
    let shared_workspace_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let seats_i32 = i32::try_from(purchased_seats).map_err(|_| AuthError::InvalidRequest)?;

    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    set_tx_workspace_context(
        &mut tx,
        Some(shared_workspace_id),
        Some(owner_user_id),
        None,
        None,
    )
    .await?;

    sqlx::query(
        r"
        INSERT INTO workspaces (id, name, plan_code)
        VALUES ($1, $2, 'free')
        ",
    )
    .bind(shared_workspace_id)
    .bind(name)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        r"
        INSERT INTO memberships (workspace_id, user_id, role)
        VALUES ($1, $2, 'owner')
        ON CONFLICT (workspace_id, user_id) DO NOTHING
        ",
    )
    .bind(shared_workspace_id)
    .bind(owner_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let insert_sql = format!(
        r"
        INSERT INTO organizations (
            id, workspace_id, billing_workspace_id, name, min_seats,
            purchased_seats, billing_cycle, owner_user_id, active_key_epoch
        )
        VALUES ($1, $2, $3, $4, 3, $5, $6, $7, 1)
        RETURNING {ORG_SELECT_COLUMNS}
        "
    );
    let row = sqlx::query(&insert_sql)
        .bind(org_id)
        .bind(shared_workspace_id)
        .bind(billing_workspace_id)
        .bind(name)
        .bind(seats_i32)
        .bind(billing_cycle)
        .bind(owner_user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        r"
        INSERT INTO organization_members (
            organization_id, workspace_id, user_id, role, invited_by
        )
        VALUES ($1, $2, $3, 'owner', $3)
        ",
    )
    .bind(org_id)
    .bind(shared_workspace_id)
    .bind(owner_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    row_to_organization(&row)
}

pub async fn get_organization_for_actor_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
    is_platform_owner: bool,
    now_secs: i64,
) -> Result<(Organization, Option<OrgRole>, u32, u32), AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org =
        lookup_org_and_lock(&mut tx, organization_id, actor_user_id, is_platform_owner).await?;
    let my_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id)
        .await
        .ok();
    if my_role.is_none() && !is_platform_owner {
        return Err(AuthError::OrgPermissionDenied);
    }

    let active_seats: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_members WHERE organization_id = $1 AND removed_at IS NULL",
    )
    .bind(organization_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let pending_invites: i64 = sqlx::query_scalar(
        r"
        SELECT count(*)
        FROM organization_invitations
        WHERE organization_id = $1
          AND accepted_at IS NULL
          AND declined_at IS NULL
          AND revoked_at IS NULL
          AND expires_at > to_timestamp($2::double precision)
        ",
    )
    .bind(organization_id)
    .bind(now_secs as f64)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok((
        org,
        my_role,
        u32::try_from(active_seats).unwrap_or(0),
        u32::try_from(pending_invites).unwrap_or(0),
    ))
}

pub async fn get_organization_by_workspace_pg(
    pg: &PgControlPlaneStore,
    workspace_id: Uuid,
) -> Result<Option<Organization>, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    set_tx_workspace_context(&mut tx, Some(workspace_id), None, None, None).await?;
    let query_sql = format!(
        "SELECT {ORG_SELECT_COLUMNS} FROM organizations WHERE workspace_id = $1 AND deleted_at IS NULL"
    );
    let row_opt = sqlx::query(&query_sql)
        .bind(workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    row_opt.map(|r| row_to_organization(&r)).transpose()
}

pub async fn require_workspace_member_role_pg(
    pg: &PgControlPlaneStore,
    workspace_id: Uuid,
    actor_user_id: Uuid,
) -> Result<(Organization, OrgRole), AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    set_tx_workspace_context(&mut tx, Some(workspace_id), Some(actor_user_id), None, None).await?;
    let row = sqlx::query(
        r"
        SELECT o.id, o.workspace_id, o.billing_workspace_id, o.name, o.purchased_seats,
               o.billing_cycle, o.owner_user_id, o.active_key_epoch,
               EXTRACT(EPOCH FROM o.created_at)::bigint AS created_at_secs,
               EXTRACT(EPOCH FROM o.deleted_at)::bigint AS deleted_at_secs,
               om.role AS member_role
        FROM organizations o
        INNER JOIN organization_members om
            ON om.organization_id = o.id
           AND om.user_id = $2
           AND om.removed_at IS NULL
        WHERE o.workspace_id = $1
          AND o.deleted_at IS NULL
        ",
    )
    .bind(workspace_id)
    .bind(actor_user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?
    .ok_or(AuthError::OrgPermissionDenied)?;

    let role_str: String = row
        .try_get("member_role")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org = row_to_organization(&row)?;
    let role = OrgRole::parse_str(&role_str)?;
    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok((org, role))
}

pub async fn list_user_organizations_and_invites_pg(
    pg: &PgControlPlaneStore,
    user_id: Uuid,
    verified_email_norm: Option<&str>,
    now_secs: i64,
) -> Result<
    (
        Vec<OrganizationMembershipSummary>,
        Vec<PendingUserInvitationView>,
    ),
    AuthError,
> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    let db_verified_email: Option<String> = sqlx::query_scalar(
        r"
        SELECT lower(btrim(email))
        FROM users
        WHERE id = $1
          AND email_verified_at IS NOT NULL
          AND deleted_at IS NULL
          AND suspended_at IS NULL
        ",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let effective_email = db_verified_email
        .as_deref()
        .or(verified_email_norm)
        .map(|s| s.trim().to_ascii_lowercase());

    set_tx_workspace_context(
        &mut tx,
        None,
        Some(user_id),
        effective_email.as_deref(),
        None,
    )
    .await?;

    let member_rows = sqlx::query(
        r"
        SELECT o.id, o.workspace_id, o.billing_workspace_id, o.name, o.purchased_seats,
               o.billing_cycle, o.owner_user_id, o.active_key_epoch,
               EXTRACT(EPOCH FROM o.created_at)::bigint AS created_at_secs,
               EXTRACT(EPOCH FROM o.deleted_at)::bigint AS deleted_at_secs,
               om.role AS member_role
        FROM organization_members om
        INNER JOIN organizations o ON o.id = om.organization_id
        WHERE om.user_id = $1
          AND om.removed_at IS NULL
          AND o.deleted_at IS NULL
        ORDER BY o.created_at ASC
        ",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let mut summaries = Vec::with_capacity(member_rows.len());
    for row in &member_rows {
        let org = row_to_organization(row)?;
        let role_str: String = row
            .try_get("member_role")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let my_role = OrgRole::parse_str(&role_str)?;
        set_tx_workspace_context(
            &mut tx,
            Some(org.workspace_id),
            Some(user_id),
            effective_email.as_deref(),
            None,
        )
        .await?;

        let active_members_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM organization_members WHERE organization_id = $1 AND removed_at IS NULL",
        )
        .bind(org.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let pending_invitations_count: i64 = sqlx::query_scalar(
            r"
            SELECT count(*)
            FROM organization_invitations
            WHERE organization_id = $1
              AND accepted_at IS NULL
              AND declined_at IS NULL
              AND revoked_at IS NULL
              AND expires_at > to_timestamp($2::double precision)
            ",
        )
        .bind(org.id)
        .bind(now_secs as f64)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        summaries.push(OrganizationMembershipSummary {
            organization: org,
            my_role,
            active_members_count: u32::try_from(active_members_count).unwrap_or(0),
            pending_invitations_count: u32::try_from(pending_invitations_count).unwrap_or(0),
        });
    }

    let mut pending_invites = Vec::new();
    if let Some(ref em) = effective_email {
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let inv_rows = sqlx::query(
            r"
            SELECT i.id, i.organization_id, i.workspace_id, i.invited_email_norm,
                   i.role,
                   EXTRACT(EPOCH FROM i.expires_at)::bigint AS expires_at_secs,
                   EXTRACT(EPOCH FROM i.created_at)::bigint AS created_at_secs,
                   o.name AS organization_name
            FROM organization_invitations i
            INNER JOIN organizations o ON o.id = i.organization_id
            WHERE i.invited_email_norm = $1
              AND i.accepted_at IS NULL
              AND i.declined_at IS NULL
              AND i.revoked_at IS NULL
              AND i.expires_at > to_timestamp($2::double precision)
              AND o.deleted_at IS NULL
            ORDER BY i.created_at DESC
            ",
        )
        .bind(em)
        .bind(now_secs as f64)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        for r in inv_rows {
            let role_s: String = r
                .try_get("role")
                .map_err(|_| AuthError::BackendUnavailable)?;
            pending_invites.push(PendingUserInvitationView {
                invitation_id: r.try_get("id").map_err(|_| AuthError::BackendUnavailable)?,
                organization_id: r
                    .try_get("organization_id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                organization_name: r
                    .try_get("organization_name")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                workspace_id: r
                    .try_get("workspace_id")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                invited_email: r
                    .try_get("invited_email_norm")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                role: OrgRole::parse_str(&role_s)?,
                expires_at: r
                    .try_get("expires_at_secs")
                    .map_err(|_| AuthError::BackendUnavailable)?,
                created_at: r
                    .try_get("created_at_secs")
                    .map_err(|_| AuthError::BackendUnavailable)?,
            });
        }
    }

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok((summaries, pending_invites))
}

pub async fn sync_purchased_seats_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    new_paid_seats: u32,
) -> Result<(), AuthError> {
    let seats_i32 = i32::try_from(new_paid_seats).map_err(|_| AuthError::InvalidRequest)?;
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    sqlx::query(
        "UPDATE organizations SET purchased_seats = $2, updated_at = now() WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(organization_id)
    .bind(seats_i32)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;
    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn invite_member_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
    invited_email_norm: &str,
    role: OrgRole,
    token_hash: &str,
    max_paid_seats: Option<u32>,
    ttl_secs: i64,
    now_secs: i64,
) -> Result<OrganizationInvitation, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let actor_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;
    if !actor_role.can_send_invitations() {
        return Err(AuthError::OrgPermissionDenied);
    }
    if actor_role == OrgRole::Admin && role == OrgRole::Admin {
        return Err(AuthError::OrgPermissionDenied);
    }

    let already_member: bool = sqlx::query_scalar(
        r"
        SELECT EXISTS (
            SELECT 1
            FROM organization_members om
            INNER JOIN users u ON u.id = om.user_id
            WHERE om.organization_id = $1
              AND om.removed_at IS NULL
              AND lower(btrim(u.email)) = $2
        )
        ",
    )
    .bind(organization_id)
    .bind(invited_email_norm)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;
    if already_member {
        return Err(AuthError::SyncVersionConflict);
    }

    let expires_secs = now_secs + ttl_secs;

    sqlx::query(
        r"
        UPDATE organization_invitations
        SET revoked_at = expires_at
        WHERE organization_id = $1
          AND invited_email_norm = $2
          AND accepted_at IS NULL
          AND declined_at IS NULL
          AND revoked_at IS NULL
          AND expires_at <= to_timestamp($3::double precision)
        ",
    )
    .bind(organization_id)
    .bind(invited_email_norm)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let duplicate_pending: bool = sqlx::query_scalar(
        r"
        SELECT EXISTS (
            SELECT 1
            FROM organization_invitations
            WHERE organization_id = $1
              AND invited_email_norm = $2
              AND accepted_at IS NULL
              AND declined_at IS NULL
              AND revoked_at IS NULL
              AND expires_at > to_timestamp($3::double precision)
        )
        ",
    )
    .bind(organization_id)
    .bind(invited_email_norm)
    .bind(now_secs as f64)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;
    if duplicate_pending {
        return Err(AuthError::SyncVersionConflict);
    }

    let seat_cap = match max_paid_seats {
        Some(paid) => org.purchased_seats.min(paid),
        None => org.purchased_seats,
    };
    let active_seats: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_members WHERE organization_id = $1 AND removed_at IS NULL",
    )
    .bind(organization_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let pending_seats: i64 = sqlx::query_scalar(
        r"
        SELECT count(*)
        FROM organization_invitations
        WHERE organization_id = $1
          AND accepted_at IS NULL
          AND declined_at IS NULL
          AND revoked_at IS NULL
          AND expires_at > to_timestamp($2::double precision)
        ",
    )
    .bind(organization_id)
    .bind(now_secs as f64)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    if u32::try_from(active_seats + pending_seats).unwrap_or(u32::MAX) >= seat_cap {
        return Err(AuthError::QuotaExceeded);
    }

    let invite_id = Uuid::now_v7();
    let insert_sql = format!(
        r"
        INSERT INTO organization_invitations (
            id, organization_id, workspace_id, invited_email_norm,
            role, token_hash, invited_by, expires_at, created_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7,
            to_timestamp($8::double precision),
            to_timestamp($9::double precision)
        )
        RETURNING {INVITE_SELECT_COLUMNS}
        "
    );
    let row = sqlx::query(&insert_sql)
        .bind(invite_id)
        .bind(organization_id)
        .bind(org.workspace_id)
        .bind(invited_email_norm)
        .bind(role.as_str())
        .bind(token_hash)
        .bind(actor_user_id)
        .bind(expires_secs as f64)
        .bind(now_secs as f64)
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| {
            if let sqlx::Error::Database(db_err) = &err {
                if db_err.is_unique_violation() {
                    return AuthError::SyncVersionConflict;
                }
            }
            AuthError::BackendUnavailable
        })?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    row_to_invitation(&row)
}

pub async fn list_invitations_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
    now_secs: i64,
) -> Result<Vec<OrganizationInvitationView>, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let _org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let actor_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;
    if !actor_role.can_send_invitations() {
        return Err(AuthError::OrgPermissionDenied);
    }
    let query_sql = format!(
        "SELECT {INVITE_SELECT_COLUMNS} FROM organization_invitations WHERE organization_id = $1 ORDER BY created_at DESC"
    );
    let rows = sqlx::query(&query_sql)
        .bind(organization_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let mut views = Vec::with_capacity(rows.len());
    for r in rows {
        views.push(row_to_invitation(&r)?.to_view(now_secs));
    }
    Ok(views)
}

pub async fn revoke_invitation_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    invitation_id: Uuid,
    actor_user_id: Uuid,
    now_secs: i64,
) -> Result<OrganizationInvitationView, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let _org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let actor_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;
    if !actor_role.can_send_invitations() {
        return Err(AuthError::OrgPermissionDenied);
    }
    let update_sql = format!(
        r"
        UPDATE organization_invitations
        SET revoked_at = to_timestamp($3::double precision)
        WHERE id = $1
          AND organization_id = $2
          AND accepted_at IS NULL
          AND declined_at IS NULL
          AND revoked_at IS NULL
        RETURNING {INVITE_SELECT_COLUMNS}
        "
    );
    let row = sqlx::query(&update_sql)
        .bind(invitation_id)
        .bind(organization_id)
        .bind(now_secs as f64)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .ok_or(AuthError::InvalidVerificationToken)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(row_to_invitation(&row)?.to_view(now_secs))
}

pub async fn accept_invitation_pg(
    pg: &PgControlPlaneStore,
    token_hash: Option<&str>,
    invitation_id: Option<Uuid>,
    joining_user_id: Uuid,
    fallback_verified_email_norm: &str,
    max_paid_seats: Option<u32>,
    now_secs: i64,
) -> Result<OrganizationMember, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    let user_row = sqlx::query(
        r"
        SELECT lower(btrim(email)) AS email_norm,
               display_name,
               (email_verified_at IS NOT NULL) AS is_verified,
               (deleted_at IS NOT NULL OR suspended_at IS NOT NULL) AS is_inactive
        FROM users
        WHERE id = $1
        ",
    )
    .bind(joining_user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let (verified_email_norm, display_name) = match user_row {
        Some(r) => {
            let inactive: bool = r.try_get("is_inactive").unwrap_or(false);
            let is_verified: bool = r.try_get("is_verified").unwrap_or(false);
            if inactive || !is_verified {
                return Err(AuthError::OrgPermissionDenied);
            }
            let em: String = r
                .try_get("email_norm")
                .map_err(|_| AuthError::BackendUnavailable)?;
            let dn: Option<String> = r.try_get("display_name").ok();
            (em, dn)
        }
        None => (fallback_verified_email_norm.to_owned(), None),
    };

    set_tx_workspace_context(
        &mut tx,
        None,
        Some(joining_user_id),
        Some(&verified_email_norm),
        token_hash,
    )
    .await?;

    let inv_row = match (token_hash, invitation_id) {
        (Some(th), _) => {
            let sql = format!(
                "SELECT {INVITE_SELECT_COLUMNS} FROM organization_invitations WHERE token_hash = $1 FOR UPDATE"
            );
            sqlx::query(&sql)
                .bind(th)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?
        }
        (None, Some(id)) => {
            let sql = format!(
                "SELECT {INVITE_SELECT_COLUMNS} FROM organization_invitations WHERE id = $1 FOR UPDATE"
            );
            sqlx::query(&sql)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?
        }
        (None, None) => return Err(AuthError::InvalidRequest),
    }
    .ok_or(AuthError::InvalidVerificationToken)?;

    let inv = row_to_invitation(&inv_row)?;
    if inv.invited_email != verified_email_norm {
        return Err(AuthError::OrgPermissionDenied);
    }
    if inv.status_at(now_secs) != InvitationStatus::Pending {
        return Err(AuthError::InvalidVerificationToken);
    }

    set_tx_workspace_context(
        &mut tx,
        Some(inv.workspace_id),
        Some(joining_user_id),
        Some(&verified_email_norm),
        token_hash,
    )
    .await?;

    let org_sql = format!(
        "SELECT {ORG_SELECT_COLUMNS} FROM organizations WHERE id = $1 AND deleted_at IS NULL FOR UPDATE"
    );
    let org_row = sqlx::query(&org_sql)
        .bind(inv.organization_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        .ok_or(AuthError::OrgPermissionDenied)?;
    let org = row_to_organization(&org_row)?;

    let already_member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM organization_members WHERE organization_id = $1 AND user_id = $2 AND removed_at IS NULL)",
    )
    .bind(org.id)
    .bind(joining_user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;
    if already_member {
        return Err(AuthError::SyncVersionConflict);
    }

    let seat_cap = match max_paid_seats {
        Some(paid) => org.purchased_seats.min(paid),
        None => org.purchased_seats,
    };
    let active_seats: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_members WHERE organization_id = $1 AND removed_at IS NULL",
    )
    .bind(org.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;
    if u32::try_from(active_seats).unwrap_or(u32::MAX) >= seat_cap {
        return Err(AuthError::QuotaExceeded);
    }

    sqlx::query(
        r"
        UPDATE organization_invitations
        SET accepted_at = to_timestamp($2::double precision), accepted_by_user_id = $3
        WHERE id = $1
        ",
    )
    .bind(inv.id)
    .bind(now_secs as f64)
    .bind(joining_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let membership_role = match inv.role {
        OrgRole::Owner => "owner",
        OrgRole::Admin => "admin",
        OrgRole::Member | OrgRole::Viewer => "member",
    };
    sqlx::query(
        r"
        INSERT INTO memberships (workspace_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = EXCLUDED.role
        ",
    )
    .bind(org.workspace_id)
    .bind(joining_user_id)
    .bind(membership_role)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let mem_row = sqlx::query(
        r"
        INSERT INTO organization_members (
            organization_id, workspace_id, user_id, role, invited_by,
            joined_at, removed_at, updated_at
        )
        VALUES (
            $1, $2, $3, $4, $5,
            to_timestamp($6::double precision), NULL, to_timestamp($6::double precision)
        )
        ON CONFLICT (organization_id, user_id) DO UPDATE
        SET role = EXCLUDED.role,
            invited_by = EXCLUDED.invited_by,
            joined_at = EXCLUDED.joined_at,
            removed_at = NULL,
            updated_at = EXCLUDED.updated_at
        RETURNING organization_id, workspace_id, user_id, role,
                  EXTRACT(EPOCH FROM joined_at)::bigint AS joined_at_secs,
                  EXTRACT(EPOCH FROM removed_at)::bigint AS removed_at_secs
        ",
    )
    .bind(org.id)
    .bind(org.workspace_id)
    .bind(joining_user_id)
    .bind(inv.role.as_str())
    .bind(inv.invited_by)
    .bind(now_secs as f64)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    let mut member = row_to_member(&mem_row)?;
    member.email = Some(verified_email_norm);
    member.display_name = display_name;
    Ok(member)
}

pub async fn decline_invitation_pg(
    pg: &PgControlPlaneStore,
    token_hash: Option<&str>,
    invitation_id: Option<Uuid>,
    user_id: Uuid,
    fallback_verified_email_norm: &str,
    now_secs: i64,
) -> Result<OrganizationInvitationView, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    let user_row = sqlx::query(
        r"
        SELECT lower(btrim(email)) AS email_norm,
               (email_verified_at IS NOT NULL) AS is_verified,
               (deleted_at IS NOT NULL OR suspended_at IS NOT NULL) AS is_inactive
        FROM users
        WHERE id = $1
        ",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let verified_email_norm = match user_row {
        Some(r) => {
            let inactive: bool = r.try_get("is_inactive").unwrap_or(false);
            let is_verified: bool = r.try_get("is_verified").unwrap_or(false);
            if inactive || !is_verified {
                return Err(AuthError::OrgPermissionDenied);
            }
            r.try_get::<String, _>("email_norm")
                .map_err(|_| AuthError::BackendUnavailable)?
        }
        None => fallback_verified_email_norm.to_owned(),
    };

    set_tx_workspace_context(
        &mut tx,
        None,
        Some(user_id),
        Some(&verified_email_norm),
        token_hash,
    )
    .await?;

    let inv_row = match (token_hash, invitation_id) {
        (Some(th), _) => {
            let sql = format!(
                "SELECT {INVITE_SELECT_COLUMNS} FROM organization_invitations WHERE token_hash = $1 FOR UPDATE"
            );
            sqlx::query(&sql)
                .bind(th)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?
        }
        (None, Some(id)) => {
            let sql = format!(
                "SELECT {INVITE_SELECT_COLUMNS} FROM organization_invitations WHERE id = $1 FOR UPDATE"
            );
            sqlx::query(&sql)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?
        }
        (None, None) => return Err(AuthError::InvalidRequest),
    }
    .ok_or(AuthError::InvalidVerificationToken)?;

    let mut inv = row_to_invitation(&inv_row)?;
    if inv.invited_email != verified_email_norm {
        return Err(AuthError::OrgPermissionDenied);
    }
    if inv.status_at(now_secs) != InvitationStatus::Pending {
        return Err(AuthError::InvalidVerificationToken);
    }

    set_tx_workspace_context(
        &mut tx,
        Some(inv.workspace_id),
        Some(user_id),
        Some(&verified_email_norm),
        token_hash,
    )
    .await?;
    sqlx::query(
        "UPDATE organization_invitations SET declined_at = to_timestamp($2::double precision) WHERE id = $1",
    )
    .bind(inv.id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    inv.declined_at = Some(now_secs);
    Ok(inv.to_view(now_secs))
}

pub async fn change_member_role_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
    target_user_id: Uuid,
    new_role: OrgRole,
    now_secs: i64,
) -> Result<OrganizationMember, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let actor_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;

    let target_row = sqlx::query(
        r"
        SELECT om.organization_id, om.workspace_id, om.user_id, om.role,
               EXTRACT(EPOCH FROM om.joined_at)::bigint AS joined_at_secs,
               EXTRACT(EPOCH FROM om.removed_at)::bigint AS removed_at_secs,
               u.email AS email, u.display_name AS display_name
        FROM organization_members om
        LEFT JOIN users u ON u.id = om.user_id
        WHERE om.organization_id = $1 AND om.user_id = $2 AND om.removed_at IS NULL
        FOR UPDATE OF om
        ",
    )
    .bind(organization_id)
    .bind(target_user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?
    .ok_or(AuthError::InvalidRequest)?;

    let mut member = row_to_member(&target_row)?;
    if !actor_role.can_change_role_of(actor_user_id, target_user_id, member.role, new_role) {
        return Err(AuthError::OrgPermissionDenied);
    }

    sqlx::query(
        r"
        UPDATE organization_members
        SET role = $3, updated_at = to_timestamp($4::double precision)
        WHERE organization_id = $1 AND user_id = $2 AND removed_at IS NULL
        ",
    )
    .bind(organization_id)
    .bind(target_user_id)
    .bind(new_role.as_str())
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let ws_role = match new_role {
        OrgRole::Owner => "owner",
        OrgRole::Admin => "admin",
        OrgRole::Member | OrgRole::Viewer => "member",
    };
    sqlx::query("UPDATE memberships SET role = $3 WHERE workspace_id = $1 AND user_id = $2")
        .bind(org.workspace_id)
        .bind(target_user_id)
        .bind(ws_role)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    member.role = new_role;
    Ok(member)
}

pub async fn remove_member_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
    target_user_id: Uuid,
    now_secs: i64,
) -> Result<(), AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let actor_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;
    let target_role = lookup_active_actor_role(&mut tx, organization_id, target_user_id)
        .await
        .map_err(|_| AuthError::InvalidRequest)?;

    if !actor_role.can_remove_target(actor_user_id, target_user_id, target_role) {
        return Err(AuthError::OrgPermissionDenied);
    }

    sqlx::query(
        r"
        UPDATE organization_members
        SET removed_at = to_timestamp($3::double precision),
            updated_at = to_timestamp($3::double precision)
        WHERE organization_id = $1 AND user_id = $2 AND removed_at IS NULL
        ",
    )
    .bind(organization_id)
    .bind(target_user_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query("DELETE FROM memberships WHERE workspace_id = $1 AND user_id = $2")
        .bind(org.workspace_id)
        .bind(target_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        r"
        UPDATE organization_member_public_keys
        SET revoked_at = to_timestamp($3::double precision)
        WHERE organization_id = $1 AND user_id = $2 AND revoked_at IS NULL
        ",
    )
    .bind(organization_id)
    .bind(target_user_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        r"
        UPDATE organization_wrapped_keys
        SET revoked_at = to_timestamp($3::double precision)
        WHERE organization_id = $1 AND recipient_user_id = $2 AND revoked_at IS NULL
        ",
    )
    .bind(organization_id)
    .bind(target_user_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(())
}

pub async fn transfer_ownership_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    current_owner_user_id: Uuid,
    new_owner_user_id: Uuid,
    now_secs: i64,
) -> Result<Organization, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org = lookup_org_and_lock(&mut tx, organization_id, current_owner_user_id, false).await?;
    let actor_role =
        lookup_active_actor_role(&mut tx, organization_id, current_owner_user_id).await?;
    if !actor_role.is_owner() || org.owner_user_id != current_owner_user_id {
        return Err(AuthError::OrgPermissionDenied);
    }
    let _target_role = lookup_active_actor_role(&mut tx, organization_id, new_owner_user_id)
        .await
        .map_err(|_| AuthError::InvalidRequest)?;

    let update_sql = format!(
        r"
        UPDATE organizations
        SET owner_user_id = $2, updated_at = to_timestamp($3::double precision)
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING {ORG_SELECT_COLUMNS}
        "
    );
    let updated_row = sqlx::query(&update_sql)
        .bind(organization_id)
        .bind(new_owner_user_id)
        .bind(now_secs as f64)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        "UPDATE organization_members SET role = 'admin', updated_at = to_timestamp($3::double precision) WHERE organization_id = $1 AND user_id = $2 AND removed_at IS NULL",
    )
    .bind(organization_id)
    .bind(current_owner_user_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        "UPDATE organization_members SET role = 'owner', updated_at = to_timestamp($3::double precision) WHERE organization_id = $1 AND user_id = $2 AND removed_at IS NULL",
    )
    .bind(organization_id)
    .bind(new_owner_user_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query("UPDATE memberships SET role = 'admin' WHERE workspace_id = $1 AND user_id = $2")
        .bind(org.workspace_id)
        .bind(current_owner_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    sqlx::query("UPDATE memberships SET role = 'owner' WHERE workspace_id = $1 AND user_id = $2")
        .bind(org.workspace_id)
        .bind(new_owner_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    row_to_organization(&updated_row)
}

pub async fn delete_organization_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
    now_secs: i64,
) -> Result<(), AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let actor_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;
    if !actor_role.is_owner() || org.owner_user_id != actor_user_id {
        return Err(AuthError::OrgPermissionDenied);
    }

    sqlx::query(
        "UPDATE organizations SET deleted_at = to_timestamp($2::double precision), updated_at = to_timestamp($2::double precision) WHERE id = $1",
    )
    .bind(organization_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        r"
        UPDATE organization_invitations
        SET revoked_at = to_timestamp($2::double precision)
        WHERE organization_id = $1
          AND accepted_at IS NULL
          AND declined_at IS NULL
          AND revoked_at IS NULL
        ",
    )
    .bind(organization_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        "UPDATE organization_member_public_keys SET revoked_at = to_timestamp($2::double precision) WHERE organization_id = $1 AND revoked_at IS NULL",
    )
    .bind(organization_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        "UPDATE organization_wrapped_keys SET revoked_at = to_timestamp($2::double precision) WHERE organization_id = $1 AND revoked_at IS NULL",
    )
    .bind(organization_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(())
}

pub async fn list_all_orgs_pg(
    pg: &PgControlPlaneStore,
) -> Result<Vec<(Organization, u32)>, AuthError> {
    let mut tx = pg
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
        SELECT o.id, o.workspace_id, o.billing_workspace_id, o.name, o.purchased_seats,
               o.billing_cycle, o.owner_user_id, o.active_key_epoch,
               EXTRACT(EPOCH FROM o.created_at)::bigint AS created_at_secs,
               EXTRACT(EPOCH FROM o.deleted_at)::bigint AS deleted_at_secs,
               (SELECT count(*) FROM organization_members om WHERE om.organization_id = o.id AND om.removed_at IS NULL) AS active_count
        FROM organizations o
        WHERE o.deleted_at IS NULL
        ORDER BY o.created_at DESC
        ",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let count_i64: i64 = r.try_get("active_count").unwrap_or(0);
        out.push((
            row_to_organization(&r)?,
            u32::try_from(count_i64).unwrap_or(0),
        ));
    }
    Ok(out)
}

pub async fn list_members_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
) -> Result<Vec<OrganizationMember>, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    let org_active: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM organizations WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(organization_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;
    if !org_active {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r"
        SELECT om.organization_id, om.workspace_id, om.user_id, om.role,
               EXTRACT(EPOCH FROM om.joined_at)::bigint AS joined_at_secs,
               EXTRACT(EPOCH FROM om.removed_at)::bigint AS removed_at_secs,
               u.email AS email, u.display_name AS display_name
        FROM organization_members om
        LEFT JOIN users u ON u.id = om.user_id
        WHERE om.organization_id = $1 AND om.removed_at IS NULL
        ORDER BY om.joined_at ASC
        ",
    )
    .bind(organization_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    rows.iter().map(row_to_member).collect()
}

#[allow(clippy::too_many_arguments)]
pub async fn register_device_public_key_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    user_id: Uuid,
    device_id: &str,
    device_label: &str,
    x25519_public_key: &[u8],
    key_fingerprint: &str,
    now_secs: i64,
) -> Result<MemberDevicePublicKey, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org = lookup_org_and_lock(&mut tx, organization_id, user_id, false).await?;
    let _role = lookup_active_actor_role(&mut tx, organization_id, user_id).await?;

    let row = sqlx::query(
        r"
        INSERT INTO organization_member_public_keys (
            organization_id, workspace_id, user_id, device_id, device_label,
            x25519_public_key, key_fingerprint, created_at, revoked_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision), NULL)
        ON CONFLICT (organization_id, user_id, device_id) DO UPDATE
        SET device_label = EXCLUDED.device_label,
            x25519_public_key = EXCLUDED.x25519_public_key,
            key_fingerprint = EXCLUDED.key_fingerprint,
            created_at = EXCLUDED.created_at,
            revoked_at = NULL
        RETURNING id, EXTRACT(EPOCH FROM created_at)::bigint AS created_at_secs
        ",
    )
    .bind(organization_id)
    .bind(org.workspace_id)
    .bind(user_id)
    .bind(device_id)
    .bind(device_label)
    .bind(x25519_public_key)
    .bind(key_fingerprint)
    .bind(now_secs as f64)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let crt_secs: i64 = row
        .try_get("created_at_secs")
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(MemberDevicePublicKey {
        id: row
            .try_get("id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        organization_id,
        workspace_id: org.workspace_id,
        user_id,
        device_id: device_id.to_owned(),
        device_label: device_label.to_owned(),
        x25519_public_key_b64: base64::engine::general_purpose::STANDARD.encode(x25519_public_key),
        key_fingerprint: key_fingerprint.to_owned(),
        created_at: crt_secs,
        revoked_at: None,
    })
}

pub async fn revoke_device_public_key_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
    target_user_id: Uuid,
    device_id: &str,
    now_secs: i64,
) -> Result<(), AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let _org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let actor_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;
    if actor_user_id != target_user_id {
        let target_role = lookup_active_actor_role(&mut tx, organization_id, target_user_id)
            .await
            .map_err(|_| AuthError::InvalidRequest)?;
        if !actor_role.can_remove_target(actor_user_id, target_user_id, target_role) {
            return Err(AuthError::OrgPermissionDenied);
        }
    }
    let updated = sqlx::query(
        r"
        UPDATE organization_member_public_keys
        SET revoked_at = to_timestamp($4::double precision)
        WHERE organization_id = $1 AND user_id = $2 AND device_id = $3 AND revoked_at IS NULL
        ",
    )
    .bind(organization_id)
    .bind(target_user_id)
    .bind(device_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;
    if updated.rows_affected() == 0 {
        return Err(AuthError::InvalidRequest);
    }

    sqlx::query(
        r"
        UPDATE organization_wrapped_keys
        SET revoked_at = to_timestamp($4::double precision)
        WHERE organization_id = $1
          AND recipient_user_id = $2
          AND recipient_device_id = $3
          AND revoked_at IS NULL
        ",
    )
    .bind(organization_id)
    .bind(target_user_id)
    .bind(device_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(())
}

pub async fn list_active_member_device_keys_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
) -> Result<Vec<MemberDevicePublicKey>, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let _org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let _role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;

    let rows = sqlx::query(
        r"
        SELECT dk.id, dk.organization_id, dk.workspace_id, dk.user_id,
               dk.device_id, dk.device_label, dk.x25519_public_key,
               dk.key_fingerprint,
               EXTRACT(EPOCH FROM dk.created_at)::bigint AS created_at_secs
        FROM organization_member_public_keys dk
        INNER JOIN organization_members om
            ON om.organization_id = dk.organization_id
           AND om.user_id = dk.user_id
           AND om.removed_at IS NULL
        WHERE dk.organization_id = $1
          AND dk.revoked_at IS NULL
        ORDER BY dk.created_at ASC
        ",
    )
    .bind(organization_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let pk: Vec<u8> = r
            .try_get("x25519_public_key")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let crt_secs: i64 = r
            .try_get("created_at_secs")
            .map_err(|_| AuthError::BackendUnavailable)?;
        out.push(MemberDevicePublicKey {
            id: r.try_get("id").map_err(|_| AuthError::BackendUnavailable)?,
            organization_id: r
                .try_get("organization_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            workspace_id: r
                .try_get("workspace_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            user_id: r
                .try_get("user_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            device_id: r
                .try_get("device_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            device_label: r
                .try_get("device_label")
                .map_err(|_| AuthError::BackendUnavailable)?,
            x25519_public_key_b64: b64.encode(&pk),
            key_fingerprint: r
                .try_get("key_fingerprint")
                .map_err(|_| AuthError::BackendUnavailable)?,
            created_at: crt_secs,
            revoked_at: None,
        });
    }
    Ok(out)
}

async fn insert_wrapped_keys_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    workspace_id: Uuid,
    actor_user_id: Uuid,
    key_epoch: u32,
    validated: &[ValidatedWrappedKeyBytes],
    now_secs: i64,
) -> Result<(), AuthError> {
    let epoch_i32 = i32::try_from(key_epoch).map_err(|_| AuthError::InvalidRequest)?;
    for v in validated {
        let authorized_device: bool = sqlx::query_scalar(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM organization_members om
                INNER JOIN organization_member_public_keys dk
                    ON dk.organization_id = om.organization_id
                   AND dk.user_id = om.user_id
                   AND dk.device_id = $3
                   AND dk.revoked_at IS NULL
                WHERE om.organization_id = $1
                  AND om.user_id = $2
                  AND om.removed_at IS NULL
            )
            ",
        )
        .bind(organization_id)
        .bind(v.recipient_user_id)
        .bind(&v.recipient_device_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        if !authorized_device {
            return Err(AuthError::OrgPermissionDenied);
        }

        sqlx::query(
            r"
            INSERT INTO organization_wrapped_keys (
                organization_id, workspace_id, key_epoch,
                recipient_user_id, recipient_device_id, sender_user_id,
                sender_ephemeral_pub, wrapped_key_ciphertext, wrapped_key_nonce,
                created_at, revoked_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::double precision), NULL)
            ON CONFLICT (organization_id, key_epoch, recipient_user_id, recipient_device_id) DO UPDATE
            SET sender_user_id = EXCLUDED.sender_user_id,
                sender_ephemeral_pub = EXCLUDED.sender_ephemeral_pub,
                wrapped_key_ciphertext = EXCLUDED.wrapped_key_ciphertext,
                wrapped_key_nonce = EXCLUDED.wrapped_key_nonce,
                created_at = EXCLUDED.created_at,
                revoked_at = NULL
            ",
        )
        .bind(organization_id)
        .bind(workspace_id)
        .bind(epoch_i32)
        .bind(v.recipient_user_id)
        .bind(&v.recipient_device_id)
        .bind(actor_user_id)
        .bind(&v.sender_ephemeral_pub)
        .bind(&v.wrapped_key_ciphertext)
        .bind(&v.wrapped_key_nonce)
        .bind(now_secs as f64)
        .execute(&mut **tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    }
    Ok(())
}

pub async fn publish_wrapped_workspace_keys_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
    key_epoch: u32,
    validated: &[ValidatedWrappedKeyBytes],
    now_secs: i64,
) -> Result<usize, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let actor_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;
    if !actor_role.can_modify_shared_resources() {
        return Err(AuthError::OrgPermissionDenied);
    }
    if key_epoch > org.active_key_epoch {
        return Err(AuthError::SyncVersionConflict);
    }
    insert_wrapped_keys_in_tx(
        &mut tx,
        organization_id,
        org.workspace_id,
        actor_user_id,
        key_epoch,
        validated,
        now_secs,
    )
    .await?;
    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(validated.len())
}

pub async fn rotate_workspace_key_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    actor_user_id: Uuid,
    expected_current_epoch: u32,
    validated: &[ValidatedWrappedKeyBytes],
    now_secs: i64,
) -> Result<u32, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let org = lookup_org_and_lock(&mut tx, organization_id, actor_user_id, false).await?;
    let actor_role = lookup_active_actor_role(&mut tx, organization_id, actor_user_id).await?;
    if !actor_role.can_manage_members() {
        return Err(AuthError::OrgPermissionDenied);
    }
    if org.active_key_epoch != expected_current_epoch {
        return Err(AuthError::SyncVersionConflict);
    }
    let new_epoch = expected_current_epoch + 1;
    let new_epoch_i32 = i32::try_from(new_epoch).map_err(|_| AuthError::InvalidRequest)?;

    sqlx::query(
        "UPDATE organizations SET active_key_epoch = $2, updated_at = to_timestamp($3::double precision) WHERE id = $1",
    )
    .bind(organization_id)
    .bind(new_epoch_i32)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    insert_wrapped_keys_in_tx(
        &mut tx,
        organization_id,
        org.workspace_id,
        actor_user_id,
        new_epoch,
        validated,
        now_secs,
    )
    .await?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(new_epoch)
}

pub async fn list_my_wrapped_keys_pg(
    pg: &PgControlPlaneStore,
    organization_id: Uuid,
    user_id: Uuid,
    device_id: Option<&str>,
) -> Result<Vec<WrappedWorkspaceKeyRecord>, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let _org = lookup_org_and_lock(&mut tx, organization_id, user_id, false).await?;
    let _role = lookup_active_actor_role(&mut tx, organization_id, user_id).await?;
    let dev_filter = device_id.map(str::trim).filter(|s| !s.is_empty());

    let rows = match dev_filter {
        Some(dev) => sqlx::query(
            r"
            SELECT id, organization_id, workspace_id, key_epoch,
                   recipient_user_id, recipient_device_id, sender_user_id,
                   sender_ephemeral_pub, wrapped_key_ciphertext, wrapped_key_nonce,
                   EXTRACT(EPOCH FROM created_at)::bigint AS created_at_secs
            FROM organization_wrapped_keys
            WHERE organization_id = $1
              AND recipient_user_id = $2
              AND recipient_device_id = $3
              AND revoked_at IS NULL
            ORDER BY key_epoch ASC
            ",
        )
        .bind(organization_id)
        .bind(user_id)
        .bind(dev)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?,
        None => sqlx::query(
            r"
            SELECT id, organization_id, workspace_id, key_epoch,
                   recipient_user_id, recipient_device_id, sender_user_id,
                   sender_ephemeral_pub, wrapped_key_ciphertext, wrapped_key_nonce,
                   EXTRACT(EPOCH FROM created_at)::bigint AS created_at_secs
            FROM organization_wrapped_keys
            WHERE organization_id = $1
              AND recipient_user_id = $2
              AND revoked_at IS NULL
            ORDER BY key_epoch ASC
            ",
        )
        .bind(organization_id)
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?,
    };

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let epoch_i32: i32 = r
            .try_get("key_epoch")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let eph: Vec<u8> = r
            .try_get("sender_ephemeral_pub")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let ct: Vec<u8> = r
            .try_get("wrapped_key_ciphertext")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let nonce: Vec<u8> = r
            .try_get("wrapped_key_nonce")
            .map_err(|_| AuthError::BackendUnavailable)?;
        let crt_secs: i64 = r
            .try_get("created_at_secs")
            .map_err(|_| AuthError::BackendUnavailable)?;
        out.push(WrappedWorkspaceKeyRecord {
            id: r.try_get("id").map_err(|_| AuthError::BackendUnavailable)?,
            organization_id: r
                .try_get("organization_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            workspace_id: r
                .try_get("workspace_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            key_epoch: u32::try_from(epoch_i32).unwrap_or(1),
            recipient_user_id: r
                .try_get("recipient_user_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            recipient_device_id: r
                .try_get("recipient_device_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            sender_user_id: r
                .try_get("sender_user_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            sender_ephemeral_pub_b64: b64.encode(&eph),
            wrapped_key_ciphertext_b64: b64.encode(&ct),
            wrapped_key_nonce_b64: b64.encode(&nonce),
            created_at: crt_secs,
        });
    }
    Ok(out)
}
