/**
 * `apps/web/src/components/ui/DecryptedMonoText.tsx`
 *
 * React Bits "decrypted text" for cryptographic fingerprints (BLAKE3, SHA-256,
 * Ed25519). Hex characters scramble for `duration` seconds, then the element
 * settles on the real value.
 *
 * The scramble writes straight to `textContent` (no React re-render per frame),
 * and the server-rendered markup already contains the final value, so the element
 * is readable even if JavaScript never runs.
 */
import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import { canAnimate } from "./motion-preference";

export interface DecryptedMonoTextProps {
  /** The real fingerprint / identifier to reveal. */
  readonly value: string;
  /** Reveal duration in seconds. */
  readonly duration?: number;
  /** Milliseconds between scramble frames. */
  readonly frameMs?: number;
  readonly className?: string;
  readonly title?: string;
}

const HEX_ALPHABET = "0123456789abcdef";

function scrambleChar(seed: number): string {
  return HEX_ALPHABET[seed % HEX_ALPHABET.length] ?? "0";
}

export function DecryptedMonoText({
  value,
  duration = 0.6,
  frameMs = 34,
  className,
  title
}: DecryptedMonoTextProps): ReactNode {
  const textRef = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    const element = textRef.current;
    if (element === null || !canAnimate() || value.length === 0) {
      return;
    }

    const totalFrames = Math.max(1, Math.round((duration * 1000) / frameMs));
    let frame = 0;
    let seed = 7;

    const interval = window.setInterval(() => {
      frame += 1;
      seed = (seed * 31 + frame) % 9973;
      const revealed = Math.floor((frame / totalFrames) * value.length);

      let rendered = "";
      for (let index = 0; index < value.length; index += 1) {
        const char = value[index] ?? "";
        if (index < revealed || char === " " || char === ":" || char === "-") {
          rendered += char;
        } else {
          rendered += scrambleChar(seed + index * 17 + frame);
        }
      }

      element.textContent = rendered;

      if (frame >= totalFrames) {
        window.clearInterval(interval);
        element.textContent = value;
      }
    }, frameMs);

    return () => {
      window.clearInterval(interval);
      element.textContent = value;
    };
  }, [duration, frameMs, value]);

  return (
    <span
      ref={textRef}
      className={className === undefined ? "safi-mono" : `safi-mono ${className}`}
      title={title ?? value}
      style={{ fontVariantNumeric: "tabular-nums", letterSpacing: "0.01em" }}
      data-safi-decrypted="true"
    >
      {value}
    </span>
  );
}
