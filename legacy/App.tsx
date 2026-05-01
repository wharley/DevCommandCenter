import { Routes, Route, Navigate } from "react-router-dom";
import { ThemeProvider } from "@/components/theme-provider";
import { ConfirmDialogProvider } from "@/components/providers/confirm-dialog-provider";
import { Toaster } from "@/components/ui/sonner";
import CmuxWorkspacePage from "@/legacy/pages/CmuxWorkspacePage";

function AppContent() {
  return (
    <Routes>
      <Route path="/" element={<CmuxWorkspacePage />} />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}

export default function App() {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="dark"
      enableSystem
      disableTransitionOnChange
    >
      <ConfirmDialogProvider>
        <AppContent />
        <Toaster />
      </ConfirmDialogProvider>
    </ThemeProvider>
  );
}
