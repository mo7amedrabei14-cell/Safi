/**
 * SafiData boot coordinator.
 *
 * The product application is code-split and preloaded during the intro, but it
 * is not mounted until the entrance animation has completed.
 */
import { Suspense, lazy, useCallback, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { SafiIntroSplash } from "./components/brand/SafiIntroSplash";

const loadApplicationShell = async () => {
  const module = await import("./ApplicationShell");
  return { default: module.ApplicationShell };
};

const ApplicationShell = lazy(loadApplicationShell);

export function App(): ReactNode {
  const [applicationRequested, setApplicationRequested] = useState(false);
  const [applicationReady, setApplicationReady] = useState(false);
  const [introMounted, setIntroMounted] = useState(true);

  useEffect(() => {
    const preloadTimer = window.setTimeout(() => {
      void loadApplicationShell();
    }, 1_200);
    return () => window.clearTimeout(preloadTimer);
  }, []);

  const requestApplication = useCallback((): void => {
    setApplicationRequested(true);
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
        open={introMounted}
        exiting={applicationReady}
        onAnimationComplete={requestApplication}
        onExitComplete={unmountIntro}
      />
      {applicationRequested ? (
        <Suspense fallback={null}>
          <ApplicationShell onReady={markApplicationReady} />
        </Suspense>
      ) : null}
    </>
  );
}
