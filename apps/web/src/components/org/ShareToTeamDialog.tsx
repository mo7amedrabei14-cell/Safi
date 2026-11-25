/**
 * `apps/web/src/components/org/ShareToTeamDialog.tsx`
 *
 * Explicit, privacy-transparent "Share to Team" modal invoked directly from real
 * SafiData Desktop Studio resource screens (Recipe Library, Interactive Dashboard,
 * Sheet Workflow Memory, and Project Blueprints).
 *
 * Encrypts locally in Rust (`ChaCha20-Poly1305`) before syncing to the selected
 * Organization's Zero-Knowledge Shared Workspace, and never uploads raw CSV/XLSX rows.
 */
import type { FormEvent, ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, Building2, CheckCircle2, Lock, ShieldCheck, Users, X } from "lucide-react";
import type { SafiLang } from "../layout/FloatingGlassDock";
import { stageDesktopSharedResourceUpsert } from "../../desktop/studio-bridge";
import {
  describeOrgError,
  fetchMyOrganizations,
  synchronizeDesktopOrganizationWorkspace,
  type EnrichedOrganizationSummary
} from "../../lib/organization-api";
import type { TeamShareDraft } from "../../lib/team-shared-resources";

export interface ShareToTeamDialogProps {
  readonly open: boolean;
  readonly draft: TeamShareDraft | null;
  readonly lang?: SafiLang;
  readonly sessionToken: string | null;
  readonly readOnlyOffline?: boolean;
  readonly defaultOrganizationId?: string | null;
  readonly onClose: () => void;
  readonly onSharedSuccess?: (result: {
    readonly organizationId: string;
    readonly organizationName: string;
    readonly workspaceId: string;
    readonly resourceId: string;
    readonly title: string;
    readonly syncedOnline: boolean;
  }) => void;
  readonly onOpenTeamWorkspace?: () => void;
}

export function ShareToTeamDialog({
  open,
  draft,
  lang = "en",
  sessionToken,
  readOnlyOffline = false,
  defaultOrganizationId = null,
  onClose,
  onSharedSuccess,
  onOpenTeamWorkspace
}: ShareToTeamDialogProps): ReactNode {
  const isAr = lang === "ar";
  const L = useCallback((ar: string, en: string) => (isAr ? ar : en), [isAr]);

  const [loadingOrgs, setLoadingOrgs] = useState(false);
  const [organizations, setOrganizations] = useState<readonly EnrichedOrganizationSummary[]>([]);
  const [selectedOrgId, setSelectedOrgId] = useState<string>("");
  const [customTitle, setCustomTitle] = useState<string>("");
  const [teamNotes, setTeamNotes] = useState<string>("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [doneResult, setDoneResult] = useState<{
    readonly organizationName: string;
    readonly title: string;
    readonly syncedOnline: boolean;
  } | null>(null);

  useEffect(() => {
    if (!open || !draft) return;
    setCustomTitle(draft.title);
    setTeamNotes("");
    setError(null);
    setDoneResult(null);

    if (!sessionToken) {
      setOrganizations([]);
      setSelectedOrgId("");
      return;
    }

    let active = true;
    setLoadingOrgs(true);
    void fetchMyOrganizations(sessionToken)
      .then((res) => {
        if (!active) return;
        setOrganizations(res.organizations);
        const writableDefault =
          res.organizations.find(
            (o) => o.organization.id === defaultOrganizationId && o.my_role !== "viewer"
          ) ??
          res.organizations.find((o) => o.my_role !== "viewer") ??
          res.organizations[0];
        setSelectedOrgId(writableDefault?.organization.id ?? "");
      })
      .catch((err) => {
        if (!active) return;
        setError(describeOrgError(err, isAr));
      })
      .finally(() => {
        if (active) setLoadingOrgs(false);
      });

    return () => {
      active = false;
    };
  }, [open, draft, sessionToken, defaultOrganizationId, isAr]);

  if (!open || !draft) return null;

  const selectedOrg = organizations.find((o) => o.organization.id === selectedOrgId) ?? null;
  const canShareToSelected =
    selectedOrg !== null && selectedOrg.my_role !== "viewer" && selectedOrg.teams_enabled;

  const handleConfirmShare = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    if (!sessionToken || !selectedOrg || !canShareToSelected || readOnlyOffline) return;

    setSubmitting(true);
    setError(null);
    try {
      const finalTitle = customTitle.trim() || draft.title;
      const payloadWithNotes = {
        ...draft.payload,
        ...(teamNotes.trim() ? { team_notes: teamNotes.trim() } : {})
      };

      // Ensure workspace keys are bootstrapped/synced before staging encryption
      try {
        await synchronizeDesktopOrganizationWorkspace({
          sessionToken,
          organizationId: selectedOrg.organization.id,
          workspaceId: selectedOrg.organization.workspace_id,
          activeKeyEpoch: selectedOrg.organization.active_key_epoch,
          myRole: selectedOrg.my_role
        });
      } catch {
        // Offline or already bootstrapped locally; staging will use local epoch key if present.
      }

      await stageDesktopSharedResourceUpsert({
        organizationId: selectedOrg.organization.id,
        workspaceId: selectedOrg.organization.workspace_id,
        resourceKind: draft.resourceKind,
        resourceId: draft.resourceId,
        title: finalTitle,
        payloadJson: JSON.stringify(payloadWithNotes)
      });

      let syncedOnline = false;
      try {
        await synchronizeDesktopOrganizationWorkspace({
          sessionToken,
          organizationId: selectedOrg.organization.id,
          workspaceId: selectedOrg.organization.workspace_id,
          activeKeyEpoch: selectedOrg.organization.active_key_epoch,
          myRole: selectedOrg.my_role
        });
        syncedOnline = true;
      } catch {
        syncedOnline = false;
      }

      setDoneResult({
        organizationName: selectedOrg.organization.name,
        title: finalTitle,
        syncedOnline
      });

      onSharedSuccess?.({
        organizationId: selectedOrg.organization.id,
        organizationName: selectedOrg.organization.name,
        workspaceId: selectedOrg.organization.workspace_id,
        resourceId: draft.resourceId,
        title: finalTitle,
        syncedOnline
      });
    } catch (err) {
      setError(describeOrgError(err, isAr));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={L("مشاركة مع الفريق (مشفّر طرفًا لطرف)", "Share to Team (End-to-End Encrypted)")}
      data-testid="share-to-team-dialog"
      dir={isAr ? "rtl" : "ltr"}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9600,
        background: "rgba(5, 8, 16, 0.78)",
        backdropFilter: "blur(8px)",
        display: "grid",
        placeItems: "center",
        padding: "16px",
        overflowY: "auto"
      }}
    >
      <div
        className="safi-panel"
        style={{
          width: "100%",
          maxWidth: "560px",
          padding: "22px",
          borderRadius: "18px",
          display: "grid",
          gap: "16px",
          border: "1px solid var(--border-strong)"
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
          <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
            <Users size={19} aria-hidden="true" style={{ color: "var(--accent-primary)" }} />
            <h2 style={{ margin: 0, fontSize: "1.1rem", fontWeight: 800 }}>
              {L("مشاركة المورد مع مساحة عمل الفريق", "Share Resource to Team Workspace")}
            </h2>
          </div>
          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            onClick={onClose}
            aria-label={L("إغلاق", "Close")}
            data-testid="share-to-team-close-btn"
            style={{ minWidth: "40px", minHeight: "40px", padding: "6px" }}
          >
            <X size={17} aria-hidden="true" />
          </button>
        </div>

        {/* Local-First Privacy Preview */}
        <div
          data-testid="share-to-team-privacy-preview"
          style={{
            padding: "14px",
            borderRadius: "12px",
            background: "color-mix(in srgb, var(--status-ok) 8%, var(--bg-surface))",
            border: "1px solid color-mix(in srgb, var(--status-ok) 32%, transparent)",
            display: "grid",
            gap: "8px",
            fontSize: "0.86rem"
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "8px", fontWeight: 700 }}>
            <ShieldCheck size={16} aria-hidden="true" style={{ color: "var(--status-ok)" }} />
            <span>
              {L(
                "معاينة الخصوصية المحلية — لا يتم رفع أي صفوف بيانات CSV/XLSX خام:",
                "Local-First Privacy Preview — Zero raw CSV/XLSX rows uploaded:"
              )}
            </span>
          </div>
          <div style={{ fontWeight: 600 }}>{isAr ? draft.summaryAr : draft.summaryEn}</div>
          <ul style={{ margin: 0, paddingInlineStart: "18px", display: "grid", gap: "4px" }}>
            {(isAr ? draft.includedItemsAr : draft.includedItemsEn).map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </div>

        {error ? (
          <div
            role="alert"
            data-testid="share-to-team-error"
            style={{
              padding: "12px",
              borderRadius: "10px",
              border: "1px solid var(--status-danger)",
              color: "var(--status-danger)",
              fontSize: "0.88rem",
              display: "flex",
              alignItems: "center",
              gap: "8px"
            }}
          >
            <AlertTriangle size={16} aria-hidden="true" />
            <span>{error}</span>
          </div>
        ) : null}

        {doneResult ? (
          <div
            role="status"
            data-testid="share-to-team-success"
            style={{
              padding: "16px",
              borderRadius: "12px",
              border: "1px solid color-mix(in srgb, var(--status-ok) 45%, transparent)",
              background: "color-mix(in srgb, var(--status-ok) 10%, var(--bg-surface))",
              display: "grid",
              gap: "12px"
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: "8px", fontWeight: 700 }}>
              <CheckCircle2 size={18} aria-hidden="true" style={{ color: "var(--status-ok)" }} />
              <span>
                {doneResult.syncedOnline
                  ? L(
                      `تم تشفير ومشاركة «${doneResult.title}» مع فريق «${doneResult.organizationName}» بنجاح!`,
                      `Encrypted & shared "${doneResult.title}" with "${doneResult.organizationName}"!`
                    )
                  : L(
                      `تم تشفير «${doneResult.title}» وحفظه في قائمة الانتظار دون إنترنت وسيُرفع تلقائيًا عند عودة الاتصال.`,
                      `Encrypted "${doneResult.title}" and queued offline — it will sync automatically when reconnected.`
                    )}
              </span>
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
              {onOpenTeamWorkspace ? (
                <button
                  type="button"
                  className="safi-btn safi-btn-primary"
                  data-testid="share-to-team-open-workspace-btn"
                  onClick={() => {
                    onClose();
                    onOpenTeamWorkspace();
                  }}
                >
                  <Users size={15} aria-hidden="true" />
                  <span>{L("فتح مساحة عمل الفريق", "Open Team Workspace")}</span>
                </button>
              ) : null}
              <button type="button" className="safi-btn safi-btn-ghost" onClick={onClose}>
                {L("إغلاق", "Done")}
              </button>
            </div>
          </div>
        ) : !sessionToken ? (
          <p style={{ margin: 0, fontSize: "0.9rem" }}>
            {L(
              "سجّل الدخول بحسابك أولًا لمشاركة الموارد عبر المزامنة المشفّرة للفريق.",
              "Sign in to your SafiData account first to share resources with your team workspace."
            )}
          </p>
        ) : loadingOrgs ? (
          <p style={{ margin: 0, fontSize: "0.9rem" }}>
            {L("جارٍ تحميل مؤسساتك...", "Loading your organizations...")}
          </p>
        ) : organizations.length === 0 ? (
          <div style={{ display: "grid", gap: "10px", fontSize: "0.9rem" }}>
            <p style={{ margin: 0 }}>
              {L(
                "لست عضوًا في أي مؤسسة بعد. أنشئ مؤسسة أو اقبل دعوة من نافذة «الفريق والمزامنة المشفّرة».",
                "You are not a member of an organization yet. Create an organization or accept an invitation from Team & E2EE Sync."
              )}
            </p>
            {onOpenTeamWorkspace ? (
              <button
                type="button"
                className="safi-btn safi-btn-primary"
                onClick={() => {
                  onClose();
                  onOpenTeamWorkspace();
                }}
              >
                <Building2 size={15} aria-hidden="true" />
                <span>{L("فتح إعدادات الفريق", "Open Team Workspace")}</span>
              </button>
            ) : null}
          </div>
        ) : (
          <form
            onSubmit={(e) => void handleConfirmShare(e)}
            style={{ display: "grid", gap: "12px" }}
          >
            <label style={{ display: "grid", gap: "6px", fontSize: "0.86rem", fontWeight: 700 }}>
              <span>{L("المؤسسة المستهدفة", "Destination Organization")}</span>
              <select
                className="safi-input"
                value={selectedOrgId}
                disabled={submitting}
                data-testid="share-to-team-org-select"
                onChange={(e) => setSelectedOrgId(e.target.value)}
                style={{ minHeight: "44px" }}
              >
                {organizations.map((o) => (
                  <option key={o.organization.id} value={o.organization.id}>
                    {o.organization.name} ({o.my_role.toUpperCase()} · Epoch #
                    {o.organization.active_key_epoch})
                  </option>
                ))}
              </select>
            </label>

            {selectedOrg && selectedOrg.my_role === "viewer" ? (
              <div
                role="alert"
                data-testid="share-to-team-viewer-blocked"
                style={{
                  padding: "10px 12px",
                  borderRadius: "10px",
                  border: "1px solid var(--status-warn)",
                  fontSize: "0.84rem"
                }}
              >
                {L(
                  "دورك في هذه المؤسسة هو «مشاهد (Viewer)» ولا يملك صلاحية المشاركة أو التعديل.",
                  "Your role in this organization is Viewer (read-only) and cannot share or modify team resources."
                )}
              </div>
            ) : null}

            <label style={{ display: "grid", gap: "6px", fontSize: "0.86rem", fontWeight: 700 }}>
              <span>{L("الاسم الظاهر للفريق", "Shared Display Title")}</span>
              <input
                type="text"
                required
                className="safi-input"
                value={customTitle}
                disabled={submitting}
                data-testid="share-to-team-title-input"
                onChange={(e) => setCustomTitle(e.target.value)}
                style={{ minHeight: "44px" }}
              />
            </label>

            <label style={{ display: "grid", gap: "6px", fontSize: "0.86rem", fontWeight: 700 }}>
              <span>{L("ملاحظة للفريق (اختياري)", "Team Note (optional)")}</span>
              <input
                type="text"
                className="safi-input"
                value={teamNotes}
                disabled={submitting}
                data-testid="share-to-team-notes-input"
                onChange={(e) => setTeamNotes(e.target.value)}
                placeholder={L(
                  "مثال: معتمدة لإغلاق حسابات الربع الرابع",
                  "e.g. Approved standard for Q4 monthly closing"
                )}
                style={{ minHeight: "44px" }}
              />
            </label>

            <div
              style={{ display: "flex", flexWrap: "wrap", justifyContent: "flex-end", gap: "8px" }}
            >
              <button
                type="button"
                className="safi-btn safi-btn-ghost"
                disabled={submitting}
                onClick={onClose}
                style={{ minHeight: "44px" }}
              >
                {L("إلغاء", "Cancel")}
              </button>
              <button
                type="submit"
                className="safi-btn safi-btn-primary"
                disabled={submitting || !canShareToSelected || readOnlyOffline}
                data-testid="share-to-team-confirm-btn"
                style={{ minHeight: "44px" }}
              >
                <Lock size={15} aria-hidden="true" />
                <span>
                  {submitting
                    ? L("جارٍ التشفير والمزامنة...", "Encrypting & Syncing...")
                    : L("تشفير محليًا ومشاركة مع الفريق", "Encrypt Locally & Share to Team")}
                </span>
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
