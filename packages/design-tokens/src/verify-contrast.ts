/**
 * SafiData Design Tokens — Automated WCAG 2.2 AA Contrast Gate CLI (T1.01)
 * Fails the build (exit code 1) if any text pair drops below 4.5:1
 * or any UI/chart element drops below 3.0:1 in Light or Dark mode.
 */

import { COLOR_TOKENS, DARK_COLORS, LIGHT_COLORS, type ThemeColorTokens } from "./colors.ts";
import {
  WCAG_AA_TEXT_MIN_RATIO,
  WCAG_AA_UI_GRAPHIC_MIN_RATIO,
  auditAllThemesContrast
} from "./contrast.ts";

interface RuntimeProcess {
  readonly argv?: readonly string[];
  exitCode?: number;
}

export function runContrastVerification(args: readonly string[] = []): boolean {
  const isSelfTest = args.includes("--self-test");

  if (isSelfTest) {
    const lowTextPalette: ThemeColorTokens = {
      ...LIGHT_COLORS,
      textMuted: "#64748B"
    };
    const lowTextAudit = auditAllThemesContrast({
      light: lowTextPalette,
      dark: DARK_COLORS
    });
    const caughtLowText = lowTextAudit.failures.some(
      (f) =>
        f.theme === "light" &&
        f.category === "text" &&
        f.foregroundToken === "textMuted" &&
        f.backgroundToken === "bg"
    );
    if (lowTextAudit.passed || !caughtLowText) {
      console.error(
        "ERROR [design-tokens/contrast --self-test]: Failed to reject text contrast < 4.5:1 (#64748B on #F5F7FA)."
      );
      return false;
    }

    const lowGraphicPalette: ThemeColorTokens = {
      ...LIGHT_COLORS,
      primary: "#7FA8B0"
    };
    const lowGraphicAudit = auditAllThemesContrast({
      light: lowGraphicPalette,
      dark: DARK_COLORS
    });
    const caughtLowGraphic = lowGraphicAudit.failures.some(
      (f) => f.theme === "light" && f.category === "ui-graphic" && f.foregroundToken === "primary"
    );
    if (lowGraphicAudit.passed || !caughtLowGraphic) {
      console.error(
        "ERROR [design-tokens/contrast --self-test]: Failed to reject UI/chart element contrast < 3.0:1."
      );
      return false;
    }

    console.log(
      "[design-tokens/contrast --self-test] OK: Injected text (< 4.5:1) and UI/chart (< 3.0:1) violations were caught and rejected."
    );
    return true;
  }

  const report = auditAllThemesContrast(COLOR_TOKENS);
  if (!report.passed) {
    for (const failure of report.failures) {
      console.error(
        `ERROR [design-tokens/contrast]: [${failure.theme}] ${failure.category} pair ${failure.foregroundToken} (${failure.foregroundHex}) on ${failure.backgroundToken} (${failure.backgroundHex}) has contrast ${failure.ratio.toFixed(2)}:1 (minimum required: ${failure.minRequired.toFixed(1)}:1).`
      );
    }
    return false;
  }

  const textChecks = report.checks.filter((c) => c.category === "text");
  const graphicChecks = report.checks.filter((c) => c.category === "ui-graphic");
  const minTextRatio = Math.min(...textChecks.map((c) => c.ratio));
  const minGraphicRatio = Math.min(...graphicChecks.map((c) => c.ratio));

  console.log(
    `[design-tokens/contrast] OK: Verified ${textChecks.length} text pairs (min ${minTextRatio.toFixed(2)}:1 >= ${WCAG_AA_TEXT_MIN_RATIO.toFixed(1)}:1) and ${graphicChecks.length} UI/chart pairs (min ${minGraphicRatio.toFixed(2)}:1 >= ${WCAG_AA_UI_GRAPHIC_MIN_RATIO.toFixed(1)}:1) across Light and Dark themes.`
  );
  return true;
}

const runtimeProcess = (globalThis as { process?: RuntimeProcess }).process;
if (runtimeProcess?.argv) {
  const cliArgs = runtimeProcess.argv.slice(2);
  if (cliArgs.includes("--run") || cliArgs.includes("--self-test")) {
    const ok = runContrastVerification(cliArgs);
    if (!ok) {
      runtimeProcess.exitCode = 1;
      throw new Error("[SF-SYS-001] WCAG 2.2 AA contrast verification failed.");
    }
  }
}
