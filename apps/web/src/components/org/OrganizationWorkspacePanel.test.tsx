import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApplicationShell } from "../../ApplicationShell";
import {
  SafiOrganizationApiError,
  acceptOrganizationInvitation,
  changeOrganizationMemberRole,
  createOrganization,
  declineOrganizationInvitation,
  deleteOrganization,
  describeOrgError,
  fetchBillingOrganizationsSummary,
  fetchEncryptedSyncChanges,
  fetchMyOrganizations,
  inviteOrganizationMember,
  pushEncryptedSyncEnvelope,
  revokeOrganizationInvitation,
  transferOrganizationOwnership
} from "../../lib/organization-api";
import { OrganizationWorkspacePanel } from "./OrganizationWorkspacePanel";

class MemoryStorage {
  private readonly store = new Map<string, string>();
  getItem(key: string): string | null {
    return this.store.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    this.store.set(key, value);
  }
  removeItem(key: string): void {
    this.store.delete(key);
  }
}

describe("Team & Organization Workspace UI (/team + Desktop Studio) & API client", () => {
  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
    vi.restoreAllMocks();
  });

  it("mounts /team inside ApplicationShell without leaking unauthenticated org/member details or Desktop Studio", () => {
    const replaceState = vi.fn();
    const sessionStorage = new MemoryStorage();
    (globalThis as { window?: unknown }).window = {
      location: {
        href: "https://safidata.app/team?invite=sfi_secret_single_use_invite_token_123456",
        pathname: "/team",
        search: "?invite=sfi_secret_single_use_invite_token_123456",
        hash: "",
        origin: "https://safidata.app"
      },
      history: { state: null, replaceState, pushState: () => {} },
      sessionStorage
    };

    const html = renderToString(createElement(ApplicationShell));
    expect(html).toContain('data-runtime="team-web-portal"');
    expect(html).toContain('data-org-workspace-panel="unauthenticated"');
    expect(html).toContain('data-testid="org-signin-cta"');
    // Token is scrubbed from the URL bar and staged in sessionStorage until verified login
    expect(replaceState).toHaveBeenCalledWith(null, "", "/team");
    expect(sessionStorage.getItem("safi.org.pending_invite.v1")).toBe(
      "sfi_secret_single_use_invite_token_123456"
    );
    // Zero sensitive studio/owner controls or raw token leakage in rendered markup
    expect(html).not.toContain("sfi_secret_single_use_invite_token_123456");
    expect(html).not.toContain('data-tab-id="owner"');
    expect(html).not.toContain("Quad-Studio");
  });

  it("renders authenticated Website & Desktop OrganizationWorkspacePanel in English (LTR) and Arabic (RTL) including offline read-only mode", () => {
    const webEn = renderToString(
      createElement(OrganizationWorkspacePanel, {
        lang: "en",
        sessionToken: "sfs_test_session_1234567890",
        desktopRuntime: false
      })
    );
    expect(webEn).toContain('data-org-workspace-panel="authenticated"');
    expect(webEn).toContain('dir="ltr"');
    expect(webEn).toContain('data-testid="create-org-name-input"');
    expect(webEn).toContain('data-testid="create-org-seats-input"');
    expect(webEn).toContain('data-testid="switch-personal-workspace"');
    expect(webEn).toContain('data-testid="org-buy-plan-btn"');

    const desktopArOffline = renderToString(
      createElement(OrganizationWorkspacePanel, {
        lang: "ar",
        sessionToken: null,
        desktopRuntime: true,
        readOnlyOffline: true
      })
    );
    expect(desktopArOffline).toContain('data-org-workspace-panel="authenticated"');
    expect(desktopArOffline).toContain('dir="rtl"');
    expect(desktopArOffline).toContain('data-testid="switch-personal-workspace"');
  });

  it("executes organization, RBAC, invitation, and X-Safi-Workspace-Id sync requests against SAFI_API_BASE_URL", async () => {
    const calls: Array<{
      url: string;
      method: string;
      headers: Record<string, string>;
      body: string | null;
    }> = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
        const url = String(input);
        const headersObj: Record<string, string> = {};
        new Headers(init?.headers).forEach((value, key) => {
          headersObj[key.toLowerCase()] = value;
        });
        calls.push({
          url,
          method: init?.method ?? "GET",
          headers: headersObj,
          body: typeof init?.body === "string" ? init.body : null
        });

        if (url.endsWith("/v1/organizations/mine")) {
          return new Response(
            JSON.stringify({
              personal_workspace_id: "00000000-0000-7000-8000-000000000001",
              verified_email: "alice@acme.test",
              organizations: [],
              pending_invitations: []
            }),
            { status: 200, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.endsWith("/v1/billing/organizations-summary")) {
          return new Response(
            JSON.stringify({
              personal_workspace_id: "00000000-0000-7000-8000-000000000001",
              verified_email: "alice@acme.test",
              organizations: [],
              pending_invitations: []
            }),
            { status: 200, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.endsWith("/v1/organizations") && init?.method === "POST") {
          return new Response(
            JSON.stringify({
              id: "10000000-0000-7000-8000-000000000010",
              workspace_id: "20000000-0000-7000-8000-000000000020",
              name: "Acme Data Team",
              purchased_seats: 5,
              billing_cycle: "annual",
              owner_user_id: "30000000-0000-7000-8000-000000000030",
              active_key_epoch: 1,
              created_at: 1700000000
            }),
            { status: 201, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.includes("/invitations/accept")) {
          return new Response(
            JSON.stringify({
              code: "SF-SYN-005",
              detail: "Invitation email does not match verified account."
            }),
            { status: 403, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.includes("/invitations/decline")) {
          return new Response(
            JSON.stringify({
              id: "40000000-0000-7000-8000-000000000040",
              organization_id: "10000000-0000-7000-8000-000000000010",
              workspace_id: "20000000-0000-7000-8000-000000000020",
              invited_email: "bob@acme.test",
              role: "member",
              invited_by: "30000000-0000-7000-8000-000000000030",
              status: "declined",
              expires_at: 1700604800,
              accepted_at: null,
              declined_at: 1700000500,
              revoked_at: null,
              created_at: 1700000000
            }),
            { status: 200, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.endsWith("/invite") && init?.method === "POST") {
          return new Response(
            JSON.stringify({
              invitation_id: "40000000-0000-7000-8000-000000000040",
              token: "sfi_unit_test_token_1234567890",
              invite_url: "https://safidata.app/team?invite=sfi_unit_test_token_1234567890",
              expires_at: 1700604800,
              invitation: {
                id: "40000000-0000-7000-8000-000000000040",
                organization_id: "10000000-0000-7000-8000-000000000010",
                workspace_id: "20000000-0000-7000-8000-000000000020",
                invited_email: "bob@acme.test",
                role: "member",
                invited_by: "30000000-0000-7000-8000-000000000030",
                status: "pending",
                expires_at: 1700604800,
                accepted_at: null,
                declined_at: null,
                revoked_at: null,
                created_at: 1700000000
              }
            }),
            { status: 201, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.endsWith("/revoke") && init?.method === "POST") {
          return new Response(
            JSON.stringify({
              id: "40000000-0000-7000-8000-000000000040",
              organization_id: "10000000-0000-7000-8000-000000000010",
              workspace_id: "20000000-0000-7000-8000-000000000020",
              invited_email: "bob@acme.test",
              role: "member",
              invited_by: "30000000-0000-7000-8000-000000000030",
              status: "revoked",
              expires_at: 1700604800,
              accepted_at: null,
              declined_at: null,
              revoked_at: 1700000600,
              created_at: 1700000000
            }),
            { status: 200, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.endsWith("/role") && init?.method === "PATCH") {
          return new Response(
            JSON.stringify({
              organization_id: "10000000-0000-7000-8000-000000000010",
              workspace_id: "20000000-0000-7000-8000-000000000020",
              user_id: "50000000-0000-7000-8000-000000000050",
              role: "viewer",
              joined_at: 1700000100,
              removed_at: null
            }),
            { status: 200, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.endsWith("/transfer-ownership") && init?.method === "POST") {
          return new Response(
            JSON.stringify({
              id: "10000000-0000-7000-8000-000000000010",
              workspace_id: "20000000-0000-7000-8000-000000000020",
              name: "Acme Data Team",
              purchased_seats: 5,
              billing_cycle: "annual",
              owner_user_id: "50000000-0000-7000-8000-000000000050",
              active_key_epoch: 1,
              created_at: 1700000000
            }),
            { status: 200, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.includes("/v1/sync/envelopes") && init?.method === "POST") {
          return new Response(
            JSON.stringify({ code: "SF-SYN-002", detail: "Version conflict on shared resource." }),
            { status: 409, headers: { "Content-Type": "application/json" } }
          );
        }
        if (url.includes("/v1/sync/changes")) {
          return new Response(
            JSON.stringify({
              workspace_id: "20000000-0000-7000-8000-000000000020",
              latest_workspace_seq: 7,
              has_more: false,
              records: []
            }),
            { status: 200, headers: { "Content-Type": "application/json" } }
          );
        }
        return new Response(null, { status: 204 });
      })
    );

    const apiBase = "https://api.safidata.test";
    const mine = await fetchMyOrganizations("sfs_tok", apiBase);
    expect(mine.verified_email).toBe("alice@acme.test");

    const billingSummary = await fetchBillingOrganizationsSummary("sfb_tok", apiBase);
    expect(billingSummary.personal_workspace_id).toBe("00000000-0000-7000-8000-000000000001");

    const created = await createOrganization(
      "sfs_tok",
      { name: "Acme Data Team", seats: 5, billing_cycle: "annual" },
      apiBase
    );
    expect(created.workspace_id).toBe("20000000-0000-7000-8000-000000000020");

    const inv = await inviteOrganizationMember(
      "sfs_tok",
      created.id,
      { email: "bob@acme.test", role: "member" },
      apiBase
    );
    expect(inv.token).toBe("sfi_unit_test_token_1234567890");

    // Wrong-email acceptance returns typed 403 SF-SYN-005
    await expect(
      acceptOrganizationInvitation(
        "sfs_intruder",
        { token: "sfi_unit_test_token_1234567890" },
        apiBase
      )
    ).rejects.toMatchObject({
      status: 403,
      code: "SF-SYN-005"
    });

    const declined = await declineOrganizationInvitation(
      "sfs_bob",
      { token: "sfi_unit_test_token_1234567890" },
      apiBase
    );
    expect(declined.status).toBe("declined");

    const revoked = await revokeOrganizationInvitation(
      "sfs_tok",
      created.id,
      inv.invitation_id,
      apiBase
    );
    expect(revoked.status).toBe("revoked");

    const changed = await changeOrganizationMemberRole(
      "sfs_tok",
      created.id,
      "50000000-0000-7000-8000-000000000050",
      "viewer",
      apiBase
    );
    expect(changed.role).toBe("viewer");

    const transferred = await transferOrganizationOwnership(
      "sfs_tok",
      created.id,
      "50000000-0000-7000-8000-000000000050",
      apiBase
    );
    expect(transferred.owner_user_id).toBe("50000000-0000-7000-8000-000000000050");

    await deleteOrganization("sfs_tok", created.id, apiBase);

    // Sync requests MUST attach X-Safi-Workspace-Id and idempotency_key
    await expect(
      pushEncryptedSyncEnvelope(
        "sfs_tok",
        created.workspace_id,
        {
          idempotency_key: "idem_1234567890",
          workspace_id: created.workspace_id,
          resource_kind: "recipe",
          resource_id: "clean_sales",
          expected_version: 1,
          deleted: false,
          ciphertext_b64: "Y2lwaGVydGV4dA==",
          nonce_b64: "MTIzNDU2Nzg5MDEy",
          key_id: "wk:20000000-0000-7000-8000-000000000020:epoch:1",
          key_epoch: 1,
          content_hash: "a".repeat(64),
          origin_device_id: "dev_a"
        },
        apiBase
      )
    ).rejects.toMatchObject({
      status: 409,
      code: "SF-SYN-002"
    });

    const changes = await fetchEncryptedSyncChanges(
      "sfs_tok",
      created.workspace_id,
      0,
      100,
      apiBase
    );
    expect(changes.latest_workspace_seq).toBe(7);

    const pushCall = calls.find((c) => c.url.endsWith("/v1/sync/envelopes"));
    expect(pushCall?.headers["x-safi-workspace-id"]).toBe(created.workspace_id);
    expect(JSON.parse(pushCall?.body ?? "{}").idempotency_key).toBe("idem_1234567890");

    const pullCall = calls.find((c) => c.url.includes("/v1/sync/changes"));
    expect(pullCall?.headers["x-safi-workspace-id"]).toBe(created.workspace_id);

    // Clear bilingual error messages with error codes
    expect(describeOrgError(new SafiOrganizationApiError(403, "SF-SYN-005"), false)).toContain(
      "SF-SYN-005"
    );
    expect(describeOrgError(new SafiOrganizationApiError(403, "SF-SYN-005"), true)).toContain(
      "SF-SYN-005"
    );
    expect(describeOrgError(new SafiOrganizationApiError(409, "SF-SYN-002"), false)).toContain(
      "SF-SYN-002"
    );
    expect(describeOrgError(new SafiOrganizationApiError(402, "SF-PAY-002"), false)).toContain(
      "SF-PAY-002"
    );
  });
});
