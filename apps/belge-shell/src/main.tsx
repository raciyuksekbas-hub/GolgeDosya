import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./shared-ui/tokens.css";
import "./shared-ui/shell.css";

// Yalnız geliştirme derlemesinde ve yalnız Tauri yokken. Üretim paketinde bu
// dal ölü koddur ve bundle'a girmez (import.meta.env.DEV sabit false).
if (import.meta.env.DEV) {
  const { installDevMock } = await import("./devMock");
  installDevMock();
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
