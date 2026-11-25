/**
 * `apps/web/src/lib/organization-api.ts`
 *
 * Production HTTP client & Desktop E2EE Sync orchestrator for:
 *  - Organizations & Shared Team Workspaces (`/v1/organizations/*`)
 *  - 4-Role Server RBAC (`owner`, `admin`, `member`, `viewer`)
 *  - Verified-Email Single-Use Invitations (`accept`, `decline`, `revoke`, `list`)
 *  - X25519 Device Public Key Directory & Wrapped Workspace Key Epochs (`/keys/*`)
 *  - Zero-Knowledge Encrypted Sync (`/v1/sync/*` + `X-Safi-Workspace-Id` + native Rust `OrgSyncStore`)
 *
 * Strictly uses `getDefaultAuthApiBaseUrl()` (`SAFI_API_BASE_URL`) and NEVER leaks raw keys or
 * plaintext customer data over HTTP.
 */
import {
  applyDesktopPulledEnvelopes,
  completeDesktopOutboundSyncEnvelope,
  getDesktopSyncStatus,
  ingestDesktopWrappedKeys,
  peekDesktopOutboundSyncEnvelope,
  prepareDesktopWorkspaceKeyRotation,
  selectDesktopSyncWorkspace,
  wrapDesktopEpochKeysForDevices,
  type DesktopOutboundEncryptedEnvelope,
  type DesktopRemoteMemberDeviceKey,
  type DesktopRemoteWrappedKeyEnvelope,
  type DesktopWorkspaceSyncStatus,
  type DesktopWrappedKeyWireEntry
} from "../desktop/studio-bridge";
import { getDefaultAuthApiBaseUrl } from "./auth-api";

export type OrgRole = "owner" | "admin" | "member" | "viewer";
export type InvitationStatus = "pending" | "accepted" | "declined" | "revoked" | "expired";

export interface OrganizationRecord {
  readonly id: string;
  readonly workspace_id: string;
  readonly billing_workspace_id?: string | null;
  readonly name: string;
  readonly purchased_seats: number;
  readonly billing_cycle: string;
  readonly owner_user_id: string;
  readonly active_key_epoch: number;
  readonly created_at: number;
}

export interface EnrichedOrganizationSummary {
  readonly organization: OrganizationRecord;
  readonly my_role: OrgRole;
  readonly active_members_count: number;
  readonly pending_invitations_count: number;
  readonly paid_seats: number;
  readonly teams_enabled: boolean;
  readonly subscription_state: string;
  readonly effective_plan: string;
}

export interface PendingUserInvitation {
  readonly invitation_id: string;
  readonly organization_id: string;
  readonly organization_name: string;
  readonly workspace_id: string;
  readonly invited_email: string;
  readonly role: OrgRole;
  readonly expires_at: number;
  readonly created_at: number;
}

export interface MyOrganizationsResponse {
  readonly personal_workspace_id: string;
  readonly verified_email: string | null;
  readonly organizations: readonly EnrichedOrganizationSummary[];
  readonly pending_invitations: readonly PendingUserInvitation[];
}

export interface OrganizationMemberRecord {
  readonly organization_id: string;
  readonly workspace_id: string;
  readonly user_id: string;
  readonly email?: string | null;
  readonly display_name?: string | null;
  readonly role: OrgRole;
  readonly joined_at: number;
  readonly removed_at: number | null;
}

export interface OrganizationInvitationView {
  readonly id: string;
  readonly organization_id: string;
  readonly workspace_id: string;
  readonly invited_email: string;
  readonly role: OrgRole;
  readonly invited_by: string;
  readonly status: InvitationStatus;
  readonly expires_at: number;
  readonly accepted_at: number | null;
  readonly declined_at: number | null;
  readonly revoked_at: number | null;
  readonly created_at: number;
}

export interface CreatedInvitationResponse {
  readonly invitation_id: string;
  readonly invitation: OrganizationInvitationView;
  readonly token: string;
  readonly invite_url: string;
  readonly expires_at: number;
}

export interface ServerEncryptedSyncRecord {
  readonly id: string;
  readonly workspace_id: string;
  readonly user_id: string;
  readonly resource_kind: string;
  readonly resource_id: string;
  readonly version: number;
  readonly workspace_seq: number;
  readonly ciphertext_b64: string;
  readonly nonce_b64: string;
  readonly key_id: string;
  readonly key_epoch: number;
  readonly content_hash: string;
  readonly deleted: boolean;
  readonly origin_device_id: string;
  readonly updated_at: number;
}

export interface SyncChangesApiResponse {
  readonly workspace_id: string;
  readonly latest_workspace_seq: number;
  readonly has_more: boolean;
  readonly records: readonly ServerEncryptedSyncRecord[];
}

export class SafiOrganizationApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message?: string) {
    super(message ?? code);
    this.name = "SafiOrganizationApiError";
    this.status = status;
    this.code = code;
  }
}

interface RequestOptions {
  readonly method?: "GET" | "POST" | "PATCH" | "DELETE";
  readonly sessionToken: string;
  readonly workspaceId?: string | null;
  readonly body?: unknown;
  readonly apiBaseUrlOverride?: string;
}

async function sendOrgApi<T>(path: string, opts: RequestOptions): Promise<T> {
  const baseUrl = (opts.apiBaseUrlOverride ?? getDefaultAuthApiBaseUrl()).replace(/\/+$/, "");
  const headers = new Headers({
    Accept: "application/json",
    Authorization: `Bearer ${opts.sessionToken}`
  });
  if (opts.body !== undefined) {
    headers.set("Content-Type", "application/json");
  }
  if (opts.workspaceId && opts.workspaceId.trim().length > 0) {
    headers.set("X-Safi-Workspace-Id", opts.workspaceId.trim());
  }

  let response: Response;
  try {
    response = await fetch(`${baseUrl}${path}`, {
      method: opts.method ?? "GET",
      headers,
      ...(opts.body === undefined ? {} : { body: JSON.stringify(opts.body) }),
      cache: "no-store",
      credentials: "omit",
      referrerPolicy: "no-referrer"
    });
  } catch {
    throw new SafiOrganizationApiError(0, "SF-AUTH-005");
  }

  if (!response.ok) {
    let code = response.status === 403 ? "SF-SYN-005" : "SF-AUTH-006";
    let detail: string | undefined;
    try {
      const problem = (await response.json()) as { code?: string; detail?: string; title?: string };
      if (typeof problem.code === "string" && problem.code.length > 0) {
        code = problem.code;
      }
      detail = problem.detail ?? problem.title;
    } catch {
      // Ignore malformed error body.
    }
    throw new SafiOrganizationApiError(response.status, code, detail);
  }

  if (response.status === 204) {
    return undefined as T;
  }
  return (await response.json()) as T;
}

export function fetchMyOrganizations(
  sessionToken: string,
  apiBaseUrlOverride?: string
): Promise<MyOrganizationsResponse> {
  return sendOrgApi<MyOrganizationsResponse>("/v1/organizations/mine", {
    sessionToken,
    ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
  });
}

export function fetchBillingOrganizationsSummary(
  billingOrDesktopToken: string,
  apiBaseUrlOverride?: string
): Promise<MyOrganizationsResponse> {
  return sendOrgApi<MyOrganizationsResponse>("/v1/billing/organizations-summary", {
    sessionToken: billingOrDesktopToken,
    ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
  });
}

export function createOrganization(
  sessionToken: string,
  params: { readonly name: string; readonly seats: number; readonly billing_cycle?: string },
  apiBaseUrlOverride?: string
): Promise<OrganizationRecord> {
  return sendOrgApi<OrganizationRecord>("/v1/organizations", {
    method: "POST",
    sessionToken,
    body: params,
    ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
  });
}

export function fetchOrganizationDetails(
  sessionToken: string,
  orgId: string,
  apiBaseUrlOverride?: string
): Promise<EnrichedOrganizationSummary> {
  return sendOrgApi<EnrichedOrganizationSummary>(`/v1/organizations/${encodeURIComponent(orgId)}`, {
    sessionToken,
    ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
  });
}

export async function deleteOrganization(
  sessionToken: string,
  orgId: string,
  apiBaseUrlOverride?: string
): Promise<void> {
  await sendOrgApi<undefined>(`/v1/organizations/${encodeURIComponent(orgId)}`, {
    method: "DELETE",
    sessionToken,
    ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
  });
}

export function fetchOrganizationMembers(
  sessionToken: string,
  orgId: string,
  apiBaseUrlOverride?: string
): Promise<readonly OrganizationMemberRecord[]> {
  return sendOrgApi<readonly OrganizationMemberRecord[]>(
    `/v1/organizations/${encodeURIComponent(orgId)}/members`,
    {
      sessionToken,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function changeOrganizationMemberRole(
  sessionToken: string,
  orgId: string,
  targetUserId: string,
  role: OrgRole,
  apiBaseUrlOverride?: string
): Promise<OrganizationMemberRecord> {
  return sendOrgApi<OrganizationMemberRecord>(
    `/v1/organizations/${encodeURIComponent(orgId)}/members/${encodeURIComponent(targetUserId)}/role`,
    {
      method: "PATCH",
      sessionToken,
      body: { role },
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export async function removeOrganizationMember(
  sessionToken: string,
  orgId: string,
  targetUserId: string,
  apiBaseUrlOverride?: string
): Promise<void> {
  await sendOrgApi<undefined>(
    `/v1/organizations/${encodeURIComponent(orgId)}/members/${encodeURIComponent(targetUserId)}`,
    {
      method: "DELETE",
      sessionToken,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function transferOrganizationOwnership(
  sessionToken: string,
  orgId: string,
  newOwnerUserId: string,
  apiBaseUrlOverride?: string
): Promise<OrganizationRecord> {
  return sendOrgApi<OrganizationRecord>(
    `/v1/organizations/${encodeURIComponent(orgId)}/transfer-ownership`,
    {
      method: "POST",
      sessionToken,
      body: { new_owner_user_id: newOwnerUserId },
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function fetchOrganizationInvitations(
  sessionToken: string,
  orgId: string,
  apiBaseUrlOverride?: string
): Promise<readonly OrganizationInvitationView[]> {
  return sendOrgApi<readonly OrganizationInvitationView[]>(
    `/v1/organizations/${encodeURIComponent(orgId)}/invitations`,
    {
      sessionToken,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function inviteOrganizationMember(
  sessionToken: string,
  orgId: string,
  params: { readonly email: string; readonly role: OrgRole; readonly ttl_secs?: number },
  apiBaseUrlOverride?: string
): Promise<CreatedInvitationResponse> {
  return sendOrgApi<CreatedInvitationResponse>(
    `/v1/organizations/${encodeURIComponent(orgId)}/invite`,
    {
      method: "POST",
      sessionToken,
      body: params,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function revokeOrganizationInvitation(
  sessionToken: string,
  orgId: string,
  inviteId: string,
  apiBaseUrlOverride?: string
): Promise<OrganizationInvitationView> {
  return sendOrgApi<OrganizationInvitationView>(
    `/v1/organizations/${encodeURIComponent(orgId)}/invitations/${encodeURIComponent(inviteId)}/revoke`,
    {
      method: "POST",
      sessionToken,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function acceptOrganizationInvitation(
  sessionToken: string,
  params: { readonly token?: string; readonly invitation_id?: string },
  apiBaseUrlOverride?: string
): Promise<OrganizationMemberRecord> {
  return sendOrgApi<OrganizationMemberRecord>("/v1/organizations/invitations/accept", {
    method: "POST",
    sessionToken,
    body: params,
    ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
  });
}

export function declineOrganizationInvitation(
  sessionToken: string,
  params: { readonly token?: string; readonly invitation_id?: string },
  apiBaseUrlOverride?: string
): Promise<OrganizationInvitationView> {
  return sendOrgApi<OrganizationInvitationView>("/v1/organizations/invitations/decline", {
    method: "POST",
    sessionToken,
    body: params,
    ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
  });
}

export function registerOrganizationDeviceKey(
  sessionToken: string,
  orgId: string,
  params: {
    readonly device_id: string;
    readonly device_label: string;
    readonly x25519_public_key_b64: string;
  },
  apiBaseUrlOverride?: string
): Promise<DesktopRemoteMemberDeviceKey> {
  return sendOrgApi<DesktopRemoteMemberDeviceKey>(
    `/v1/organizations/${encodeURIComponent(orgId)}/keys/devices`,
    {
      method: "POST",
      sessionToken,
      body: params,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function fetchOrganizationDeviceKeys(
  sessionToken: string,
  orgId: string,
  apiBaseUrlOverride?: string
): Promise<readonly DesktopRemoteMemberDeviceKey[]> {
  return sendOrgApi<readonly DesktopRemoteMemberDeviceKey[]>(
    `/v1/organizations/${encodeURIComponent(orgId)}/keys/devices`,
    {
      sessionToken,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function publishWrappedWorkspaceKeys(
  sessionToken: string,
  orgId: string,
  params: { readonly key_epoch: number; readonly entries: readonly DesktopWrappedKeyWireEntry[] },
  apiBaseUrlOverride?: string
): Promise<{ readonly key_epoch: number; readonly wrapped_count: number }> {
  return sendOrgApi<{ readonly key_epoch: number; readonly wrapped_count: number }>(
    `/v1/organizations/${encodeURIComponent(orgId)}/keys/wrap`,
    {
      method: "POST",
      sessionToken,
      body: params,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function rotateOrganizationWorkspaceKey(
  sessionToken: string,
  orgId: string,
  params: {
    readonly expected_current_epoch: number;
    readonly entries: readonly DesktopWrappedKeyWireEntry[];
  },
  apiBaseUrlOverride?: string
): Promise<{ readonly organization_id: string; readonly active_key_epoch: number }> {
  return sendOrgApi<{ readonly organization_id: string; readonly active_key_epoch: number }>(
    `/v1/organizations/${encodeURIComponent(orgId)}/keys/rotate`,
    {
      method: "POST",
      sessionToken,
      body: params,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function fetchMyWrappedWorkspaceKeys(
  sessionToken: string,
  orgId: string,
  deviceId: string,
  apiBaseUrlOverride?: string
): Promise<readonly DesktopRemoteWrappedKeyEnvelope[]> {
  return sendOrgApi<readonly DesktopRemoteWrappedKeyEnvelope[]>(
    `/v1/organizations/${encodeURIComponent(orgId)}/keys/my-wrapped?device_id=${encodeURIComponent(deviceId)}`,
    {
      sessionToken,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function pushEncryptedSyncEnvelope(
  sessionToken: string,
  workspaceId: string,
  envelope: DesktopOutboundEncryptedEnvelope,
  apiBaseUrlOverride?: string
): Promise<ServerEncryptedSyncRecord> {
  return sendOrgApi<ServerEncryptedSyncRecord>("/v1/sync/envelopes", {
    method: "POST",
    sessionToken,
    workspaceId,
    body: {
      resource_kind: envelope.resource_kind,
      resource_id: envelope.resource_id,
      expected_version: envelope.expected_version,
      ciphertext_b64: envelope.ciphertext_b64,
      nonce_b64: envelope.nonce_b64,
      key_id: envelope.key_id,
      key_epoch: envelope.key_epoch,
      content_hash: envelope.content_hash,
      deleted: envelope.deleted,
      origin_device_id: envelope.origin_device_id,
      idempotency_key: envelope.idempotency_key
    },
    ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
  });
}

export function pullEncryptedSyncEnvelope(
  sessionToken: string,
  workspaceId: string,
  resourceKind: string,
  resourceId: string,
  apiBaseUrlOverride?: string
): Promise<ServerEncryptedSyncRecord> {
  return sendOrgApi<ServerEncryptedSyncRecord>(
    `/v1/sync/envelopes/${encodeURIComponent(resourceKind)}/${encodeURIComponent(resourceId)}`,
    {
      sessionToken,
      workspaceId,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

export function fetchEncryptedSyncChanges(
  sessionToken: string,
  workspaceId: string,
  sinceSeq: number,
  limit = 100,
  apiBaseUrlOverride?: string
): Promise<SyncChangesApiResponse> {
  return sendOrgApi<SyncChangesApiResponse>(
    `/v1/sync/changes?since_seq=${encodeURIComponent(String(sinceSeq))}&limit=${encodeURIComponent(String(limit))}`,
    {
      sessionToken,
      workspaceId,
      ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
    }
  );
}

/**
 * Full Desktop E2EE Sync cycle for a Shared Team Workspace:
 *  1. Selects the shared workspace in native Rust `OrgSyncStore`.
 *  2. Registers this device's X25519 public key on the server.
 *  3. Downloads & unwraps any wrapped Workspace Keys addressed to this device.
 *  4. If the caller holds `owner` or `admin` (or `member` with unlocked key), wraps the active
 *     epoch key for all active member devices so newly joined teammates receive access.
 *  5. Drains the local encrypted offline queue (`ChaCha20-Poly1305` ciphertext only), handling
 *     idempotent retries, `409 SF-SYN-002` version conflicts, and network drops safely.
 *  6. Incrementally pulls and decrypts remote changes (`since_seq`), including tombstones.
 */
export async function synchronizeDesktopOrganizationWorkspace(params: {
  readonly sessionToken: string;
  readonly organizationId: string;
  readonly workspaceId: string;
  readonly activeKeyEpoch: number;
  readonly myRole: OrgRole;
  readonly apiBaseUrlOverride?: string;
}): Promise<DesktopWorkspaceSyncStatus> {
  const { sessionToken, organizationId, workspaceId, activeKeyEpoch, myRole, apiBaseUrlOverride } =
    params;

  let status = await selectDesktopSyncWorkspace(organizationId, workspaceId, activeKeyEpoch);

  try {
    // 1. Register this installation's X25519 public key (idempotent on server).
    await registerOrganizationDeviceKey(
      sessionToken,
      organizationId,
      {
        device_id: status.device_identity.device_id,
        device_label: status.device_identity.device_label,
        x25519_public_key_b64: status.device_identity.x25519_public_key_b64
      },
      apiBaseUrlOverride
    );

    // 2. Pull and unwrap any Workspace Keys wrapped for this device.
    const myWrapped = await fetchMyWrappedWorkspaceKeys(
      sessionToken,
      organizationId,
      status.device_identity.device_id,
      apiBaseUrlOverride
    );
    if (myWrapped.length > 0) {
      await ingestDesktopWrappedKeys(myWrapped);
    }

    status = await getDesktopSyncStatus(workspaceId);

    // 3. If we can modify shared resources and either hold the epoch key OR need to bootstrap
    //    Epoch 1 as the Owner/Admin, wrap the epoch key for all active member devices!
    if (
      myRole !== "viewer" &&
      (status.unlocked_epochs.includes(activeKeyEpoch) ||
        (activeKeyEpoch === 1 && (myRole === "owner" || myRole === "admin")))
    ) {
      const devices = await fetchOrganizationDeviceKeys(
        sessionToken,
        organizationId,
        apiBaseUrlOverride
      );
      if (devices.length > 0) {
        const wrappedEntries = await wrapDesktopEpochKeysForDevices(
          organizationId,
          workspaceId,
          activeKeyEpoch,
          devices
        );
        await publishWrappedWorkspaceKeys(
          sessionToken,
          organizationId,
          { key_epoch: activeKeyEpoch, entries: wrappedEntries },
          apiBaseUrlOverride
        );
      }
      status = await getDesktopSyncStatus(workspaceId);
    }

    // 4. Drain outbound offline queue (only if active key epoch is unlocked & role is not viewer).
    if (myRole !== "viewer" && status.unlocked_epochs.includes(status.active_key_epoch)) {
      for (let guard = 0; guard < 100; guard += 1) {
        const nextEnv = await peekDesktopOutboundSyncEnvelope(workspaceId);
        if (!nextEnv) break;
        try {
          const committed = await pushEncryptedSyncEnvelope(
            sessionToken,
            workspaceId,
            nextEnv,
            apiBaseUrlOverride
          );
          await completeDesktopOutboundSyncEnvelope({
            outcome: "committed",
            idempotencyKey: nextEnv.idempotency_key,
            version: committed.version,
            workspaceSeq: committed.workspace_seq,
            authorUserId: committed.user_id
          });
        } catch (err) {
          if (err instanceof SafiOrganizationApiError && err.code === "SF-SYN-002") {
            const remote = await pullEncryptedSyncEnvelope(
              sessionToken,
              workspaceId,
              nextEnv.resource_kind,
              nextEnv.resource_id,
              apiBaseUrlOverride
            );
            await completeDesktopOutboundSyncEnvelope({
              outcome: "conflict",
              idempotencyKey: nextEnv.idempotency_key,
              remoteEnvelope: remote
            });
            break;
          }
          const code = err instanceof SafiOrganizationApiError ? err.code : "SF-AUTH-005";
          await completeDesktopOutboundSyncEnvelope({
            outcome: "error",
            workspaceId,
            errorCode: code
          });
          throw err;
        }
      }
    }

    // 5. Incremental pull loop (`since_seq`) when at least one epoch key is unlocked.
    status = await getDesktopSyncStatus(workspaceId);
    if (status.unlocked_epochs.length > 0) {
      let cursor = status.last_workspace_seq;
      for (let page = 0; page < 20; page += 1) {
        const delta = await fetchEncryptedSyncChanges(
          sessionToken,
          workspaceId,
          cursor,
          100,
          apiBaseUrlOverride
        );
        if (delta.records.length > 0) {
          const applied = await applyDesktopPulledEnvelopes(workspaceId, delta.records);
          status = applied.status;
          cursor = status.last_workspace_seq;
        } else {
          break;
        }
        if (!delta.has_more) break;
      }
    }

    return await getDesktopSyncStatus(workspaceId);
  } catch (err) {
    const code = err instanceof SafiOrganizationApiError ? err.code : "SF-AUTH-005";
    try {
      await completeDesktopOutboundSyncEnvelope({
        outcome: "error",
        workspaceId,
        errorCode: code
      });
    } catch {
      // Preserve primary error.
    }
    throw err;
  }
}

/**
 * Remove a member and immediately rotate the Organization Workspace Key (`epoch + 1`)
 * on Desktop so future revisions use a fresh key never shared with the removed member.
 */
export async function removeMemberAndRotateDesktopKey(params: {
  readonly sessionToken: string;
  readonly organizationId: string;
  readonly workspaceId: string;
  readonly currentKeyEpoch: number;
  readonly targetUserId: string;
  readonly desktopRuntime: boolean;
  readonly apiBaseUrlOverride?: string;
}): Promise<{ readonly rotatedEpoch: number | null }> {
  await removeOrganizationMember(
    params.sessionToken,
    params.organizationId,
    params.targetUserId,
    params.apiBaseUrlOverride
  );
  if (!params.desktopRuntime) {
    return { rotatedEpoch: null };
  }
  const remainingDevices = await fetchOrganizationDeviceKeys(
    params.sessionToken,
    params.organizationId,
    params.apiBaseUrlOverride
  );
  if (remainingDevices.length === 0) {
    return { rotatedEpoch: null };
  }
  const prepared = await prepareDesktopWorkspaceKeyRotation(
    params.organizationId,
    params.workspaceId,
    params.currentKeyEpoch,
    remainingDevices
  );
  const rotated = await rotateOrganizationWorkspaceKey(
    params.sessionToken,
    params.organizationId,
    {
      expected_current_epoch: params.currentKeyEpoch,
      entries: prepared.entries
    },
    params.apiBaseUrlOverride
  );
  return { rotatedEpoch: rotated.active_key_epoch };
}

export function describeOrgError(error: unknown, isAr: boolean): string {
  const code =
    error instanceof SafiOrganizationApiError
      ? error.code
      : error instanceof Error
        ? error.message
        : "SF-AUTH-006";

  if (code.includes("SF-SYN-005")) {
    return isAr
      ? "ليس لديك صلاحية كافية لتنفيذ هذه العملية، أو أن البريد الموثق لا يطابق بريد الدعوة (SF-SYN-005)."
      : "You do not have permission for this action, or your verified account email does not match the invitation email (SF-SYN-005).";
  }
  if (code.includes("SF-LIC-008")) {
    return isAr
      ? "تتطلب هذه الميزة اشتراك مؤسسة فعّال (Organization Plan — بحد أدنى 3 مقاعد) (SF-LIC-008)."
      : "This feature requires an active Organization subscription (minimum 3 seats) (SF-LIC-008).";
  }
  if (code.includes("SF-LIC-005")) {
    return isAr
      ? "تم الوصول إلى الحد الأقصى للمقاعد المدفوعة. اشترِ مقاعد إضافية لإضافة أعضاء جدد (SF-LIC-005)."
      : "All purchased seats are currently in use or reserved by pending invitations. Upgrade seats to invite more members (SF-LIC-005).";
  }
  if (code.includes("SF-SYN-004")) {
    return isAr
      ? "خطة المؤسسة تتطلب 3 مقاعد على الأقل (SF-SYN-004)."
      : "The Organization plan requires a minimum of 3 seats (SF-SYN-004).";
  }
  if (code.includes("SF-SYN-002")) {
    return isAr
      ? "تعارض في الإصدار أو دعوة مكررة لنفس البريد الإلكتروني — راجع أحدث حالة (SF-SYN-002)."
      : "A version conflict or duplicate active invitation was detected — review the latest workspace state (SF-SYN-002).";
  }
  if (code.includes("SF-AUTH-007")) {
    return isAr
      ? "رمز الدعوة غير صالح أو انتهت صلاحيته أو استُخدم مسبقًا (SF-AUTH-007)."
      : "This invitation token is invalid, expired, or has already been used (SF-AUTH-007).";
  }
  if (code.includes("SF-AUTH-005")) {
    return isAr
      ? "تعذّر الاتصال بالخادم — التعديلات المحلية في Desktop محفوظة مشفّرة في قائمة الانتظار وستُزامن فور عودة الاتصال (SF-AUTH-005)."
      : "Server unreachable — local Desktop edits remain encrypted in your offline queue and will sync automatically when reconnected (SF-AUTH-005).";
  }
  return isAr
    ? `تعذّر إتمام العملية (${code}).`
    : `Could not complete the requested operation (${code}).`;
}
