/**
 * `apps/web/src/utils/logger.ts` — Unified JSON Logger, Rolling Buffer, and Secret Redaction (Section 15.3 & 15.8).
 */

export interface LogEntry {
  timestamp: string;
  level: "INFO" | "WARN" | "ERROR";
  service: string;
  event: string;
  error_code?: string;
  message: string;
  meta?: Record<string, unknown>;
}

// Secret Redaction regexes (Section 15.8)
const SECRET_PATTERNS = [
  /password\s*[:=]\s*['"]?[^'"\s]+/gi,
  /api[_-]?key\s*[:=]\s*['"]?[^'"\s]+/gi,
  /authorization\s*[:=]\s*['"]?[^'"\s]+/gi,
  /bearer\s+[a-zA-Z0-9_.-]+/gi,
  /\b\d{14}\b/g // Egyptian National ID format as sensitive data example
];

export function redactSensitiveData(input: string): string {
  let cleaned = input;
  for (const pattern of SECRET_PATTERNS) {
    cleaned = cleaned.replace(pattern, "[REDACTED]");
  }
  return cleaned;
}

class RollingLogger {
  private buffer: LogEntry[] = [];
  private maxCapacity = 200;

  public log(
    level: "INFO" | "WARN" | "ERROR",
    event: string,
    message: string,
    error_code?: string,
    meta?: Record<string, unknown>
  ) {
    const entry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      service: "web-client",
      event,
      ...(error_code !== undefined ? { error_code } : {}),
      message: redactSensitiveData(message),
      ...(meta !== undefined
        ? {
            meta: JSON.parse(redactSensitiveData(JSON.stringify(meta))) as Record<string, unknown>
          }
        : {})
    };

    this.buffer.push(entry);
    if (this.buffer.length > this.maxCapacity) {
      this.buffer.shift(); // rolling buffer drop oldest
    }
  }

  public getEntries(): LogEntry[] {
    return [...this.buffer];
  }

  public clear() {
    this.buffer = [];
  }

  public exportLogs(): string {
    return JSON.stringify(this.buffer, null, 2);
  }
}

export const logger = new RollingLogger();
