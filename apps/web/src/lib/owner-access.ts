/**
 * Owner-only UI guard helpers for the executive control plane.
 *
 * Security invariant: authorization is enforced strictly on the backend using
 * the sealed immutable `owner_user_id` + RFC 6238 TOTP elevated/step-up session.
 * The client NEVER embeds or checks a hardcoded owner email address.
 */
import { FRIEND_ROLE } from "./business-config";
import { isValidEmail, normalizeEmail } from "./auth-rules";

export interface FriendMember {
  readonly email: string;
  readonly note: string;
  readonly grantedAt: string;
  readonly grantedByUserId: string;
  readonly aiCallsUsed: number;
  readonly aiCallsLimit: number;
  readonly entitlesPaidPlan: boolean;
}

export type FriendGrantResult =
  | { readonly ok: true; readonly member: FriendMember }
  | { readonly ok: false; readonly reason: "not_owner" | "invalid_email" | "self_grant" };

export type FriendRevokeResult =
  | { readonly ok: true; readonly email: string }
  | { readonly ok: false; readonly reason: "not_owner" | "invalid_email" };

export interface OwnerActorContext {
  readonly isOwner: boolean;
  readonly userId: string;
  readonly email?: string | null;
}

export function canManageFriends(actor: OwnerActorContext): boolean {
  return actor.isOwner && actor.userId.trim().length > 0;
}

export function validateFriendGrantRequest(
  actor: OwnerActorContext,
  friendEmail: string,
  note: string,
  grantedAtIso: string
): FriendGrantResult {
  if (!canManageFriends(actor)) {
    return { ok: false, reason: "not_owner" };
  }

  const email = normalizeEmail(friendEmail);
  if (!isValidEmail(email)) {
    return { ok: false, reason: "invalid_email" };
  }
  if (actor.email && normalizeEmail(actor.email) === email) {
    return { ok: false, reason: "self_grant" };
  }

  return {
    ok: true,
    member: {
      email,
      note: note.trim(),
      grantedAt: grantedAtIso,
      grantedByUserId: actor.userId,
      aiCallsUsed: 0,
      aiCallsLimit: FRIEND_ROLE.ai_calls_per_month,
      entitlesPaidPlan: FRIEND_ROLE.all_paid_features_open
    }
  };
}

export function validateFriendRevokeRequest(
  actor: OwnerActorContext,
  friendEmail: string
): FriendRevokeResult {
  if (!canManageFriends(actor)) {
    return { ok: false, reason: "not_owner" };
  }

  const email = normalizeEmail(friendEmail);
  if (!isValidEmail(email)) {
    return { ok: false, reason: "invalid_email" };
  }

  return { ok: true, email };
}
