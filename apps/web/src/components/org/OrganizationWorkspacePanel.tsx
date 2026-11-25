/**
 * `apps/web/src/components/org/OrganizationWorkspacePanel.tsx`
 *
 * Production Organization, Shared Team Workspace, 4-Role RBAC, Verified-Email Invitations,
 * and Desktop Zero-Knowledge E2EE Sync Experience (Website `/team` + Desktop Studio).
 */
import type { FormEvent, ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  Building2,
  Check,
  Copy,
  CreditCard,
  KeyRound,
  Lock,
  RefreshCw,
  ShieldCheck,
  Trash2,
  UserMinus,
  UserPlus,
  Users,
  X
} from "lucide-react";
import type { SafiLang } from "../layout/FloatingGlassDock";
import {
  getDesktopSyncStatus,
  listDesktopSyncConflicts,
  listDesktopSyncedResources,
  prepareDesktopWorkspaceKeyRotation,
  resolveDesktopSyncConflict,
  rotateDesktopLocalDeviceKey,
  selectDesktopSyncWorkspace,
  stageDesktopSharedResourceDelete,
  stageDesktopSharedResourceUpsert,
  type DesktopSharedWorkspaceResource,
  type DesktopSyncConflictItem,
  type DesktopWorkspaceSyncStatus
} from "../../desktop/studio-bridge";
import { launchDesktopWebsiteBilling } from "../../lib/desktop-billing-handoff";
import { BUSINESS_PRICING, normalizeOrganizationSeats } from "../../lib/business-pricing";
import {
  acceptOrganizationInvitation,
  changeOrganizationMemberRole,
  createOrganization,
  declineOrganizationInvitation,
  deleteOrganization,
  describeOrgError,
  fetchMyOrganizations,
  fetchOrganizationDeviceKeys,
  fetchOrganizationInvitations,
  fetchOrganizationMembers,
  inviteOrganizationMember,
  registerOrganizationDeviceKey,
  removeMemberAndRotateDesktopKey,
  revokeOrganizationInvitation,
  rotateOrganizationWorkspaceKey,
  synchronizeDesktopOrganizationWorkspace,
  transferOrganizationOwnership,
  type CreatedInvitationResponse,
  type EnrichedOrganizationSummary,
  type OrgRole,
  type OrganizationInvitationView,
  type OrganizationMemberRecord,
  type PendingUserInvitation
} from "../../lib/organization-api";

const PENDING_INVITE_STORAGE_KEY = "safi.org.pending_invite.v1";

function consumeInitialInviteTokenFromUrl(): string {
  if (typeof window === "undefined") return "";
  try {
    const url = new URL(window.location.href);
    const fromQuery = (url.searchParams.get("invite") ?? "").trim();
    if (fromQuery.startsWith("sfi_")) {
      try {
        window.sessionStorage.setItem(PENDING_INVITE_STORAGE_KEY, fromQuery);
      } catch {
        // Ignore storage failure.
      }
      url.searchParams.delete("invite");
      window.history.replaceState(
        window.history.state,
        "",
        `${url.pathname}${url.search}${url.hash}`
      );
      return fromQuery;
    }
    const saved = window.sessionStorage.getItem(PENDING_INVITE_STORAGE_KEY) ?? "";
    return saved.trim();
  } catch {
    return "";
  }
}

function clearSavedInviteToken(): void {
  if (typeof window === "undefined") return;
  try {
    window.sessionStorage.removeItem(PENDING_INVITE_STORAGE_KEY);
  } catch {
    // Ignore storage failure.
  }
}

export interface OrganizationWorkspacePanelProps {
  readonly lang?: SafiLang;
  readonly sessionToken: string | null;
  readonly desktopRuntime?: boolean;
  readonly readOnlyOffline?: boolean;
  readonly apiBaseUrlOverride?: string;
  readonly onRequestSignIn?: () => void;
  readonly onOpenBillingPortal?: (seats: number) => void;
  readonly onBackToShowcase?: () => void;
  readonly onCloseModal?: () => void;
  readonly onActiveWorkspaceChanged?: (selection: {
    readonly organizationId: string | null;
    readonly organizationName: string | null;
    readonly workspaceId: string | null;
    readonly myRole: OrgRole | null;
    readonly syncStatus: DesktopWorkspaceSyncStatus | null;
  }) => void;
}

export function OrganizationWorkspacePanel({
  lang = "en",
  sessionToken,
  desktopRuntime = false,
  readOnlyOffline = false,
  apiBaseUrlOverride,
  onRequestSignIn,
  onOpenBillingPortal,
  onBackToShowcase,
  onCloseModal,
  onActiveWorkspaceChanged
}: OrganizationWorkspacePanelProps): ReactNode {
  const isAr = lang === "ar";
  const L = useCallback((ar: string, en: string) => (isAr ? ar : en), [isAr]);

  const [inviteTokenInput, setInviteTokenInput] = useState<string>(
    consumeInitialInviteTokenFromUrl
  );
  const [busy, setBusy] = useState(false);
  const [banner, setBanner] = useState<{
    readonly kind: "success" | "error";
    readonly text: string;
  } | null>(null);

  const [personalWorkspaceId, setPersonalWorkspaceId] = useState<string | null>(null);
  const [verifiedEmail, setVerifiedEmail] = useState<string | null>(null);
  const [organizations, setOrganizations] = useState<readonly EnrichedOrganizationSummary[]>([]);
  const [pendingUserInvites, setPendingUserInvites] = useState<readonly PendingUserInvitation[]>(
    []
  );
  const [selectedOrgId, setSelectedOrgId] = useState<string | null>(null);

  const [members, setMembers] = useState<readonly OrganizationMemberRecord[]>([]);
  const [invitations, setInvitations] = useState<readonly OrganizationInvitationView[]>([]);
  const [createdInvite, setCreatedInvite] = useState<CreatedInvitationResponse | null>(null);

  // Create org form
  const [newOrgName, setNewOrgName] = useState("");
  const [newOrgSeats, setNewOrgSeats] = useState<number>(
    BUSINESS_PRICING.organizationSeat.minSeats
  );

  // Invite member form
  const [inviteEmail, setInviteEmail] = useState("");
  const [inviteRole, setInviteRole] = useState<OrgRole>("member");

  // Desktop E2EE Sync state
  const [syncStatus, setSyncStatus] = useState<DesktopWorkspaceSyncStatus | null>(null);
  const [sharedResources, setSharedResources] = useState<readonly DesktopSharedWorkspaceResource[]>(
    []
  );
  const [conflicts, setConflicts] = useState<readonly DesktopSyncConflictItem[]>([]);
  const [resKind, setResKind] = useState<"recipe" | "project" | "dashboard" | "document">("recipe");
  const [resId, setResId] = useState("shared-cleaning-01");
  const [resTitle, setResTitle] = useState("");
  const [resContent, setResContent] = useState('{"steps":["trim_whitespace"]}');

  const selectedOrg = organizations.find((o) => o.organization.id === selectedOrgId) ?? null;

  const refreshLocalDesktopState = useCallback(
    async (workspaceId: string | null) => {
      if (!desktopRuntime) return;
      try {
        const st = await getDesktopSyncStatus(workspaceId);
        setSyncStatus(st);
        if (workspaceId) {
          const [resList, confList] = await Promise.all([
            listDesktopSyncedResources(workspaceId, false),
            listDesktopSyncConflicts(workspaceId)
          ]);
          setSharedResources(resList);
          setConflicts(confList);
        } else {
          setSharedResources([]);
          setConflicts([]);
        }
      } catch {
        // Keep last state when unauthenticated.
      }
    },
    [desktopRuntime]
  );

  const loadSelectedOrgDetails = useCallback(
    async (orgSummary: EnrichedOrganizationSummary, token: string) => {
      const orgId = orgSummary.organization.id;
      const wsId = orgSummary.organization.workspace_id;
      const memberList = await fetchOrganizationMembers(token, orgId, apiBaseUrlOverride);
      setMembers(memberList);

      if (orgSummary.my_role === "owner" || orgSummary.my_role === "admin") {
        const invList = await fetchOrganizationInvitations(token, orgId, apiBaseUrlOverride);
        setInvitations(invList);
      } else {
        setInvitations([]);
      }

      if (desktopRuntime && !readOnlyOffline) {
        try {
          const st = await synchronizeDesktopOrganizationWorkspace({
            sessionToken: token,
            organizationId: orgId,
            workspaceId: wsId,
            activeKeyEpoch: orgSummary.organization.active_key_epoch,
            myRole: orgSummary.my_role,
            ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
          });
          setSyncStatus(st);
        } catch {
          // Offline fallback: load local encrypted vault state cleanly!
        }
        await refreshLocalDesktopState(wsId);
      }
    },
    [apiBaseUrlOverride, desktopRuntime, readOnlyOffline, refreshLocalDesktopState]
  );

  const refreshAll = useCallback(
    async (preferredOrgId?: string | null) => {
      if (!sessionToken) return;
      setBusy(true);
      try {
        const mine = await fetchMyOrganizations(sessionToken, apiBaseUrlOverride);
        setPersonalWorkspaceId(mine.personal_workspace_id);
        setVerifiedEmail(mine.verified_email);
        setOrganizations(mine.organizations);
        setPendingUserInvites(mine.pending_invitations);

        const targetId =
          preferredOrgId !== undefined
            ? preferredOrgId
            : selectedOrgId && mine.organizations.some((o) => o.organization.id === selectedOrgId)
              ? selectedOrgId
              : (mine.organizations[0]?.organization.id ?? null);
        setSelectedOrgId(targetId);

        const chosen = mine.organizations.find((o) => o.organization.id === targetId) ?? null;
        if (chosen) {
          await loadSelectedOrgDetails(chosen, sessionToken);
        } else {
          setMembers([]);
          setInvitations([]);
          if (desktopRuntime) {
            const st = await selectDesktopSyncWorkspace(null, null, 1);
            setSyncStatus(st);
            setSharedResources([]);
            setConflicts([]);
          }
        }
      } catch (err) {
        setBanner({ kind: "error", text: describeOrgError(err, isAr) });
      } finally {
        setBusy(false);
      }
    },
    [apiBaseUrlOverride, desktopRuntime, isAr, loadSelectedOrgDetails, selectedOrgId, sessionToken]
  );

  useEffect(() => {
    if (sessionToken) {
      void refreshAll();
    } else if (desktopRuntime && readOnlyOffline) {
      void getDesktopSyncStatus(null)
        .then(async (st) => {
          setSyncStatus(st);
          if (st.active_workspace_id) {
            setSharedResources(await listDesktopSyncedResources(st.active_workspace_id, false));
            setConflicts(await listDesktopSyncConflicts(st.active_workspace_id));
          }
        })
        .catch(() => undefined);
    }
  }, [desktopRuntime, readOnlyOffline, refreshAll, sessionToken]);

  useEffect(() => {
    if (!onActiveWorkspaceChanged) return;
    onActiveWorkspaceChanged({
      organizationId: selectedOrg?.organization.id ?? null,
      organizationName: selectedOrg?.organization.name ?? null,
      workspaceId: selectedOrg?.organization.workspace_id ?? null,
      myRole: selectedOrg?.my_role ?? null,
      syncStatus
    });
  }, [onActiveWorkspaceChanged, selectedOrg, syncStatus]);

  const handleSelectWorkspace = async (orgId: string | null): Promise<void> => {
    setSelectedOrgId(orgId);
    setCreatedInvite(null);
    if (!orgId) {
      setMembers([]);
      setInvitations([]);
      if (desktopRuntime) {
        const st = await selectDesktopSyncWorkspace(null, null, 1);
        setSyncStatus(st);
        setSharedResources([]);
        setConflicts([]);
      }
      return;
    }
    if (sessionToken) {
      const found = organizations.find((o) => o.organization.id === orgId);
      if (found) {
        setBusy(true);
        try {
          await loadSelectedOrgDetails(found, sessionToken);
        } catch (err) {
          setBanner({ kind: "error", text: describeOrgError(err, isAr) });
        } finally {
          setBusy(false);
        }
      }
    }
  };

  const handleCreateOrganization = async (e: FormEvent): Promise<void> => {
    e.preventDefault();
    if (!sessionToken || !newOrgName.trim()) return;
    setBusy(true);
    setBanner(null);
    try {
      const created = await createOrganization(
        sessionToken,
        {
          name: newOrgName.trim(),
          seats: normalizeOrganizationSeats(newOrgSeats)
        },
        apiBaseUrlOverride
      );
      setNewOrgName("");
      setBanner({
        kind: "success",
        text: L(
          `تم إنشاء المؤسسة «${created.name}» ومساحة العمل المشتركة بنجاح.`,
          `Organization "${created.name}" and its shared workspace are ready.`
        )
      });
      await refreshAll(created.id);
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleInviteMember = async (e: FormEvent): Promise<void> => {
    e.preventDefault();
    if (!sessionToken || !selectedOrg || !inviteEmail.trim()) return;
    setBusy(true);
    setBanner(null);
    try {
      const res = await inviteOrganizationMember(
        sessionToken,
        selectedOrg.organization.id,
        { email: inviteEmail.trim(), role: inviteRole },
        apiBaseUrlOverride
      );
      setCreatedInvite(res);
      setInviteEmail("");
      setBanner({
        kind: "success",
        text: L(
          "تم إصدار دعوة آمنة مرتبطة بالبريد الموثق لمرة واحدة.",
          "Single-use verified-email invitation created."
        )
      });
      await refreshAll(selectedOrg.organization.id);
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleAcceptInvite = async (opts: {
    readonly token?: string;
    readonly invitation_id?: string;
  }): Promise<void> => {
    if (!sessionToken) return;
    setBusy(true);
    setBanner(null);
    try {
      const joined = await acceptOrganizationInvitation(sessionToken, opts, apiBaseUrlOverride);
      clearSavedInviteToken();
      setInviteTokenInput("");
      setBanner({
        kind: "success",
        text: L(
          "تم قبول الدعوة والانضمام إلى مساحة العمل المشتركة بنجاح.",
          "Invitation accepted — you have joined the shared team workspace."
        )
      });
      await refreshAll(joined.organization_id);
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleDeclineInvite = async (opts: {
    readonly token?: string;
    readonly invitation_id?: string;
  }): Promise<void> => {
    if (!sessionToken) return;
    setBusy(true);
    setBanner(null);
    try {
      await declineOrganizationInvitation(sessionToken, opts, apiBaseUrlOverride);
      clearSavedInviteToken();
      setInviteTokenInput("");
      setBanner({
        kind: "success",
        text: L("تم رفض الدعوة.", "Invitation declined.")
      });
      await refreshAll();
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleRevokeInvite = async (inviteId: string): Promise<void> => {
    if (!sessionToken || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      await revokeOrganizationInvitation(
        sessionToken,
        selectedOrg.organization.id,
        inviteId,
        apiBaseUrlOverride
      );
      setBanner({
        kind: "success",
        text: L("تم إلغاء الدعوة فورًا.", "Invitation revoked immediately.")
      });
      await refreshAll(selectedOrg.organization.id);
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleChangeRole = async (targetUserId: string, nextRole: OrgRole): Promise<void> => {
    if (!sessionToken || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      await changeOrganizationMemberRole(
        sessionToken,
        selectedOrg.organization.id,
        targetUserId,
        nextRole,
        apiBaseUrlOverride
      );
      setBanner({
        kind: "success",
        text: L("تم تحديث صلاحية العضو بنجاح.", "Member role updated.")
      });
      await refreshAll(selectedOrg.organization.id);
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleRemoveMember = async (targetUserId: string): Promise<void> => {
    if (!sessionToken || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      const res = await removeMemberAndRotateDesktopKey({
        sessionToken,
        organizationId: selectedOrg.organization.id,
        workspaceId: selectedOrg.organization.workspace_id,
        currentKeyEpoch: selectedOrg.organization.active_key_epoch,
        targetUserId,
        desktopRuntime,
        ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
      });
      setBanner({
        kind: "success",
        text:
          res.rotatedEpoch !== null
            ? L(
                `تمت إزالة العضو وتدوير مفتاح التشفير إلى الجيل #${res.rotatedEpoch} لحماية البيانات المستقبلية.`,
                `Member removed and workspace encryption key rotated to Epoch #${res.rotatedEpoch} for future revisions.`
              )
            : L("تمت إزالة العضو وإبطال مفاتيحه.", "Member removed and device keys revoked.")
      });
      await refreshAll(selectedOrg.organization.id);
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleTransferOwnership = async (newOwnerUserId: string): Promise<void> => {
    if (!sessionToken || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      await transferOrganizationOwnership(
        sessionToken,
        selectedOrg.organization.id,
        newOwnerUserId,
        apiBaseUrlOverride
      );
      setBanner({
        kind: "success",
        text: L("تم نقل ملكية المؤسسة بنجاح.", "Organization ownership transferred.")
      });
      await refreshAll(selectedOrg.organization.id);
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteOrganization = async (): Promise<void> => {
    if (!sessionToken || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      await deleteOrganization(sessionToken, selectedOrg.organization.id, apiBaseUrlOverride);
      setBanner({
        kind: "success",
        text: L("تم حذف المؤسسة وإبطال جميع الدعوات والمفاتيح.", "Organization deleted.")
      });
      await refreshAll(null);
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleUpgradeSeatsOrPlan = async (targetSeats: number): Promise<void> => {
    const seats = normalizeOrganizationSeats(targetSeats);
    if (desktopRuntime) {
      if (!sessionToken) return;
      setBusy(true);
      setBanner(null);
      try {
        await launchDesktopWebsiteBilling(
          sessionToken,
          {
            product: "organization_seat_annual",
            seats,
            billing_country_iso2: "US"
          },
          apiBaseUrlOverride !== undefined ? { apiBaseUrl: apiBaseUrlOverride } : undefined
        );
        setBanner({
          kind: "success",
          text: L(
            "تم فتح بوابة الدفع الرسمية (/billing) في متصفحك لإتمام الترقية بأمان.",
            "Opened the official /billing portal in your browser to complete seat upgrade securely."
          )
        });
      } catch (err) {
        setBanner({ kind: "error", text: describeOrgError(err, isAr) });
      } finally {
        setBusy(false);
      }
      return;
    }
    onOpenBillingPortal?.(seats);
  };

  const handleDesktopSyncNow = async (): Promise<void> => {
    if (!desktopRuntime || !sessionToken || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      const st = await synchronizeDesktopOrganizationWorkspace({
        sessionToken,
        organizationId: selectedOrg.organization.id,
        workspaceId: selectedOrg.organization.workspace_id,
        activeKeyEpoch: selectedOrg.organization.active_key_epoch,
        myRole: selectedOrg.my_role,
        ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
      });
      setSyncStatus(st);
      await refreshLocalDesktopState(selectedOrg.organization.workspace_id);
      setBanner({
        kind: "success",
        text: L(
          "اكتملت المزامنة المشفّرة طرفًا لطرف بنجاح.",
          "End-to-end encrypted sync completed."
        )
      });
    } catch (err) {
      await refreshLocalDesktopState(selectedOrg.organization.workspace_id);
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleStageAndSyncResource = async (e: FormEvent): Promise<void> => {
    e.preventDefault();
    if (!desktopRuntime || !selectedOrg || !resTitle.trim() || !resId.trim()) return;
    setBusy(true);
    setBanner(null);
    try {
      await stageDesktopSharedResourceUpsert({
        organizationId: selectedOrg.organization.id,
        workspaceId: selectedOrg.organization.workspace_id,
        resourceKind: resKind,
        resourceId: resId.trim(),
        title: resTitle.trim(),
        payloadJson: resContent
      });
      await refreshLocalDesktopState(selectedOrg.organization.workspace_id);
      if (sessionToken && !readOnlyOffline) {
        await synchronizeDesktopOrganizationWorkspace({
          sessionToken,
          organizationId: selectedOrg.organization.id,
          workspaceId: selectedOrg.organization.workspace_id,
          activeKeyEpoch: selectedOrg.organization.active_key_epoch,
          myRole: selectedOrg.my_role,
          ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
        });
        await refreshLocalDesktopState(selectedOrg.organization.workspace_id);
        setBanner({
          kind: "success",
          text: L(
            "تم تشفير المورد محليًا ومزامنته مع مساحة عمل الفريق.",
            "Resource encrypted locally on Desktop and synced to the shared workspace."
          )
        });
      } else {
        setBanner({
          kind: "success",
          text: L(
            "تم حفظ التغيير مشفّرًا في قائمة الانتظار دون إنترنت — سيُرفع عند عودة الاتصال.",
            "Saved encrypted in local offline queue — will sync when online."
          )
        });
      }
      setResTitle("");
    } catch (err) {
      await refreshLocalDesktopState(selectedOrg.organization.workspace_id);
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteSharedResource = async (kind: string, id: string): Promise<void> => {
    if (!desktopRuntime || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      await stageDesktopSharedResourceDelete({
        organizationId: selectedOrg.organization.id,
        workspaceId: selectedOrg.organization.workspace_id,
        resourceKind: kind,
        resourceId: id
      });
      if (sessionToken && !readOnlyOffline) {
        await synchronizeDesktopOrganizationWorkspace({
          sessionToken,
          organizationId: selectedOrg.organization.id,
          workspaceId: selectedOrg.organization.workspace_id,
          activeKeyEpoch: selectedOrg.organization.active_key_epoch,
          myRole: selectedOrg.my_role,
          ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
        });
      }
      await refreshLocalDesktopState(selectedOrg.organization.workspace_id);
    } catch (err) {
      await refreshLocalDesktopState(selectedOrg.organization.workspace_id);
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleResolveConflict = async (
    conflict: DesktopSyncConflictItem,
    choice: "keep_local" | "accept_remote"
  ): Promise<void> => {
    if (!desktopRuntime || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      await resolveDesktopSyncConflict(
        conflict.workspace_id,
        conflict.resource_kind,
        conflict.resource_id,
        choice
      );
      if (choice === "keep_local" && sessionToken && !readOnlyOffline) {
        await synchronizeDesktopOrganizationWorkspace({
          sessionToken,
          organizationId: selectedOrg.organization.id,
          workspaceId: selectedOrg.organization.workspace_id,
          activeKeyEpoch: selectedOrg.organization.active_key_epoch,
          myRole: selectedOrg.my_role,
          ...(apiBaseUrlOverride !== undefined ? { apiBaseUrlOverride } : {})
        });
      }
      await refreshLocalDesktopState(selectedOrg.organization.workspace_id);
      setBanner({
        kind: "success",
        text: L("تم حسم التعارض بنجاح.", "Sync conflict resolved.")
      });
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleManualKeyRotation = async (): Promise<void> => {
    if (!desktopRuntime || !sessionToken || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      const devices = await fetchOrganizationDeviceKeys(
        sessionToken,
        selectedOrg.organization.id,
        apiBaseUrlOverride
      );
      const prepared = await prepareDesktopWorkspaceKeyRotation(
        selectedOrg.organization.id,
        selectedOrg.organization.workspace_id,
        selectedOrg.organization.active_key_epoch,
        devices
      );
      const res = await rotateOrganizationWorkspaceKey(
        sessionToken,
        selectedOrg.organization.id,
        {
          expected_current_epoch: selectedOrg.organization.active_key_epoch,
          entries: prepared.entries
        },
        apiBaseUrlOverride
      );
      setBanner({
        kind: "success",
        text: L(
          `تم تدوير مفتاح مساحة العمل إلى الجيل #${res.active_key_epoch}.`,
          `Rotated workspace encryption key to Epoch #${res.active_key_epoch}.`
        )
      });
      await refreshAll(selectedOrg.organization.id);
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  const handleRotateDeviceKey = async (): Promise<void> => {
    if (!desktopRuntime || !sessionToken || !selectedOrg) return;
    setBusy(true);
    setBanner(null);
    try {
      const rotated = await rotateDesktopLocalDeviceKey(null);
      await registerOrganizationDeviceKey(
        sessionToken,
        selectedOrg.organization.id,
        {
          device_id: rotated.device_id,
          device_label: rotated.device_label,
          x25519_public_key_b64: rotated.x25519_public_key_b64
        },
        apiBaseUrlOverride
      );
      await refreshLocalDesktopState(selectedOrg.organization.workspace_id);
      setBanner({
        kind: "success",
        text: L(
          "تم توليد مفتاح جهاز X25519 جديد محليًا وتسجيل المفتاح العام.",
          "Generated a new local X25519 device keypair and registered its public key."
        )
      });
    } catch (err) {
      setBanner({ kind: "error", text: describeOrgError(err, isAr) });
    } finally {
      setBusy(false);
    }
  };

  // Unauthenticated state (Website `/team` or `/team?invite=sfi_...`)
  if (!sessionToken && !(desktopRuntime && readOnlyOffline)) {
    return (
      <section
        className="safi-panel"
        data-org-workspace-panel="unauthenticated"
        dir={isAr ? "rtl" : "ltr"}
        style={{
          maxWidth: "760px",
          margin: "24px auto",
          padding: "clamp(20px, 3vw, 34px)",
          borderRadius: "20px",
          display: "grid",
          gap: "18px"
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px"
          }}
        >
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "8px",
              fontWeight: 700,
              fontSize: "14px"
            }}
          >
            <Building2 size={18} aria-hidden="true" />
            {L("مساحة عمل المؤسسة والفرق", "Team & Organization Workspace")}
          </span>
          {onBackToShowcase ? (
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              onClick={onBackToShowcase}
              style={{ minHeight: "44px" }}
            >
              <ArrowLeft size={16} aria-hidden="true" />
              {L("العودة للموقع", "Back to Website")}
            </button>
          ) : null}
        </div>

        <h1 style={{ fontSize: "clamp(22px, 3vw, 28px)", margin: 0 }}>
          {inviteTokenInput
            ? L(
                "لديك دعوة للانضمام إلى فريق على صافي البيانات",
                "You have an invitation to join a SafiData team"
              )
            : L(
                "أدِر فريقك ومساحة العمل المشتركة والصلاحيات",
                "Manage your shared team workspace, roles, and invitations"
              )}
        </h1>

        <p style={{ margin: 0, opacity: 0.85, lineHeight: 1.6 }}>
          {inviteTokenInput
            ? L(
                "لحماية خصوصية المؤسسة، لا تُعرض بيانات الفريق قبل التحقق من الهوية. سجّل الدخول أو أنشئ حسابًا ووثّق نفس البريد الإلكتروني الذي أُرسلت إليه الدعوة لقبولها.",
                "For tenant security, organization details are never disclosed before authentication. Sign in or register and verify the exact email address that received this invitation."
              )
            : L(
                "سجّل الدخول بحسابك الموثق لإنشاء مؤسسة، إدارة الأعضاء والأدوار الأربعة (Owner / Admin / Member / Viewer)، أو قبول دعوات الفريق.",
                "Sign in with your verified account to create an organization, manage members across all 4 RBAC roles (Owner / Admin / Member / Viewer), or accept team invitations."
              )}
        </p>

        <div style={{ display: "flex", flexWrap: "wrap", gap: "12px" }}>
          {onRequestSignIn ? (
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              data-testid="org-signin-cta"
              onClick={onRequestSignIn}
              style={{ minHeight: "44px" }}
            >
              <ShieldCheck size={16} aria-hidden="true" />
              {L("تسجيل الدخول أو إنشاء حساب موثق", "Sign In or Create Verified Account")}
            </button>
          ) : null}
          {onOpenBillingPortal ? (
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              data-testid="org-subscribe-cta"
              onClick={() => onOpenBillingPortal(BUSINESS_PRICING.organizationSeat.minSeats)}
              style={{ minHeight: "44px" }}
            >
              <CreditCard size={16} aria-hidden="true" />
              {L(
                "اشترك في خطة المؤسسات (من 3 مقاعد)",
                "Subscribe to Organization Plan (min 3 seats)"
              )}
            </button>
          ) : null}
        </div>
      </section>
    );
  }

  const canManageMembers =
    selectedOrg !== null && (selectedOrg.my_role === "owner" || selectedOrg.my_role === "admin");
  const isOrgOwner = selectedOrg !== null && selectedOrg.my_role === "owner";
  const canModifyResources = selectedOrg !== null && selectedOrg.my_role !== "viewer";
  const usedPlusPendingSeats = selectedOrg
    ? selectedOrg.active_members_count + selectedOrg.pending_invitations_count
    : 0;

  return (
    <section
      className="safi-panel"
      data-org-workspace-panel="authenticated"
      dir={isAr ? "rtl" : "ltr"}
      style={{
        maxWidth: "1140px",
        margin: "12px auto 36px",
        padding: "clamp(16px, 2.5vw, 28px)",
        borderRadius: "20px",
        display: "grid",
        gap: "20px"
      }}
    >
      {/* Top Bar */}
      <header
        style={{
          display: "flex",
          flexWrap: "wrap",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "12px"
        }}
      >
        <div style={{ display: "grid", gap: "4px" }}>
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <Building2 size={20} aria-hidden="true" />
            <h2 style={{ margin: 0, fontSize: "20px" }}>
              {L(
                "مساحة عمل الفريق والمزامنة المشفّرة (E2EE)",
                "Shared Team Workspace, RBAC & Encrypted Sync"
              )}
            </h2>
          </div>
          {verifiedEmail ? (
            <span style={{ fontSize: "13px", opacity: 0.78 }}>
              {L("الحساب الموثق:", "Verified account:")} <strong dir="ltr">{verifiedEmail}</strong>
            </span>
          ) : null}
        </div>

        <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
          {sessionToken ? (
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              data-testid="org-refresh-btn"
              disabled={busy}
              onClick={() => void refreshAll()}
              style={{ minHeight: "44px" }}
            >
              <RefreshCw size={15} aria-hidden="true" />
              {L("تحديث", "Refresh")}
            </button>
          ) : null}
          {onBackToShowcase ? (
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              onClick={onBackToShowcase}
              style={{ minHeight: "44px" }}
            >
              <ArrowLeft size={15} aria-hidden="true" />
              {L("الرئيسية", "Home")}
            </button>
          ) : null}
          {onCloseModal ? (
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              data-testid="org-close-modal-btn"
              onClick={onCloseModal}
              style={{ minHeight: "44px" }}
            >
              <X size={16} aria-hidden="true" />
              {L("إغلاق", "Close")}
            </button>
          ) : null}
        </div>
      </header>

      {/* Status / Error Banner */}
      {banner ? (
        <div
          role={banner.kind === "error" ? "alert" : "status"}
          data-testid="org-status-banner"
          style={{
            padding: "12px 16px",
            borderRadius: "12px",
            border:
              banner.kind === "error"
                ? "1px solid var(--safi-danger, #ef4444)"
                : "1px solid var(--status-ok, #10b981)",
            background:
              banner.kind === "error" ? "rgba(239, 68, 68, 0.12)" : "rgba(16, 185, 129, 0.12)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px"
          }}
        >
          <span>{banner.text}</span>
          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            onClick={() => setBanner(null)}
            aria-label={L("إغلاق التنبيه", "Dismiss notice")}
          >
            <X size={14} aria-hidden="true" />
          </button>
        </div>
      ) : null}

      {/* Pending Invitations Addressed to Caller's Verified Email + Token Redemption */}
      {pendingUserInvites.length > 0 || inviteTokenInput ? (
        <div
          className="safi-panel"
          data-testid="org-pending-user-invites"
          style={{
            padding: "16px",
            borderRadius: "14px",
            border: "1px solid rgba(56, 189, 248, 0.35)",
            display: "grid",
            gap: "12px"
          }}
        >
          <strong style={{ fontSize: "15px" }}>
            {L("دعوات الانضمام الواردة لحسابك الموثق", "Incoming Team Invitations")}
          </strong>

          {pendingUserInvites.map((inv) => (
            <div
              key={inv.invitation_id}
              data-testid={`incoming-invite-${inv.invitation_id}`}
              style={{
                display: "flex",
                flexWrap: "wrap",
                alignItems: "center",
                justifyContent: "space-between",
                gap: "10px",
                padding: "10px 12px",
                borderRadius: "10px",
                background: "rgba(255,255,255,0.03)"
              }}
            >
              <div>
                <strong>{inv.organization_name}</strong> ·{" "}
                <span>
                  {L("الدور:", "Role:")} <code dir="ltr">{inv.role}</code>
                </span>
              </div>
              <div style={{ display: "flex", gap: "8px" }}>
                <button
                  type="button"
                  className="safi-btn safi-btn-primary"
                  data-testid={`accept-invite-${inv.invitation_id}`}
                  disabled={busy}
                  onClick={() => void handleAcceptInvite({ invitation_id: inv.invitation_id })}
                  style={{ minHeight: "44px" }}
                >
                  <Check size={15} aria-hidden="true" />
                  {L("قبول الدعوة", "Accept")}
                </button>
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  data-testid={`decline-invite-${inv.invitation_id}`}
                  disabled={busy}
                  onClick={() => void handleDeclineInvite({ invitation_id: inv.invitation_id })}
                  style={{ minHeight: "44px" }}
                >
                  {L("رفض", "Decline")}
                </button>
              </div>
            </div>
          ))}

          <div style={{ display: "flex", flexWrap: "wrap", gap: "8px", alignItems: "center" }}>
            <input
              type="text"
              dir="ltr"
              value={inviteTokenInput}
              onChange={(e) => setInviteTokenInput(e.target.value)}
              placeholder="sfi_..."
              aria-label={L("رمز الدعوة", "Invitation token")}
              data-testid="org-invite-token-input"
              style={{
                flex: "1 1 240px",
                minHeight: "44px",
                padding: "8px 12px",
                borderRadius: "10px",
                background: "rgba(0,0,0,0.2)",
                border: "1px solid rgba(255,255,255,0.14)",
                color: "inherit"
              }}
            />
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              data-testid="org-accept-token-btn"
              disabled={busy || !inviteTokenInput.trim()}
              onClick={() => void handleAcceptInvite({ token: inviteTokenInput.trim() })}
              style={{ minHeight: "44px" }}
            >
              {L("قبول بالرمز", "Accept Token")}
            </button>
            {inviteTokenInput.trim() ? (
              <button
                type="button"
                className="safi-btn safi-btn-ghost"
                data-testid="org-decline-token-btn"
                disabled={busy}
                onClick={() => void handleDeclineInvite({ token: inviteTokenInput.trim() })}
                style={{ minHeight: "44px" }}
              >
                {L("رفض بالرمز", "Decline Token")}
              </button>
            ) : null}
          </div>
        </div>
      ) : (
        <details style={{ fontSize: "14px" }}>
          <summary style={{ cursor: "pointer", minHeight: "36px" }}>
            {L("هل لديك رمز دعوة (sfi_...)؟", "Have an invitation token (sfi_...)?")}
          </summary>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "8px", marginTop: "10px" }}>
            <input
              type="text"
              dir="ltr"
              value={inviteTokenInput}
              onChange={(e) => setInviteTokenInput(e.target.value)}
              placeholder="sfi_..."
              data-testid="org-invite-token-input"
              style={{
                flex: "1 1 240px",
                minHeight: "44px",
                padding: "8px 12px",
                borderRadius: "10px",
                background: "rgba(0,0,0,0.2)",
                border: "1px solid rgba(255,255,255,0.14)",
                color: "inherit"
              }}
            />
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              data-testid="org-accept-token-btn"
              disabled={busy || !inviteTokenInput.trim()}
              onClick={() => void handleAcceptInvite({ token: inviteTokenInput.trim() })}
              style={{ minHeight: "44px" }}
            >
              {L("قبول الدعوة", "Accept Invitation")}
            </button>
          </div>
        </details>
      )}

      {/* Workspace Switcher + Create Organization */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(300px, 1fr))",
          gap: "16px"
        }}
      >
        <div
          className="safi-panel"
          style={{ padding: "16px", borderRadius: "14px", display: "grid", gap: "10px" }}
        >
          <strong>{L("مساحة العمل النشطة", "Active Workspace Switcher")}</strong>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
            <button
              type="button"
              className={
                selectedOrgId === null ? "safi-btn safi-btn-primary" : "safi-btn safi-btn-ghost"
              }
              data-testid="switch-personal-workspace"
              onClick={() => void handleSelectWorkspace(null)}
              style={{ minHeight: "44px" }}
            >
              {L("مساحة العمل الشخصية", "Personal Workspace")}
              {personalWorkspaceId ? (
                <span
                  dir="ltr"
                  style={{ fontSize: "11px", opacity: 0.7, marginInlineStart: "6px" }}
                >
                  ({personalWorkspaceId.slice(0, 8)})
                </span>
              ) : null}
            </button>

            {organizations.map((item) => (
              <button
                key={item.organization.id}
                type="button"
                className={
                  selectedOrgId === item.organization.id
                    ? "safi-btn safi-btn-primary"
                    : "safi-btn safi-btn-ghost"
                }
                data-testid={`switch-org-workspace-${item.organization.id}`}
                onClick={() => void handleSelectWorkspace(item.organization.id)}
                style={{ minHeight: "44px" }}
              >
                <Users size={15} aria-hidden="true" />
                <span>{item.organization.name}</span>
                <span
                  dir="ltr"
                  style={{ fontSize: "11px", opacity: 0.8, marginInlineStart: "6px" }}
                >
                  [{item.my_role}]
                </span>
              </button>
            ))}
          </div>
        </div>

        {/* Create Organization Form */}
        {sessionToken ? (
          <form
            onSubmit={(e) => void handleCreateOrganization(e)}
            className="safi-panel"
            data-testid="create-org-form"
            style={{ padding: "16px", borderRadius: "14px", display: "grid", gap: "10px" }}
          >
            <strong>{L("إنشاء مؤسسة جديدة (خطة الفرق)", "Create New Organization")}</strong>
            <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
              <input
                type="text"
                required
                maxLength={120}
                value={newOrgName}
                onChange={(e) => setNewOrgName(e.target.value)}
                placeholder={L("اسم المؤسسة أو الفريق", "Organization or Team Name")}
                data-testid="create-org-name-input"
                style={{
                  flex: "2 1 180px",
                  minHeight: "44px",
                  padding: "8px 12px",
                  borderRadius: "10px",
                  background: "rgba(0,0,0,0.2)",
                  border: "1px solid rgba(255,255,255,0.14)",
                  color: "inherit"
                }}
              />
              <input
                type="number"
                min={BUSINESS_PRICING.organizationSeat.minSeats}
                max={500}
                value={newOrgSeats}
                onChange={(e) => setNewOrgSeats(Number(e.target.value) || 3)}
                aria-label={L("عدد المقاعد", "Seats")}
                data-testid="create-org-seats-input"
                style={{
                  width: "92px",
                  minHeight: "44px",
                  padding: "8px 10px",
                  borderRadius: "10px",
                  background: "rgba(0,0,0,0.2)",
                  border: "1px solid rgba(255,255,255,0.14)",
                  color: "inherit"
                }}
              />
              <button
                type="submit"
                className="safi-btn safi-btn-primary"
                data-testid="create-org-submit-btn"
                disabled={busy || !newOrgName.trim()}
                style={{ minHeight: "44px" }}
              >
                {L("إنشاء", "Create")}
              </button>
            </div>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                fontSize: "12px",
                opacity: 0.8
              }}
            >
              <span>
                {L(
                  "يتطلب اشتراك Organization فعّال (الحد الأدنى 3 مقاعد · 17$/850 ج.م للمقعد شهريًا).",
                  "Requires an active Organization plan (min 3 seats · $17 / 850 EGP per seat/mo)."
                )}
              </span>
              <button
                type="button"
                className="safi-btn safi-btn-ghost"
                data-testid="org-buy-plan-btn"
                onClick={() => void handleUpgradeSeatsOrPlan(newOrgSeats)}
              >
                <CreditCard size={14} aria-hidden="true" />
                {L("شراء / ترقية", "Buy / Upgrade")}
              </button>
            </div>
          </form>
        ) : null}
      </div>

      {/* Selected Shared Organization Workspace Details */}
      {selectedOrg ? (
        <div data-testid="selected-org-dashboard" style={{ display: "grid", gap: "18px" }}>
          {/* Summary & Subscription Card */}
          <div
            className="safi-panel"
            style={{
              padding: "18px",
              borderRadius: "14px",
              display: "flex",
              flexWrap: "wrap",
              alignItems: "center",
              justifyContent: "space-between",
              gap: "14px"
            }}
          >
            <div style={{ display: "grid", gap: "4px" }}>
              <div style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: "10px" }}>
                <h3 style={{ margin: 0, fontSize: "18px" }}>{selectedOrg.organization.name}</h3>
                <span
                  data-testid="selected-org-my-role"
                  style={{
                    padding: "3px 10px",
                    borderRadius: "999px",
                    fontSize: "12px",
                    fontWeight: 700,
                    background: "rgba(56, 189, 248, 0.16)"
                  }}
                >
                  {L("دورك:", "Your role:")} {selectedOrg.my_role.toUpperCase()}
                </span>
                <span
                  data-testid="selected-org-sub-state"
                  style={{
                    padding: "3px 10px",
                    borderRadius: "999px",
                    fontSize: "12px",
                    fontWeight: 700,
                    background: selectedOrg.teams_enabled
                      ? "rgba(16, 185, 129, 0.16)"
                      : "rgba(239, 68, 68, 0.16)"
                  }}
                >
                  {selectedOrg.teams_enabled
                    ? L("الاشتراك نشط", "Subscription Active")
                    : L("الاشتراك غير نشط / منتهي", "Subscription Inactive")}
                </span>
              </div>
              <div style={{ fontSize: "12px", opacity: 0.78 }}>
                {L("معرّف مساحة العمل المشتركة:", "Shared Workspace ID:")}{" "}
                <code data-testid="selected-org-workspace-id" dir="ltr">
                  {selectedOrg.organization.workspace_id}
                </code>{" "}
                · {L("جيل مفتاح التشفير:", "Key Epoch:")}{" "}
                <strong dir="ltr">#{selectedOrg.organization.active_key_epoch}</strong>
              </div>
            </div>

            <div style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: "12px" }}>
              <div data-testid="selected-org-seat-usage" style={{ fontSize: "14px" }}>
                <strong>
                  {usedPlusPendingSeats} / {selectedOrg.paid_seats}
                </strong>{" "}
                {L(
                  `مقاعد مستخدمة (${selectedOrg.active_members_count} أعضاء + ${selectedOrg.pending_invitations_count} دعوات معلقة)`,
                  `seats reserved (${selectedOrg.active_members_count} active + ${selectedOrg.pending_invitations_count} pending)`
                )}
              </div>

              {isOrgOwner ? (
                <button
                  type="button"
                  className="safi-btn safi-btn-primary"
                  data-testid="org-owner-upgrade-seats-btn"
                  disabled={busy}
                  onClick={() => void handleUpgradeSeatsOrPlan(selectedOrg.paid_seats + 2)}
                  style={{ minHeight: "44px" }}
                >
                  <CreditCard size={15} aria-hidden="true" />
                  {L("إدارة الاشتراك / شراء مقاعد", "Manage Plan / Add Seats")}
                </button>
              ) : (
                <span style={{ fontSize: "12px", opacity: 0.75 }}>
                  {L(
                    "إدارة الاشتراك والمقاعد متاحة لمالك المؤسسة (Owner) فقط.",
                    "Subscription & seat billing can only be modified by the Organization Owner."
                  )}
                </span>
              )}
            </div>
          </div>

          {/* Members & RBAC Section */}
          <div
            className="safi-panel"
            data-testid="org-members-section"
            style={{ padding: "18px", borderRadius: "14px", display: "grid", gap: "12px" }}
          >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
              <strong>
                {L("أعضاء مساحة العمل المشتركة والصلاحيات (RBAC)", "Workspace Members & RBAC")}
              </strong>
              {!canManageMembers ? (
                <span style={{ fontSize: "12px", opacity: 0.75 }}>
                  {L(
                    "لديك صلاحية عرض الأعضاء فقط — التعديل متاح لـ Owner و Admin.",
                    "Read-only member view — role changes require Owner or Admin."
                  )}
                </span>
              ) : null}
            </div>

            <div style={{ display: "grid", gap: "8px" }}>
              {members.map((m) => {
                const isTargetOwner = m.role === "owner";
                const adminCanManageTarget =
                  selectedOrg.my_role === "admin" && (m.role === "member" || m.role === "viewer");
                const canEditThisMember = !isTargetOwner && (isOrgOwner || adminCanManageTarget);

                return (
                  <div
                    key={m.user_id}
                    data-testid={`org-member-row-${m.user_id}`}
                    style={{
                      display: "flex",
                      flexWrap: "wrap",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: "10px",
                      padding: "10px 12px",
                      borderRadius: "10px",
                      background: "rgba(255,255,255,0.03)"
                    }}
                  >
                    <div style={{ display: "grid", gap: "2px" }}>
                      <strong>{m.display_name ?? m.email ?? m.user_id.slice(0, 8)}</strong>
                      <span dir="ltr" style={{ fontSize: "12px", opacity: 0.75 }}>
                        {m.email ?? m.user_id}
                      </span>
                    </div>

                    <div
                      style={{
                        display: "flex",
                        flexWrap: "wrap",
                        alignItems: "center",
                        gap: "8px"
                      }}
                    >
                      {canEditThisMember ? (
                        <select
                          value={m.role}
                          disabled={busy}
                          aria-label={L("تغيير الدور", "Change role")}
                          data-testid={`member-role-select-${m.user_id}`}
                          onChange={(e) =>
                            void handleChangeRole(m.user_id, e.target.value as OrgRole)
                          }
                          style={{
                            minHeight: "40px",
                            padding: "6px 10px",
                            borderRadius: "8px",
                            background: "rgba(0,0,0,0.3)",
                            color: "inherit"
                          }}
                        >
                          {isOrgOwner ? <option value="admin">Admin</option> : null}
                          <option value="member">Member</option>
                          <option value="viewer">Viewer</option>
                        </select>
                      ) : (
                        <code
                          data-testid={`member-role-badge-${m.user_id}`}
                          dir="ltr"
                          style={{ padding: "4px 10px", borderRadius: "8px" }}
                        >
                          {m.role}
                        </code>
                      )}

                      {canEditThisMember ? (
                        <button
                          type="button"
                          className="safi-btn safi-btn-ghost"
                          data-testid={`remove-member-btn-${m.user_id}`}
                          disabled={busy}
                          onClick={() => void handleRemoveMember(m.user_id)}
                          style={{ minHeight: "40px" }}
                        >
                          <UserMinus size={14} aria-hidden="true" />
                          {L("إزالة", "Remove")}
                        </button>
                      ) : null}

                      {isOrgOwner && !isTargetOwner ? (
                        <button
                          type="button"
                          className="safi-btn safi-btn-ghost"
                          data-testid={`transfer-owner-btn-${m.user_id}`}
                          disabled={busy}
                          onClick={() => void handleTransferOwnership(m.user_id)}
                          style={{ minHeight: "40px" }}
                        >
                          {L("نقل الملكية", "Make Owner")}
                        </button>
                      ) : null}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {/* Invitations Management (Owner & Admin) */}
          {canManageMembers ? (
            <div
              className="safi-panel"
              data-testid="org-invitations-section"
              style={{ padding: "18px", borderRadius: "14px", display: "grid", gap: "14px" }}
            >
              <strong>
                {L(
                  "إرسال ومتابعة دعوات الفريق (مربوطة بالبريد الموثق)",
                  "Team Invitations (Bound to Verified Email)"
                )}
              </strong>

              <form
                onSubmit={(e) => void handleInviteMember(e)}
                style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}
              >
                <input
                  type="email"
                  required
                  dir="ltr"
                  value={inviteEmail}
                  onChange={(e) => setInviteEmail(e.target.value)}
                  placeholder="teammate@company.com"
                  data-testid="invite-email-input"
                  style={{
                    flex: "2 1 220px",
                    minHeight: "44px",
                    padding: "8px 12px",
                    borderRadius: "10px",
                    background: "rgba(0,0,0,0.2)",
                    border: "1px solid rgba(255,255,255,0.14)",
                    color: "inherit"
                  }}
                />
                <select
                  value={inviteRole}
                  onChange={(e) => setInviteRole(e.target.value as OrgRole)}
                  aria-label={L("دور العضو المدعو", "Invited role")}
                  data-testid="invite-role-select"
                  style={{
                    minHeight: "44px",
                    padding: "8px 12px",
                    borderRadius: "10px",
                    background: "rgba(0,0,0,0.3)",
                    color: "inherit"
                  }}
                >
                  {isOrgOwner ? <option value="admin">Admin</option> : null}
                  <option value="member">Member</option>
                  <option value="viewer">Viewer</option>
                </select>
                <button
                  type="submit"
                  className="safi-btn safi-btn-primary"
                  data-testid="send-invite-btn"
                  disabled={busy || !inviteEmail.trim()}
                  style={{ minHeight: "44px" }}
                >
                  <UserPlus size={15} aria-hidden="true" />
                  {L("إرسال دعوة", "Send Invite")}
                </button>
              </form>

              {createdInvite ? (
                <div
                  data-testid="created-invite-link-box"
                  style={{
                    padding: "12px",
                    borderRadius: "10px",
                    background: "rgba(16, 185, 129, 0.1)",
                    border: "1px solid rgba(16, 185, 129, 0.35)",
                    display: "grid",
                    gap: "6px"
                  }}
                >
                  <span style={{ fontSize: "13px", fontWeight: 600 }}>
                    {L(
                      "رابط الدعوة لمرة واحدة (يُعرض الآن فقط ولا يُخزّن خامًا في الخادم):",
                      "Single-use invitation link (shown once; server stores only BLAKE3 hash):"
                    )}
                  </span>
                  <div
                    style={{ display: "flex", flexWrap: "wrap", gap: "8px", alignItems: "center" }}
                  >
                    <code
                      dir="ltr"
                      data-testid="created-invite-url"
                      style={{
                        flex: "1 1 280px",
                        wordBreak: "break-all",
                        fontSize: "12px",
                        padding: "6px 8px",
                        borderRadius: "6px",
                        background: "rgba(0,0,0,0.25)"
                      }}
                    >
                      {createdInvite.invite_url}
                    </code>
                    <button
                      type="button"
                      className="safi-btn safi-btn-ghost"
                      onClick={() => {
                        void navigator.clipboard?.writeText(createdInvite.invite_url);
                      }}
                    >
                      <Copy size={14} aria-hidden="true" />
                      {L("نسخ الرابط", "Copy Link")}
                    </button>
                  </div>
                </div>
              ) : null}

              {invitations.length > 0 ? (
                <div style={{ display: "grid", gap: "6px" }}>
                  {invitations.map((inv) => (
                    <div
                      key={inv.id}
                      data-testid={`org-invite-row-${inv.id}`}
                      style={{
                        display: "flex",
                        flexWrap: "wrap",
                        alignItems: "center",
                        justifyContent: "space-between",
                        gap: "8px",
                        padding: "8px 12px",
                        borderRadius: "8px",
                        background: "rgba(255,255,255,0.025)",
                        fontSize: "13px"
                      }}
                    >
                      <div>
                        <strong dir="ltr">{inv.invited_email}</strong> ·{" "}
                        <code dir="ltr">{inv.role}</code> ·{" "}
                        <span data-testid={`invite-status-${inv.id}`}>
                          [{inv.status.toUpperCase()}]
                        </span>
                      </div>
                      {inv.status === "pending" ? (
                        <button
                          type="button"
                          className="safi-btn safi-btn-ghost"
                          data-testid={`revoke-invite-btn-${inv.id}`}
                          disabled={busy}
                          onClick={() => void handleRevokeInvite(inv.id)}
                        >
                          {L("إلغاء الدعوة", "Revoke")}
                        </button>
                      ) : null}
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
          ) : null}

          {/* Desktop Zero-Knowledge End-to-End Encrypted Sync */}
          {desktopRuntime ? (
            <div
              className="safi-panel"
              data-testid="desktop-e2ee-sync-section"
              style={{ padding: "18px", borderRadius: "14px", display: "grid", gap: "14px" }}
            >
              <div
                style={{
                  display: "flex",
                  flexWrap: "wrap",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: "10px"
                }}
              >
                <div style={{ display: "grid", gap: "4px" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                    <Lock size={17} aria-hidden="true" />
                    <strong>
                      {L(
                        "المزامنة المشفّرة طرفًا لطرف (ChaCha20-Poly1305 + X25519)",
                        "Zero-Knowledge Encrypted Sync (ChaCha20-Poly1305 + X25519)"
                      )}
                    </strong>
                  </div>
                  {syncStatus ? (
                    <span style={{ fontSize: "12px", opacity: 0.8 }}>
                      {L("الحالة:", "Status:")}{" "}
                      <strong data-testid="desktop-sync-health-badge">
                        {syncStatus.health.toUpperCase()}
                      </strong>{" "}
                      · {L("قائمة الانتظار دون إنترنت:", "Offline Queue:")}{" "}
                      <strong data-testid="desktop-sync-offline-count">
                        {syncStatus.pending_offline_count}
                      </strong>{" "}
                      · {L("بصمة الجهاز:", "Device Fingerprint:")}{" "}
                      <code dir="ltr">
                        {syncStatus.device_identity.key_fingerprint.slice(0, 16)}
                      </code>
                    </span>
                  ) : null}
                </div>

                <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
                  <button
                    type="button"
                    className="safi-btn safi-btn-primary"
                    data-testid="desktop-sync-now-btn"
                    disabled={busy || readOnlyOffline || !sessionToken}
                    onClick={() => void handleDesktopSyncNow()}
                    style={{ minHeight: "42px" }}
                  >
                    <RefreshCw size={14} aria-hidden="true" />
                    {L("مزامنة الآن", "Sync Now")}
                  </button>
                  {canManageMembers ? (
                    <button
                      type="button"
                      className="safi-btn safi-btn-ghost"
                      data-testid="desktop-rotate-workspace-key-btn"
                      disabled={busy || readOnlyOffline || !sessionToken}
                      onClick={() => void handleManualKeyRotation()}
                      style={{ minHeight: "42px" }}
                    >
                      <KeyRound size={14} aria-hidden="true" />
                      {L("تدوير مفتاح الفريق", "Rotate Team Key")}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="safi-btn safi-btn-ghost"
                    data-testid="desktop-rotate-device-key-btn"
                    disabled={busy || readOnlyOffline || !sessionToken}
                    onClick={() => void handleRotateDeviceKey()}
                    style={{ minHeight: "42px" }}
                  >
                    {L("تدوير مفتاح الجهاز", "Rotate Device Key")}
                  </button>
                </div>
              </div>

              {/* Conflict Resolution UI */}
              {conflicts.length > 0 ? (
                <div
                  data-testid="desktop-sync-conflicts-box"
                  style={{
                    padding: "14px",
                    borderRadius: "12px",
                    border: "1px solid var(--safi-warning, #f59e0b)",
                    background: "rgba(245, 158, 11, 0.1)",
                    display: "grid",
                    gap: "10px"
                  }}
                >
                  <div
                    style={{ display: "flex", alignItems: "center", gap: "8px", fontWeight: 700 }}
                  >
                    <AlertTriangle size={16} aria-hidden="true" />
                    {L(
                      "تم رصد تعارض في الإصدار — اختر النسخة المعتمدة:",
                      "Version conflict detected — choose which revision to keep:"
                    )}
                  </div>
                  {conflicts.map((c) => (
                    <div
                      key={`${c.resource_kind}:${c.resource_id}`}
                      data-testid={`sync-conflict-${c.resource_id}`}
                      style={{
                        padding: "10px",
                        borderRadius: "8px",
                        background: "rgba(0,0,0,0.25)",
                        display: "grid",
                        gap: "8px"
                      }}
                    >
                      <div style={{ fontSize: "13px" }}>
                        <strong>
                          {c.resource_kind} / {c.resource_id}
                        </strong>{" "}
                        · {L("نسختك المحلية:", "Your local:")} «{c.local_title}» vs{" "}
                        {L("نسخة الفريق (v", "Team remote (v")}
                        {c.remote_version}): «{c.remote_title}»
                      </div>
                      <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
                        <button
                          type="button"
                          className="safi-btn safi-btn-primary"
                          data-testid={`conflict-keep-local-${c.resource_id}`}
                          disabled={busy}
                          onClick={() => void handleResolveConflict(c, "keep_local")}
                        >
                          {L("الاحتفاظ بنسختي المحلية", "Keep My Local Version")}
                        </button>
                        <button
                          type="button"
                          className="safi-btn safi-btn-ghost"
                          data-testid={`conflict-accept-remote-${c.resource_id}`}
                          disabled={busy}
                          onClick={() => void handleResolveConflict(c, "accept_remote")}
                        >
                          {L("اعتماد نسخة الفريق", "Accept Team Version")}
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}

              {/* Create / Update Shared Encrypted Resource */}
              {canModifyResources && !readOnlyOffline ? (
                <form
                  onSubmit={(e) => void handleStageAndSyncResource(e)}
                  data-testid="desktop-shared-resource-form"
                  style={{ display: "grid", gap: "8px" }}
                >
                  <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
                    <select
                      value={resKind}
                      onChange={(e) =>
                        setResKind(
                          e.target.value as "recipe" | "project" | "dashboard" | "document"
                        )
                      }
                      data-testid="shared-resource-kind-select"
                      style={{
                        minHeight: "42px",
                        padding: "6px 10px",
                        borderRadius: "8px",
                        background: "rgba(0,0,0,0.3)",
                        color: "inherit"
                      }}
                    >
                      <option value="recipe">Recipe</option>
                      <option value="project">Project</option>
                      <option value="dashboard">Dashboard</option>
                      <option value="document">Document</option>
                    </select>
                    <input
                      type="text"
                      required
                      dir="ltr"
                      value={resId}
                      onChange={(e) => setResId(e.target.value)}
                      placeholder="resource-id"
                      data-testid="shared-resource-id-input"
                      style={{
                        width: "180px",
                        minHeight: "42px",
                        padding: "6px 10px",
                        borderRadius: "8px",
                        background: "rgba(0,0,0,0.2)",
                        border: "1px solid rgba(255,255,255,0.14)",
                        color: "inherit"
                      }}
                    />
                    <input
                      type="text"
                      required
                      value={resTitle}
                      onChange={(e) => setResTitle(e.target.value)}
                      placeholder={L("عنوان المورد المشترك", "Shared resource title")}
                      data-testid="shared-resource-title-input"
                      style={{
                        flex: "1 1 200px",
                        minHeight: "42px",
                        padding: "6px 10px",
                        borderRadius: "8px",
                        background: "rgba(0,0,0,0.2)",
                        border: "1px solid rgba(255,255,255,0.14)",
                        color: "inherit"
                      }}
                    />
                  </div>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
                    <input
                      type="text"
                      dir="ltr"
                      value={resContent}
                      onChange={(e) => setResContent(e.target.value)}
                      data-testid="shared-resource-payload-input"
                      style={{
                        flex: "1 1 280px",
                        minHeight: "42px",
                        padding: "6px 10px",
                        borderRadius: "8px",
                        background: "rgba(0,0,0,0.2)",
                        border: "1px solid rgba(255,255,255,0.14)",
                        color: "inherit"
                      }}
                    />
                    <button
                      type="submit"
                      className="safi-btn safi-btn-primary"
                      data-testid="shared-resource-save-sync-btn"
                      disabled={busy || !resTitle.trim() || !resId.trim()}
                      style={{ minHeight: "42px" }}
                    >
                      <Lock size={14} aria-hidden="true" />
                      {L("تشفير ومزامنة", "Encrypt & Sync")}
                    </button>
                  </div>
                </form>
              ) : (
                <p
                  data-testid="viewer-readonly-sync-notice"
                  style={{ margin: 0, fontSize: "13px", opacity: 0.8 }}
                >
                  {L(
                    "دور المشاهد (Viewer) أو وضع القراءة فقط يسمح بفك تشفير وعرض الموارد المشتركة دون تعديلها.",
                    "Viewer role / Offline Read-Only mode can decrypt and view shared resources but cannot modify them."
                  )}
                </p>
              )}

              {/* Decrypted Shared Resources List */}
              <div
                data-testid="desktop-shared-resources-list"
                style={{ display: "grid", gap: "8px" }}
              >
                {sharedResources.length === 0 ? (
                  <span style={{ fontSize: "13px", opacity: 0.72 }}>
                    {L(
                      "لا توجد موارد مشتركة متزامنة في هذه المساحة بعد.",
                      "No synced shared resources in this workspace yet."
                    )}
                  </span>
                ) : (
                  sharedResources.map((r) => (
                    <div
                      key={`${r.resource_kind}:${r.resource_id}`}
                      data-testid={`shared-resource-item-${r.resource_id}`}
                      style={{
                        display: "flex",
                        flexWrap: "wrap",
                        alignItems: "center",
                        justifyContent: "space-between",
                        gap: "10px",
                        padding: "10px 12px",
                        borderRadius: "10px",
                        background: "rgba(255,255,255,0.03)"
                      }}
                    >
                      <div style={{ display: "grid", gap: "2px" }}>
                        <div>
                          <strong>{r.title}</strong> ·{" "}
                          <code dir="ltr">
                            {r.resource_kind}/{r.resource_id}
                          </code>{" "}
                          · <span dir="ltr">v{r.version}</span> (Epoch #{r.key_epoch})
                          {r.dirty ? (
                            <span style={{ marginInlineStart: "8px", color: "#fbbf24" }}>
                              [{L("بانتظار الرفع", "Queued Offline")}]
                            </span>
                          ) : null}
                        </div>
                        <code dir="ltr" style={{ fontSize: "12px", opacity: 0.78 }}>
                          {r.payload_json}
                        </code>
                      </div>

                      {canModifyResources && !readOnlyOffline ? (
                        <button
                          type="button"
                          className="safi-btn safi-btn-ghost"
                          data-testid={`delete-shared-resource-${r.resource_id}`}
                          disabled={busy}
                          onClick={() =>
                            void handleDeleteSharedResource(r.resource_kind, r.resource_id)
                          }
                        >
                          <Trash2 size={14} aria-hidden="true" />
                          {L("حذف", "Delete")}
                        </button>
                      ) : null}
                    </div>
                  ))
                )}
              </div>
            </div>
          ) : null}

          {/* Danger Zone: Delete Organization (Owner ONLY) */}
          {isOrgOwner ? (
            <div
              style={{
                display: "flex",
                justifyContent: "flex-end"
              }}
            >
              <button
                type="button"
                className="safi-btn safi-btn-ghost"
                data-testid="delete-org-btn"
                disabled={busy}
                onClick={() => void handleDeleteOrganization()}
                style={{ color: "var(--safi-danger, #ef4444)", minHeight: "42px" }}
              >
                <Trash2 size={15} aria-hidden="true" />
                {L("حذف المؤسسة نهائيًا (Owner فقط)", "Delete Organization (Owner Only)")}
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
