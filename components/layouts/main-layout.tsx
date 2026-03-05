import React from "react";

import { AppSidebar } from "@/components/app-sidebar";
import { ConfirmDialogProvider } from "@/components/providers/confirm-dialog-provider";
import { ThemeProvider } from "@/components/theme-provider";
import { Toaster } from "@/components/ui/sonner";

interface MainLayoutProps {
  children: React.ReactNode;
}

export function MainLayout({ children }: MainLayoutProps) {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="dark"
      enableSystem
      disableTransitionOnChange
    >
      <ConfirmDialogProvider>
        <div className="flex h-screen overflow-hidden bg-background">
          {/* Titlebar arrastável */}
          <div className="electron-drag fixed top-0 left-0 right-0 h-8 z-50" />
          <AppSidebar />
          <main className="flex-1 overflow-auto">{children}</main>
        </div>
        <Toaster />
      </ConfirmDialogProvider>
    </ThemeProvider>
  );
}
