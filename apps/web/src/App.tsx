/**
 * SafiData boot coordinator.
 *
 * The product application is requested immediately and loads under the short intro.
 */
import { Suspense, lazy, useCallback, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { SafiIntroSplash } from "./components/brand/SafiIntroSplash";
import { SafiCustomScrollbar } from "./components/layout/SafiCustomScrollbar";
import { getInitialSafiLang } from "./lib/locale";

const loadApplicationShell = async () => {
  const module = await import("./ApplicationShell");
  return { default: module.ApplicationShell };
};

const ApplicationShell = lazy(loadApplicationShell);

function readFragmentToken(key: string): string | null {
  if (typeof window === "undefined") return null;

  const url = new URL(window.location.href);
  const fragmentToken = new URLSearchParams(url.hash.slice(1)).get(key);
  return fragmentToken ?? url.searchParams.get(key);
}

function readVerificationToken(): string | null {
  return readFragmentToken("verify_email");
}

function readResetToken(): string | null {
  return readFragmentToken("reset_password");
}

export function App(): ReactNode {
  const [applicationReady, setApplicationReady] = useState(false);
  const [introAnimationComplete, setIntroAnimationComplete] = useState(false);
  const [introMounted, setIntroMounted] = useState(true);
  const [initialLang] = useState(getInitialSafiLang);
  const [verificationToken] = useState(readVerificationToken);
  const [resetToken] = useState(readResetToken);

  useEffect(() => {
    if (verificationToken === null && resetToken === null) return;
    const url = new URL(window.location.href);
    url.searchParams.delete("verify_email");
    url.searchParams.delete("reset_password");
    const fragment = new URLSearchParams(url.hash.slice(1));
    fragment.delete("verify_email");
    fragment.delete("reset_password");
    url.hash = fragment.toString() ? `#${fragment.toString()}` : "";
    window.history.replaceState(
      window.history.state,
      "",
      `${url.pathname}${url.search}${url.hash}`
    );
  }, [verificationToken, resetToken]);

  const markIntroAnimationComplete = useCallback((): void => {
    setIntroAnimationComplete(true);
  }, []);

  const markApplicationReady = useCallback((): void => {
    setApplicationReady(true);
  }, []);

  const unmountIntro = useCallback((): void => {
    setIntroMounted(false);
  }, []);

  return (
    <>
      <SafiIntroSplash
        lang={initialLang}
        open={introMounted}
        exiting={applicationReady && introAnimationComplete}
        onAnimationComplete={markIntroAnimationComplete}
        onExitComplete={unmountIntro}
      />
      <Suspense fallback={null}>
        <ApplicationShell
          onReady={markApplicationReady}
          initialVerificationToken={verificationToken}
          initialResetToken={resetToken}
        />
      </Suspense>
      {/* The custom scrollbar only mounts after the intro hands over to the website. */}
      <SafiCustomScrollbar active={!introMounted} lang={initialLang} />
    </>
  );
}
