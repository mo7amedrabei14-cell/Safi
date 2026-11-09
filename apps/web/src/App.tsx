/**
 * SafiData boot coordinator.
 *
 * The product application is intentionally code-split and is not imported or
 * mounted until the intro animation has completed.
 */
import { Suspense, lazy, useCallback, useState } from "react";
import type { ReactNode } from "react";
import { SafiIntroSplash } from "./components/brand/SafiIntroSplash";

const ApplicationShell = lazy(async () => {
  const module = await import("./ApplicationShell");
  return { default: module.ApplicationShell };
});

export function App(): ReactNode {
  const [applicationRequested, setApplicationRequested] = useState(false);
  const [applicationReady, setApplicationReady] = useState(false);

  const requestApplication = useCallback((): void => {
    setApplicationRequested(true);
  }, []);

  const markApplicationReady = useCallback((): void => {
    setApplicationReady(true);
  }, []);

  return (
    <>
      <SafiIntroSplash
        open={!applicationReady}
        onAnimationComplete={requestApplication}
      />
      {applicationRequested ? (
        <Suspense fallback={null}>
          <ApplicationShell onReady={markApplicationReady} />
        </Suspense>
      ) : null}
    </>
  );
}
