export interface SafiAuthSession {
  readonly id: string;
  readonly workspace_id: string;
  readonly device_name: string;
  readonly expires_at: number;
  readonly created_at: number;
  readonly last_seen_at: number;
}

export interface SafiLoginResponse {
  readonly token: string;
  readonly session: SafiAuthSession;
}

export class SafiAuthApiError extends Error {
  readonly status: number;
  readonly code: string | null;

  constructor(status: number, code: string | null) {
    super("Safi account request failed");
    this.name = "SafiAuthApiError";
    this.status = status;
    this.code = code;
  }
}

async function sendAuthRequest<T>(
  path: string,
  method: "POST" | "DELETE",
  body?: unknown,
  sessionToken?: string
): Promise<T> {
  const headers = new Headers({ Accept: "application/json" });
  if (body !== undefined) headers.set("Content-Type", "application/json");
  if (sessionToken) headers.set("Authorization", `Bearer ${sessionToken}`);

  let response: Response;
  try {
    response = await fetch(`/v1/auth${path}`, {
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
      // Do not expose arbitrary server or mail-provider response bodies.
    }
    throw new SafiAuthApiError(response.status, code);
  }

  if (response.status === 204) return undefined as T;
  try {
    return (await response.json()) as T;
  } catch {
    throw new SafiAuthApiError(response.status, null);
  }
}

export function postSafiAuth<T>(path: string, body: unknown): Promise<T> {
  return sendAuthRequest<T>(path, "POST", body);
}

export function deleteSafiAuth(path: string, sessionToken: string): Promise<void> {
  return sendAuthRequest<void>(path, "DELETE", undefined, sessionToken);
}
