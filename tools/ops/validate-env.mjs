#!/usr/bin/env node
/**
 * tools/ops/validate-env.mjs — Secret-safe environment validator for SafiData Server.
 * Checks required and optional variables without ever printing secret values.
 */

const env = process.env;
const mode = (env.SAFI_ENV ?? "development").trim().toLowerCase();
const strict = mode === "production" || mode === "staging";

function isSet(name) {
  const val = (env[name] ?? "").trim();
  return val.length > 0 && !val.startsWith("CHANGE_ME_");
}

function isHex32(name) {
  const val = (env[name] ?? "").trim();
  return /^[0-9a-fA-F]{64}$/.test(val);
}

const requiredInStrict = [
  { name: "DATABASE_URL", check: isSet },
  { name: "SAFI_VALKEY_URL", check: isSet },
  { name: "SAFI_TOTP_ENCRYPTION_KEY", check: isHex32 },
  { name: "SAFI_LICENSE_SIGNING_PRIVATE_KEY", check: isHex32 },
  { name: "SAFI_UPDATE_SIGNING_PRIVATE_KEY", check: isHex32 },
  { name: "SAFI_TOKEN_SIGNING_KEY", check: isHex32 }
];

const integrations = {
  postgresql: isSet("DATABASE_URL") ? "CONFIGURED" : "NOT_CONFIGURED",
  valkey: isSet("SAFI_VALKEY_URL") ? "CONFIGURED" : "NOT_CONFIGURED",
  owner_totp_encryption: isHex32("SAFI_TOTP_ENCRYPTION_KEY") ? "CONFIGURED" : "NOT_CONFIGURED",
  ed25519_license_signing: isHex32("SAFI_LICENSE_SIGNING_PRIVATE_KEY")
    ? "CONFIGURED"
    : "NOT_CONFIGURED",
  ed25519_update_signing: isHex32("SAFI_UPDATE_SIGNING_PRIVATE_KEY")
    ? "CONFIGURED"
    : "NOT_CONFIGURED",
  smtp_email:
    isSet("SAFI_SMTP_HOST") && isSet("SAFI_SMTP_FROM") ? "CONFIGURED" : "NOT_CONFIGURED",
  sms_otp: isSet("SAFI_SMS_PROVIDER") && isSet("SAFI_SMS_API_KEY") ? "CONFIGURED" : "NOT_CONFIGURED",
  whatsapp_otp:
    isSet("SAFI_WHATSAPP_PROVIDER") && isSet("SAFI_WHATSAPP_API_KEY")
      ? "CONFIGURED"
      : "NOT_CONFIGURED",
  oauth_google:
    isSet("SAFI_OAUTH_GOOGLE_CLIENT_ID") && isSet("SAFI_OAUTH_GOOGLE_CLIENT_SECRET")
      ? "CONFIGURED"
      : "NOT_CONFIGURED",
  oauth_apple:
    isSet("SAFI_OAUTH_APPLE_CLIENT_ID") && isSet("SAFI_OAUTH_APPLE_PRIVATE_KEY_PEM")
      ? "CONFIGURED"
      : "NOT_CONFIGURED",
  oauth_github:
    isSet("SAFI_OAUTH_GITHUB_CLIENT_ID") && isSet("SAFI_OAUTH_GITHUB_CLIENT_SECRET")
      ? "CONFIGURED"
      : "NOT_CONFIGURED",
  billing_paddle:
    isSet("SAFI_PADDLE_API_KEY") && isSet("SAFI_PADDLE_WEBHOOK_SECRET")
      ? "CONFIGURED"
      : "NOT_CONFIGURED",
  billing_paymob:
    isSet("SAFI_PAYMOB_API_KEY") && isSet("SAFI_PAYMOB_HMAC_SECRET")
      ? "CONFIGURED"
      : "NOT_CONFIGURED",
  ai_provider:
    isSet("SAFI_AI_PROVIDER_URL") && isSet("SAFI_AI_API_KEY") && isSet("SAFI_AI_MODEL")
      ? "CONFIGURED"
      : "NOT_CONFIGURED",
  observability_otlp: isSet("SAFI_OTLP_ENDPOINT") ? "CONFIGURED" : "NOT_CONFIGURED",
  error_tracking_glitchtip: isSet("SAFI_GLITCHTIP_DSN") ? "CONFIGURED" : "NOT_CONFIGURED"
};

const missingRequired = strict
  ? requiredInStrict.filter((item) => !item.check(item.name)).map((item) => item.name)
  : [];

const report = {
  environment: mode,
  strict_mode: strict,
  ready: missingRequired.length === 0,
  missing_required: missingRequired,
  integrations
};

console.log(JSON.stringify(report, null, 2));
if (!report.ready) {
  console.error(
    `SF-SYS-032: Missing or malformed required configuration for ${mode}: ${missingRequired.join(", ")}`
  );
  process.exit(1);
}
