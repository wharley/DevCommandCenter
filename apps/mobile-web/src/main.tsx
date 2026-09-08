import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import { router } from "./router";
import "./index.css";
import { registerPwa } from "./lib/pwa";
void registerPwa();

const rootEl = document.getElementById("root");
if (!rootEl) {
	throw new Error("missing #root");
}

createRoot(rootEl).render(
	<StrictMode>
		<RouterProvider router={router} />
	</StrictMode>,
);
