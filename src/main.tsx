import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, MemoryRouter } from "react-router-dom";
import App from "./App";

// Fontes Geist
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";

import "./globals.css";

// Detectar se está rodando no Electron
// Em produção (Electron), usa MemoryRouter para compatibilidade com file://
// Em desenvolvimento (browser), usa BrowserRouter
const isElectron = !!window.electronAPI;
const Router = isElectron ? MemoryRouter : BrowserRouter;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Router initialEntries={isElectron ? ["/"] : undefined}>
      <App />
    </Router>
  </React.StrictMode>
);
