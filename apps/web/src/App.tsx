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

function readVerificationToken(): string | null {
  if (typeof window === "undefined") return null;

  const url = new URL(window.location.href);
  const fragmentToken = new URLSearchParams(url.hash.slice(1)).get("verify_email");
  return fragmentToken ?? url.searchParams.get("verify_email");
}

export function App(): ReactNode {
  const [applicationReady, setApplicationReady] = useState(false);
  const [introAnimationComplete, setIntroAnimationComplete] = useState(false);
  const [introMounted, setIntroMounted] = useState(true);
  const [initialLang] = useState(getInitialSafiLang);
  const [verificationToken] = useState(readVerificationToken);

  useEffect(() => {
    if (verificationToken !== null) {
      const url = new URL(window.location.href);
      url.searchParams.delete("verify_email");
      const fragment = new URLSearchParams(url.hash.slice(1));
      if (fragment.has("verify_email")) {
        fragment.delete("verify_email");
        url.hash = fragment.toString() ? `#${fragment.toString()}` : "";
      }
      window.history.replaceState(
        window.history.state,
        "",
        `${url.pathname}${url.search}${url.hash}`
      );
    }
  }, [verificationToken]);

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
        />
      </Suspense>
      {/* The custom scrollbar only mounts after the intro hands over to the website. */}
      <SafiCustomScrollbar active={!introMounted} lang={initialLang} />
    </>
  );
}
