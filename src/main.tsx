import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, MemoryRouter } from "react-router-dom";
import App from "./App";
import { ErrorBoundary } from "./components/error-boundary";
import { installDesktopBridge } from "./lib/desktop-bridge";

// Fontes Geist
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";

import "./globals.css";

function renderFatalOverlay(title: string, message: string, details?: string) {
  if (typeof document === "undefined") return;

  let host = document.getElementById("dcc-fatal-overlay");
  if (!host) {
    host = document.createElement("div");
    host.id = "dcc-fatal-overlay";
    document.body.appendChild(host);
  }

  host.innerHTML = `
    <div style="
      position: fixed;
      inset: 0;
      z-index: 2147483647;
      display: flex;
      align-items: center;
      justify-content: center;
      background: #0b0d10;
      color: #e5e7eb;
      padding: 24px;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    ">
      <div style="
        width: min(900px, 100%);
        max-height: 90vh;
        overflow: auto;
        border: 1px solid #7f1d1d;
        border-radius: 12px;
        background: #111827;
        box-shadow: 0 10px 30px rgba(0,0,0,.45);
        padding: 16px;
      ">
        <h2 style="margin:0 0 10px 0; color:#f87171; font-size:18px;">${title}</h2>
        <p style="margin:0 0 12px 0; font-size:13px; line-height:1.4;">${message}</p>
        ${
          details
            ? `<pre style="margin:0; white-space:pre-wrap; word-break:break-word; font-size:12px; line-height:1.35; background:#0f172a; border:1px solid #334155; border-radius:8px; padding:12px;">${details}</pre>`
            : ""
        }
      </div>
    </div>
  `;
}

function installGlobalErrorCapture() {
  if (typeof window === "undefined") return;

  window.addEventListener("error", (event) => {
    const message = event.message || "Erro não identificado";
    const stack = event.error?.stack || `${event.filename}:${event.lineno}:${event.colno}`;
    console.error("[GlobalError]", message, event.error);
    renderFatalOverlay("Erro global capturado", message, stack);
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    const message =
      reason instanceof Error
        ? reason.message
        : typeof reason === "string"
          ? reason
          : "Promise rejeitada sem tratamento";
    const stack = reason instanceof Error ? reason.stack : JSON.stringify(reason, null, 2);
    console.error("[UnhandledRejection]", reason);
    renderFatalOverlay("Promise rejeitada sem tratamento", message, stack);
  });
}

// Shell desktop (Tauri): MemoryRouter para file://; browser dev: BrowserRouter
installGlobalErrorCapture();
installDesktopBridge();
const isDesktopShell = !!window.desktopAPI;
const Router = isDesktopShell ? MemoryRouter : BrowserRouter;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary>
      <Router initialEntries={isDesktopShell ? ["/"] : undefined}>
        <App />
      </Router>
    </ErrorBoundary>
  </React.StrictMode>
);
