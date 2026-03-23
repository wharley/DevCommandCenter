"use client";

import { useState } from "react";
import { Mail, CheckCircle2, Loader2, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";

const isDev =
  typeof window !== "undefined" &&
  (import.meta.env?.DEV ?? process.env.NODE_ENV === "development");

function hasLicenseAPI(): boolean {
  return typeof window !== "undefined" && !!window.desktopAPI?.license;
}
function hasSkipActivation(): boolean {
  return typeof window.desktopAPI?.license?.skipActivation === "function";
}

interface ActivationPageProps {
  onActivated: () => void;
}

export default function ActivationPage({ onActivated }: ActivationPageProps) {
  const [email, setEmail] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!hasLicenseAPI()) {
      setError("Ambiente não suporta ativação.");
      return;
    }
    setLoading(true);
    try {
      const result = await window.desktopAPI!.license.activate(email);
      if (result.success) {
        setSuccess(true);
        setTimeout(() => onActivated(), 1200);
      } else {
        setError(result.message ?? "Falha na ativação.");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Erro inesperado.");
    } finally {
      setLoading(false);
    }
  };

  const handleSkip = async () => {
    if (!hasSkipActivation()) return;
    setLoading(true);
    setError(null);
    try {
      const result = await window.desktopAPI!.license.skipActivation();
      if (result.success) {
        onActivated();
      } else {
        setError("Não foi possível pular a ativação (modo produção?).");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Erro ao pular ativação.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background p-6">
      {/* Subtle background gradient + grid feel */}
      <div
        className="pointer-events-none fixed inset-0 bg-[radial-gradient(ellipse_80%_60%_at_50%_0%,var(--primary)/0.08,transparent_50%)]"
        aria-hidden
      />
      <div
        className="pointer-events-none fixed inset-0 bg-[linear-gradient(to_bottom,transparent_0%,var(--background)_70%)]"
        aria-hidden
      />

      <Card
        className={cn(
          "relative w-full max-w-md border-border/80 bg-card/95 shadow-xl",
          "ring-1 ring-primary/10 dark:ring-primary/20",
          "backdrop-blur-sm",
        )}
      >
        <CardHeader className="space-y-4 pb-2 text-center">
          <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-primary/10 text-primary">
            <Sparkles className="h-7 w-7" />
          </div>
          <div className="space-y-1">
            <CardTitle className="text-2xl font-semibold tracking-tight">
              Ative o Dev Command Center
            </CardTitle>
            <CardDescription className="text-base text-muted-foreground">
              Beta — informe seu e-mail para acessar. Sem spam, só notícias do
              produto.
            </CardDescription>
          </div>
        </CardHeader>

        <CardContent className="space-y-6 pt-2">
          {success ? (
            <div className="flex flex-col items-center gap-4 py-6">
              <div className="flex h-14 w-14 items-center justify-center rounded-full bg-success/15 text-success">
                <CheckCircle2 className="h-8 w-8" />
              </div>
              <p className="text-center font-medium text-foreground">
                Ativado com sucesso!
              </p>
              <p className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                Entrando...
              </p>
            </div>
          ) : (
            <form onSubmit={handleSubmit} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="activation-email">E-mail</Label>
                <div className="relative">
                  <Mail className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    id="activation-email"
                    type="email"
                    placeholder="seu@email.com"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    className="pl-10"
                    autoComplete="email"
                    autoFocus
                    disabled={loading}
                  />
                </div>
              </div>
              {error && (
                <p className="text-sm text-destructive" role="alert">
                  {error}
                </p>
              )}
              <Button
                type="submit"
                className="w-full"
                size="lg"
                disabled={loading || !email.trim()}
              >
                {loading ? (
                  <>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Ativando...
                  </>
                ) : (
                  "Ativar e entrar"
                )}
              </Button>
            </form>
          )}

          {isDev && hasLicenseAPI() && hasSkipActivation() && !success && (
            <div className="border-t border-border/60 pt-4">
              <button
                type="button"
                onClick={handleSkip}
                disabled={loading}
                className="text-center w-full text-xs text-muted-foreground hover:text-foreground underline underline-offset-2 transition-colors cursor-pointer"
              >
                Pular ativação (só desenvolvimento)
              </button>
            </div>
          )}
        </CardContent>
      </Card>

      <p className="mt-8 text-center text-xs text-muted-foreground">
        100% local · Seus dados ficam com você
      </p>
    </div>
  );
}
