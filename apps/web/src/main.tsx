import "@fontsource/ibm-plex-sans-arabic/400.css";
import "@fontsource/ibm-plex-sans-arabic/500.css";
import "@fontsource/ibm-plex-sans-arabic/600.css";
import "@fontsource/ibm-plex-sans-arabic/700.css";
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/600.css";
import "@fontsource/jetbrains-mono/700.css";
import "./index.css";

import { StrictMode, useLayoutEffect } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.tsx";
import { ErrorBoundary } from "./ErrorBoundary.tsx";

function BootScreenHandoff(): null {
  useLayoutEffect(() => {
    // Remove the identical static frame before the browser paints the React handoff.
    document.getElementById("safi-boot-screen")?.remove();
  }, []);

  return null;
}

const rootElement = document.getElementById("root");
if (rootElement !== null) {
  createRoot(rootElement).render(
    <StrictMode>
      <>
        <BootScreenHandoff />
        <ErrorBoundary>
          <App />
        </ErrorBoundary>
      </>
    </StrictMode>
  );
}
