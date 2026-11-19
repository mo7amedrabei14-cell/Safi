/**
 * Real HTTP client for the SafiData Owner Control Plane (`/v1/owner/*`).
 *
 * Every call carries the authenticated Bearer session token and receives real
 * backend state. High-risk actions require a fresh server-verified RFC 6238
 * Google Authenticator TOTP step-up verification (`SF-AUTH-016`).
 */
import { resolveAuthApiBaseUrl, SafiAuthApiError } from "./auth-api";

export interface OwnerMfaStatus {
  readonly owner_bootstrapped: boolean;
  readonly is_owner: boolean;
  readonly totp_enrolled: boolean;
  readonly totp_confirmed: boolean;
  readonly elevated_session_active: boolean;
  readonly elevated_expires_at: number | null;
  readonly step_up_fresh: boolean;
  readonly recovery_codes_remaining: number;
  readonly locked_until: number | null;
}

export interface TotpEnrollmentBundle {
  readonly otpauth_uri: string;
  readonly manual_secret_base32: string;
  readonly issuer: string;
  readonly account_label: string;
  readonly period_secs: number;
  readonly digits: number;
  readonly recovery_codes: readonly string[];
}

export interface ElevatedOwnerSession {
  readonly session_id: string;
  readonly user_id: string;
  readonly verified_at: number;
  readonly expires_at: number;
  readonly last_step_up_at: number;
}

export interface OwnerStatsResponse {
  readonly total_users: number;
  readonly verified_phone_users?: number;
  readonly suspended_users: number;
  readonly active_devices?: number;
  readonly total_coupons?: number;
  readonly total_coupon_redemptions?: number;
  readonly open_support_tickets?: number;
  readonly organizations_count?: number;
  readonly maintenance_mode: boolean;
  readonly billing_providers: {
    readonly paddle: string;
    readonly paymob: string;
  };
  readonly revenue?: {
    readonly total_invoices: number;
    readonly paid_invoices: number;
    readonly refunded_invoices: number;
    readonly chargeback_invoices: number;
    readonly gross_usd_minor: number;
    readonly gross_egp_minor: number;
    readonly refunded_usd_minor: number;
    readonly refunded_egp_minor: number;
    readonly chargeback_usd_minor: number;
    readonly chargeback_egp_minor: number;
  };
  readonly entitlements: {
    readonly active_trials: number;
    readonly free_users: number;
    readonly monthly_subscribers: number;
    readonly annual_subscribers: number;
    readonly organization_seats: number;
    readonly friend_vip_users: number;
    readonly credit_pack_holders: number;
    readonly pay_per_file_holders: number;
    readonly mrr_usd: number;
    readonly mrr_egp: number;
    readonly arr_usd: number;
    readonly arr_egp: number;
    readonly rows_cleaned_total: number;
    readonly pii_masked_total: number;
    readonly ai_calls_used_total: number;
  };
}

export interface OwnerCouponRecord {
  readonly id: string;
  readonly code: string;
  readonly discount_kind: "percent" | "fixed_usd" | "fixed_egp";
  readonly discount_value: number;
  readonly enabled: boolean;
  readonly archived: boolean;
  readonly expires_at: number | null;
  readonly max_redemptions: number | null;
  readonly redemptions_count: number;
}

export interface OwnerDeviceRecord {
  readonly device_id: string;
  readonly user_id: string;
  readonly device_name: string;
  readonly status: "active" | "replaced_by_new_device" | "revoked_by_owner" | "signed_out";
  readonly session_generation: number;
  readonly last_seen_at: number;
}

export interface OwnerDeviceReplacementEvent {
  readonly id: string;
  readonly user_id: string;
  readonly trigger_kind: string;
  readonly reason: string;
  readonly new_session_generation: number;
  readonly created_at: number;
}

export interface OwnerDevicesOverviewResponse {
  readonly devices: readonly OwnerDeviceRecord[];
  readonly replacements: readonly OwnerDeviceReplacementEvent[];
}

export interface OwnerOrganizationRow {
  readonly organization: {
    readonly id: string;
    readonly name: string;
    readonly purchased_seats: number;
    readonly owner_user_id: string;
  };
  readonly active_members: number;
}

export interface OwnerSupportTicket {
  readonly id: string;
  readonly user_id: string;
  readonly subject: string;
  readonly body: string;
  readonly status: string;
  readonly priority: string;
  readonly owner_notes: string;
}

export interface OwnerAnnouncement {
  readonly id: string;
  readonly title: string;
  readonly body: string;
  readonly severity: string;
  readonly published: boolean;
  readonly created_at: number;
}

export interface OwnerCustomerProfileResponse {
  readonly account: OwnerAccountView["account"];
  readonly entitlements: OwnerAccountView["entitlements"];
  readonly invoices: readonly {
    readonly external_invoice_id: string;
    readonly amount_minor: number;
    readonly currency: string;
    readonly status: string;
  }[];
  readonly devices: readonly OwnerDeviceRecord[];
  readonly device_replacements: readonly OwnerDeviceReplacementEvent[];
}

export interface OwnerAccountView {
  readonly account: {
    readonly user_id: string;
    readonly workspace_id: string;
    readonly email: string;
    readonly display_name: string;
    readonly verified_phone: string | null;
    readonly suspended: boolean;
    readonly created_at: number;
  };
  readonly entitlements: {
    readonly effective_plan: string;
    readonly state: string;
    readonly ai_calls_used: number;
    readonly ai_calls_limit: number;
    readonly operation_credits: number;
    readonly pay_per_file_tokens: number;
    readonly trial_expires_at: number | null;
  };
}

export interface OwnerBillingEvent {
  readonly id: string;
  readonly provider: string;
  readonly external_event_id: string;
  readonly event_type: string;
  readonly workspace_id: string | null;
  readonly occurred_at: number;
  readonly processed_at: number;
  readonly outcome: string;
}

export interface OwnerFeatureFlag {
  readonly key: string;
  readonly enabled: boolean;
  readonly description: string;
  readonly updated_at: number;
}

export interface OwnerAuditLogEntry {
  readonly id: string;
  readonly seq: number;
  readonly timestamp: string;
  readonly actor_user_id: string | null;
  readonly action: string;
  readonly target_type: string;
  readonly target_id: string;
  readonly outcome: "allowed" | "denied" | "error";
  readonly previous_hash: string;
  readonly current_hash: string;
}

export interface OwnerAuditLogsResponse {
  readonly entries: readonly OwnerAuditLogEntry[];
  readonly is_chain_valid: boolean;
}

export interface OwnerWebLoginResponse {
  readonly token: string;
  readonly session_id: string;
  readonly user_id: string;
  readonly workspace_id: string;
  readonly email: string;
  readonly is_owner: boolean;
  readonly expires_at: number;
}

export interface OwnerOfficialTemplate {
  readonly id: string;
  readonly slug: string;
  readonly name: string;
  readonly description: string;
  readonly category: string;
  readonly current_version: number;
  readonly active: boolean;
  readonly updated_at: number;
  readonly versions: readonly {
    readonly version_number: number;
    readonly release_notes: string;
    readonly content_hash: string;
    readonly published_at: number;
  }[];
}

export interface OwnerCommunitySubmission {
  readonly id: string;
  readonly submitter_user_id: string;
  readonly name: string;
  readonly description: string;
  readonly recipe_type: string;
  readonly status:
    | "pending"
    | "under_review"
    | "approved"
    | "rejected"
    | "changes_requested"
    | "published";
  readonly pii_warnings: readonly string[];
  readonly reviewer_notes: string;
  readonly published_template_id: string | null;
  readonly created_at: number;
  readonly updated_at: number;
}

export interface OwnerAppRelease {
  readonly id: string;
  readonly version_code: number;
  readonly version_name: string;
  readonly channel: string;
  readonly platform: string;
  readonly download_url: string;
  readonly file_size_bytes: number;
  readonly sha256_hex: string;
  readonly release_notes: string;
  readonly active: boolean;
  readonly created_at: number;
}

export interface DesktopPkceApproveResponse {
  readonly code: string;
  readonly state: string;
  readonly callback_uri: string;
}

const BUILD_API_ORIGIN = typeof __SAFI_API_BASE_URL__ === "string" ? __SAFI_API_BASE_URL__ : "";

function apiOrigin(): string {
  const desktop =
    import.meta.env.PROD && typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  return resolveAuthApiBaseUrl(BUILD_API_ORIGIN, desktop);
}

async function rawJsonRequest<T>(
  urlPath: string,
  method: "GET" | "POST" | "DELETE",
  token?: string | null,
  body?: unknown
): Promise<T> {
  const headers = new Headers({ Accept: "application/json" });
  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }
  if (body !== undefined) {
    headers.set("Content-Type", "application/json");
  }

  let response: Response;
  try {
    response = await fetch(`${apiOrigin()}${urlPath}`, {
      method,
      headers,
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      cache: "no-store",
      credentials: "omit",
      referrerPolicy: "no-referrer",
      redirect: "error"
    });
  } catch {
    throw new SafiAuthApiError(0, null);
  }

  if (!response.ok) {
    let code: string | null = null;
    try {
      const problem: unknown = await response.json();
      if (
        typeof problem === "object" &&
        problem !== null &&
        "code" in problem &&
        typeof problem.code === "string"
      ) {
        code = problem.code;
      }
    } catch {
      // Never expose untrusted error bodies.
    }
    throw new SafiAuthApiError(response.status, code);
  }

  if (response.status === 204) {
    return undefined as T;
  }
  return (await response.json()) as T;
}

async function ownerRequest<T>(
  path: string,
  method: "GET" | "POST" | "DELETE",
  sessionToken: string,
  body?: unknown
): Promise<T> {
  return rawJsonRequest<T>(`/v1/owner${path}`, method, sessionToken, body);
}

export const ownerApi = {
  getMfaStatus: (token: string) => ownerRequest<OwnerMfaStatus>("/mfa/status", "GET", token),
  enrollTotp: (token: string) => ownerRequest<TotpEnrollmentBundle>("/mfa/enroll", "POST", token),
  confirmTotp: (token: string, code: string) =>
    ownerRequest<ElevatedOwnerSession>("/mfa/confirm", "POST", token, { code }),
  verifyTotp: (token: string, code: string) =>
    ownerRequest<ElevatedOwnerSession>("/mfa/verify", "POST", token, { code }),
  stepUpTotp: (token: string, code: string) =>
    ownerRequest<ElevatedOwnerSession>("/mfa/step-up", "POST", token, { code }),
  recoverWithCode: (token: string, recoveryCode: string) =>
    ownerRequest<ElevatedOwnerSession>("/mfa/recover", "POST", token, {
      recovery_code: recoveryCode
    }),
  getStats: (token: string) => ownerRequest<OwnerStatsResponse>("/stats", "GET", token),
  listAccounts: (token: string, query = "") =>
    ownerRequest<readonly OwnerAccountView[]>(
      query.trim().length > 0 ? `/accounts?q=${encodeURIComponent(query.trim())}` : "/accounts",
      "GET",
      token
    ),
  grantFriendVip: (token: string, userId: string, workspaceId: string, reason: string) =>
    ownerRequest<unknown>("/friends/grant", "POST", token, {
      user_id: userId,
      workspace_id: workspaceId,
      reason
    }),
  revokeFriendVip: (token: string, userId: string, workspaceId: string) =>
    ownerRequest<unknown>("/friends/revoke", "POST", token, {
      user_id: userId,
      workspace_id: workspaceId
    }),
  suspendAccount: (token: string, userId: string) =>
    ownerRequest<unknown>(`/accounts/${encodeURIComponent(userId)}/suspend`, "POST", token),
  reactivateAccount: (token: string, userId: string) =>
    ownerRequest<unknown>(`/accounts/${encodeURIComponent(userId)}/reactivate`, "POST", token),
  extendTrial: (token: string, workspaceId: string, userId: string, extraDays: number) =>
    ownerRequest<unknown>(
      `/subscriptions/${encodeURIComponent(workspaceId)}/extend-trial`,
      "POST",
      token,
      { user_id: userId, extra_days: extraDays }
    ),
  cancelSubscription: (token: string, workspaceId: string, userId: string) =>
    ownerRequest<unknown>(
      `/subscriptions/${encodeURIComponent(workspaceId)}/cancel`,
      "POST",
      token,
      { user_id: userId }
    ),
  issueRefund: (token: string, externalInvoiceId: string, mandatoryLegalOverride = false) =>
    ownerRequest<unknown>("/billing/refund", "POST", token, {
      external_invoice_id: externalInvoiceId,
      mandatory_legal_override: mandatoryLegalOverride
    }),
  listBillingEvents: (token: string) =>
    ownerRequest<readonly OwnerBillingEvent[]>("/billing/events", "GET", token),
  setMaintenance: (token: string, enabled: boolean, reason: string) =>
    ownerRequest<{ readonly enabled: boolean; readonly reason: string | null }>(
      "/system/maintenance",
      "POST",
      token,
      { enabled, reason }
    ),
  listFeatureFlags: (token: string) =>
    ownerRequest<readonly OwnerFeatureFlag[]>("/feature-flags", "GET", token),
  setFeatureFlag: (token: string, key: string, enabled: boolean, description: string) =>
    ownerRequest<OwnerFeatureFlag>("/feature-flags", "POST", token, {
      key,
      enabled,
      description
    }),
  listAuditLogs: (token: string) =>
    ownerRequest<OwnerAuditLogsResponse>("/audit-logs", "GET", token),
  getCustomerProfile: (token: string, userId: string) =>
    ownerRequest<OwnerCustomerProfileResponse>(
      `/accounts/${encodeURIComponent(userId)}/profile`,
      "GET",
      token
    ),
  listDevices: (token: string) =>
    ownerRequest<OwnerDevicesOverviewResponse>("/devices", "GET", token),
  resetAccountDevices: (token: string, userId: string, reason: string) =>
    ownerRequest<OwnerDeviceReplacementEvent>(
      `/accounts/${encodeURIComponent(userId)}/devices/reset`,
      "POST",
      token,
      { reason }
    ),
  recordChargeback: (
    token: string,
    externalInvoiceId: string,
    workspaceId: string,
    userId: string
  ) =>
    ownerRequest<unknown>("/billing/chargeback", "POST", token, {
      external_invoice_id: externalInvoiceId,
      workspace_id: workspaceId,
      user_id: userId
    }),
  listCoupons: (token: string) =>
    ownerRequest<readonly OwnerCouponRecord[]>("/coupons", "GET", token),
  createCoupon: (
    token: string,
    payload: {
      readonly code: string;
      readonly discount_kind: "percent" | "fixed_usd" | "fixed_egp";
      readonly discount_value: number;
      readonly max_redemptions?: number;
      readonly expires_at?: number;
    }
  ) => ownerRequest<OwnerCouponRecord>("/coupons", "POST", token, payload),
  updateCoupon: (
    token: string,
    code: string,
    payload: { readonly enabled?: boolean; readonly archived?: boolean }
  ) =>
    ownerRequest<OwnerCouponRecord>(`/coupons/${encodeURIComponent(code)}`, "POST", token, payload),
  listOrganizations: (token: string) =>
    ownerRequest<readonly OwnerOrganizationRow[]>("/organizations", "GET", token),
  listSupportTickets: (token: string) =>
    ownerRequest<readonly OwnerSupportTicket[]>("/support/tickets", "GET", token),
  updateSupportTicket: (
    token: string,
    id: string,
    payload: { readonly status?: string; readonly owner_notes?: string }
  ) =>
    ownerRequest<OwnerSupportTicket>(
      `/support/tickets/${encodeURIComponent(id)}`,
      "POST",
      token,
      payload
    ),
  listAnnouncements: (token: string) =>
    ownerRequest<readonly OwnerAnnouncement[]>("/announcements", "GET", token),
  createAnnouncement: (token: string, title: string, body: string, severity = "info") =>
    ownerRequest<OwnerAnnouncement>("/announcements", "POST", token, {
      title,
      body,
      severity,
      published: true
    }),
  getProductConfig: (token: string) =>
    ownerRequest<Record<string, unknown>>("/product-config", "GET", token),
  ownerWebLogin: (email: string, password: string) =>
    rawJsonRequest<OwnerWebLoginResponse>("/v1/owner/web/login", "POST", null, {
      email,
      password
    }),
  ownerWebLogout: (token: string) => ownerRequest<undefined>("/web/session", "DELETE", token),
  listCommunitySubmissions: (token: string) =>
    ownerRequest<readonly OwnerCommunitySubmission[]>("/community/submissions", "GET", token),
  reviewCommunitySubmission: (
    token: string,
    id: string,
    status: "under_review" | "approved" | "rejected" | "changes_requested",
    reviewerNotes: string
  ) =>
    ownerRequest<OwnerCommunitySubmission>(
      `/community/submissions/${encodeURIComponent(id)}/review`,
      "POST",
      token,
      { status, reviewer_notes: reviewerNotes }
    ),
  publishCommunitySubmission: (
    token: string,
    id: string,
    payload: {
      readonly slug: string;
      readonly official_name: string;
      readonly official_description: string;
      readonly category?: string;
      readonly release_notes?: string;
    }
  ) =>
    ownerRequest<{
      readonly template: OwnerOfficialTemplate;
      readonly submission: OwnerCommunitySubmission;
    }>(`/community/submissions/${encodeURIComponent(id)}/publish`, "POST", token, payload),
  listOfficialTemplates: (token: string) =>
    ownerRequest<readonly OwnerOfficialTemplate[]>("/templates/official", "GET", token),
  setOfficialTemplateStatus: (token: string, templateId: string, active: boolean) =>
    ownerRequest<OwnerOfficialTemplate>(
      `/templates/official/${encodeURIComponent(templateId)}/status`,
      "POST",
      token,
      { active }
    ),
  rollbackOfficialTemplate: (token: string, templateId: string, versionNumber: number) =>
    ownerRequest<OwnerOfficialTemplate>(
      `/templates/official/${encodeURIComponent(templateId)}/rollback`,
      "POST",
      token,
      { version_number: versionNumber }
    ),
  listReleases: (token: string) =>
    ownerRequest<readonly OwnerAppRelease[]>("/releases", "GET", token),
  publishRelease: (
    token: string,
    payload: {
      readonly version_code: number;
      readonly version_name: string;
      readonly channel: string;
      readonly platform: string;
      readonly download_url: string;
      readonly file_size_bytes: number;
      readonly sha256_hex: string;
      readonly release_notes: string;
    }
  ) => ownerRequest<OwnerAppRelease>("/releases", "POST", token, payload),
  approveDesktopPkceAuth: (payload: {
    readonly request_id: string;
    readonly state: string;
    readonly email: string;
    readonly password: string;
  }) =>
    rawJsonRequest<DesktopPkceApproveResponse>("/v1/auth/desktop/approve", "POST", null, payload)
};
