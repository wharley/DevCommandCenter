import React from "react";
import { Bar, BarChart, CartesianGrid, Pie, PieChart, XAxis } from "recharts";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import type { CombStatus } from "@/lib/database/types";
import type {
  DashboardMetrics,
  DashboardPeriodDays,
} from "@/hooks/use-dashboard-metrics";
import { Button } from "@/components/ui/button";

const combColorByStatus: Record<CombStatus, string> = {
  active: "var(--chart-1)",
  ready_for_review: "var(--chart-2)",
  applied: "var(--chart-3)",
  discarded: "var(--chart-4)",
  archived: "var(--chart-5)",
  error: "var(--chart-5)",
};

interface DashboardPageProps {
  selectedProjectName?: string;
  periodDays: DashboardPeriodDays;
  onPeriodChange: (period: DashboardPeriodDays) => void;
  metrics: DashboardMetrics;
}

export default function DashboardPage({
  selectedProjectName,
  periodDays,
  onPeriodChange,
  metrics,
}: DashboardPageProps) {
  const throughputData = [
    { key: "created", label: "Criados", value: metrics.workspacesCreated },
    { key: "applied", label: "Aplicados", value: metrics.workspacesApplied },
  ];

  const throughputConfig: ChartConfig = {
    value: {
      label: "Quantidade",
      color: "var(--chart-1)",
    },
  };

  const combFunnelData = metrics.combStatusCounts
    .filter((item) => item.count > 0)
    .map((item) => ({
      status: item.status,
      label: item.label,
      value: item.count,
      fill: combColorByStatus[item.status],
    }));

  const combFunnelConfig: ChartConfig = {
    value: { label: "Workspaces" },
    ...Object.fromEntries(
      metrics.combStatusCounts.map((item) => [
        item.status,
        { label: item.label, color: combColorByStatus[item.status] },
      ]),
    ),
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <header className="shrink-0 border-b border-border bg-card/80 px-6 py-3 backdrop-blur supports-backdrop-filter:bg-card/60">
        <div className="mx-auto flex max-w-6xl items-center justify-between gap-4">
          <div>
            <h1 className="text-lg font-semibold tracking-tight text-foreground">
              Dashboard
            </h1>
            <p className="text-xs text-muted-foreground">
              {selectedProjectName
                ? `Visão operacional de ${selectedProjectName}`
                : "Selecione um projeto para visualizar métricas"}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant={periodDays === 7 ? "default" : "outline"}
              size="sm"
              onClick={() => onPeriodChange(7)}
            >
              7 dias
            </Button>
            <Button
              variant={periodDays === 30 ? "default" : "outline"}
              size="sm"
              onClick={() => onPeriodChange(30)}
            >
              30 dias
            </Button>
          </div>
        </div>
      </header>

      <div className="flex-1 overflow-auto px-6 py-5">
        <div className="mx-auto grid max-w-6xl grid-cols-1 gap-4 md:grid-cols-2">
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Workspaces criados
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold">{metrics.workspacesCreated}</div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Workspaces aplicados
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold">{metrics.workspacesApplied}</div>
            </CardContent>
          </Card>
        </div>

        <div className="mx-auto mt-4 grid max-w-6xl grid-cols-1 gap-4 lg:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Fluxo de Trabalho</CardTitle>
            </CardHeader>
            <CardContent>
              <ChartContainer config={throughputConfig} className="h-[220px] w-full">
                <BarChart data={throughputData}>
                  <CartesianGrid vertical={false} />
                  <XAxis dataKey="label" axisLine={false} tickLine={false} />
                  <ChartTooltip content={<ChartTooltipContent />} />
                  <Bar dataKey="value" fill="var(--color-value)" radius={6} />
                </BarChart>
              </ChartContainer>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Distribuição de Status</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              {combFunnelData.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  Sem workspaces no projeto selecionado.
                </p>
              ) : (
                <>
                  <ChartContainer config={combFunnelConfig} className="h-[220px] w-full">
                    <PieChart>
                      <Pie data={combFunnelData} dataKey="value" nameKey="label" />
                      <ChartTooltip content={<ChartTooltipContent />} />
                    </PieChart>
                  </ChartContainer>
                  <div className="flex flex-wrap gap-2">
                    {combFunnelData.map((item) => (
                      <Badge key={item.status} variant="outline">
                        {item.label}: {item.value}
                      </Badge>
                    ))}
                  </div>
                </>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
