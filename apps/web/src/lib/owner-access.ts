/**
 * Owner-only access rules for the executive console.
 *
 * The Friend VIP role can only be granted or revoked by the single strict owner
 * from `config/business.json`. No purchase path, self-service toggle, or
 * non-owner account can produce a Friend grant.
 */
import { FRIEND_ROLE, OWNER_EMAIL } from "./business-config";
import { isOwnerEmail, isValidEmail, normalizeEmail } from "./auth-rules";

export interface FriendMember {
  readonly email: string;
  readonly note: string;
  readonly grantedAt: string;
  readonly grantedBy: string;
  readonly aiCallsUsed: number;
  readonly aiCallsLimit: number;
  readonly entitlesPaidPlan: boolean;
}

export type FriendGrantResult =
  | { readonly ok: true; readonly member: FriendMember }
  | { readonly ok: false; readonly reason: "not_owner" | "invalid_email" | "owner_address" };

export type FriendRevokeResult =
  | { readonly ok: true; readonly email: string }
  | { readonly ok: false; readonly reason: "not_owner" | "invalid_email" };

export function canManageFriends(actorEmail: string): boolean {
  return isOwnerEmail(actorEmail);
}

export function grantFriendRole(
  actorEmail: string,
  friendEmail: string,
  note: string,
  grantedAtIso: string
): FriendGrantResult {
  if (!canManageFriends(actorEmail)) {
    return { ok: false, reason: "not_owner" };
  }

  const email = normalizeEmail(friendEmail);
  if (!isValidEmail(email)) {
    return { ok: false, reason: "invalid_email" };
  }
  if (email === OWNER_EMAIL) {
    return { ok: false, reason: "owner_address" };
  }

  return {
    ok: true,
    member: {
      email,
      note: note.trim(),
      grantedAt: grantedAtIso,
      grantedBy: OWNER_EMAIL,
      aiCallsUsed: 0,
      aiCallsLimit: FRIEND_ROLE.ai_calls_per_month,
      entitlesPaidPlan: FRIEND_ROLE.all_paid_features_open
    }
  };
}

export function revokeFriendRole(actorEmail: string, friendEmail: string): FriendRevokeResult {
  if (!canManageFriends(actorEmail)) {
    return { ok: false, reason: "not_owner" };
  }

  const email = normalizeEmail(friendEmail);
  if (!isValidEmail(email)) {
    return { ok: false, reason: "invalid_email" };
  }

  return { ok: true, email };
}
