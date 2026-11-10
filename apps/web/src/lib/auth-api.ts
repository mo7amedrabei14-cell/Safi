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

export class SafiAuthConfigurationError extends Error {
  readonly code = "SF-SYS-031";

  constructor() {
    super("Safi account service origin is not configured");
    this.name = "SafiAuthConfigurationError";
  }
}

const BUILD_API_ORIGIN =
  typeof __SAFI_API_BASE_URL__ === "string" ? __SAFI_API_BASE_URL__ : "";

/**
 * Normalize a strict HTTPS origin. Returns null for anything that is not a
 * multi-label DNS origin without credentials, a path, a query, or a fragment.
 */
export function normalizeAuthApiOrigin(raw: string): string | null {
  const value = raw.trim();
  if (value.length < 8 || value.slice(0, 8).toLowerCase() !== "https://") return null;
  const rest = value.slice(8);
  if (rest.length === 0 || /[/?#@]/.test(rest)) return null;

  let host = rest;
  let portText: string | null = null;
  const colon = rest.lastIndexOf(":");
  if (colon !== -1) {
    host = rest.slice(0, colon);
    portText = rest.slice(colon + 1);
    if (!/^\d+$/.test(portText)) return null;
  }

  if (host.length === 0 || host.startsWith("[") || host.includes(":")) return null;
  host = host.toLowerCase();
  if (host === "localhost" || host.endsWith(".localhost")) return null;
  if (/^[\d.]+$/.test(host)) return null;
  if (!host.includes(".")) return null;
  const labels = host.split(".");
  if (
    !labels.every(
      (label) =>
        label.length > 0 &&
        label.length <= 63 &&
        !label.startsWith("-") &&
        !label.endsWith("-") &&
        /^[a-z0-9-]+$/.test(label)
    )
  ) {
    return null;
  }

  let port: number | null = null;
  if (portText !== null) {
    const parsed = Number(portText);
    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 65_535) return null;
    if (parsed !== 443) port = parsed;
  }

  return port === null ? `https://${host}` : `https://${host}:${port}`;
}

/**
 * Resolve the API base used by account requests.
 *
 * The website keeps same-origin relative paths. A packaged desktop build must
 * provide a valid HTTPS origin; a missing value is a configuration error rather
 * than a silent request to the Tauri asset origin.
 */
export function resolveAuthApiBaseUrl(
  raw: string,
  requireConfiguredOrigin: boolean
): string {
  const value = raw.trim();
  if (value.length === 0) {
    if (requireConfiguredOrigin) throw new SafiAuthConfigurationError();
    return "";
  }
  const origin = normalizeAuthApiOrigin(value);
  if (origin === null) throw new SafiAuthConfigurationError();
  return origin;
}

function isInstalledDesktopBuild(): boolean {
  return (
    import.meta.env.PROD &&
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in window
  );
}

let resolvedBase: string | undefined;

function authApiBaseUrl(): string {
  if (resolvedBase === undefined) {
    resolvedBase = resolveAuthApiBaseUrl(BUILD_API_ORIGIN, isInstalledDesktopBuild());
  }
  return resolvedBase;
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
    response = await fetch(`${authApiBaseUrl()}/v1/auth${path}`, {
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
  return sendAuthRequest<undefined>(path, "DELETE", undefined, sessionToken);
}
