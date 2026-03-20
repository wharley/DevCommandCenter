import { useEffect, useState } from "react";
import { Routes, Route, Navigate } from "react-router-dom";
import { ThemeProvider } from "@/components/theme-provider";
import { ConfirmDialogProvider } from "@/components/providers/confirm-dialog-provider";
import { Toaster } from "@/components/ui/sonner";
import HiveWorkspacePage from "@/src/pages/HiveWorkspacePage";
import ActivationPage from "@/src/pages/ActivationPage";

const isElectron = typeof window !== "undefined" && !!window.electronAPI;

function AppContent() {
  const [licenseLoading, setLicenseLoading] = useState(isElectron);
  const [isActivated, setIsActivated] = useState(false);

  useEffect(() => {
    if (!isElectron || !window.electronAPI?.license) {
      setLicenseLoading(false);
      setIsActivated(true);
      return;
    }
    window.electronAPI.license
      .getStatus()
      .then((status) => {
        setIsActivated(!!status?.activated);
      })
      .catch(() => setIsActivated(false))
      .finally(() => setLicenseLoading(false));
  }, []);

  if (isElectron && licenseLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-background">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  if (isElectron && !isActivated) {
    return <ActivationPage onActivated={() => setIsActivated(true)} />;
  }

  return (
    <Routes>
      <Route path="/" element={<HiveWorkspacePage />} />
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
