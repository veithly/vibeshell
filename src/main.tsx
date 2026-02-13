import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n"; // Initialize i18n before App
import App from "./App";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
