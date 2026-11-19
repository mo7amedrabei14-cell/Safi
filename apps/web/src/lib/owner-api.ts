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
  readonly suspended_users: number;
  readonly maintenance_mode: boolean;
  readonly billing_providers: {
    readonly paddle: string;
    readonly paymob: string;
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

const BUILD_API_ORIGIN = typeof __SAFI_API_BASE_URL__ === "string" ? __SAFI_API_BASE_URL__ : "";

function apiOrigin(): string {
  const desktop =
    import.meta.env.PROD && typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  return resolveAuthApiBaseUrl(BUILD_API_ORIGIN, desktop);
}

async function ownerRequest<T>(
  path: string,
  method: "GET" | "POST",
  sessionToken: string,
  body?: unknown
): Promise<T> {
  const headers = new Headers({
    Accept: "application/json",
    Authorization: `Bearer ${sessionToken}`
  });
  if (body !== undefined) {
    headers.set("Content-Type", "application/json");
  }

  let response: Response;
  try {
    response = await fetch(`${apiOrigin()}/v1/owner${path}`, {
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

  return (await response.json()) as T;
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
    ownerRequest<OwnerAuditLogsResponse>("/audit-logs", "GET", token)
};
