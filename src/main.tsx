import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n"; // Initialize i18n before App
import App from "./App";
import "./styles.css";
import { CloudSyncLifecycle } from "./components/CloudSyncLifecycle";
import { DetachedWindow } from "./components/DetachedWindow";
import { parseDetachTarget } from "./lib/detach";

// A torn-out tab opens the same frontend with a `detach` query payload and
// skips the full app shell (session bootstrap, sync loops) entirely.
const detachTarget = parseDetachTarget(window.location.search);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {detachTarget ? (
      <DetachedWindow target={detachTarget} />
    ) : (
      <>
        <CloudSyncLifecycle />
        <App />
      </>
    )}
  </React.StrictMode>,
);
