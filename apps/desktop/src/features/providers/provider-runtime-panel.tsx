import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCcw } from "lucide-react";
import type { ProviderCatalog } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import {
	getProviderRuntimeDraft,
	supportsProviderRuntime,
	type ProviderRuntimeDraft,
	type ProviderRuntimeSettings,
} from "./provider-runtime-settings";

type ProviderRuntimePanelProps = {
	providers: ProviderCatalog["providers"];
	runtimeSettings: ProviderRuntimeSettings;
	onChangeRuntime: (providerId: string, draft: ProviderRuntimeDraft) => void;
	onClearRuntime: (providerId: string) => void;
	className?: string;
};

function ProviderRuntimeCard({
	provider,
	draft,
	onChangeRuntime,
	onClearRuntime,
}: {
	provider: ProviderCatalog["providers"][number];
	draft: ProviderRuntimeDraft;
	onChangeRuntime: (providerId: string, draft: ProviderRuntimeDraft) => void;
	onClearRuntime: (providerId: string) => void;
}) {
	const { t } = useTranslation("common");

	return (
		<Card className="rounded-2xl border-border/60 bg-muted/10 p-0 shadow-none">
			<CardHeader className="border-b border-border/40 px-4 py-3">
				<div className="min-w-0 flex-1">
					<div className="flex items-center gap-2">
						<CardTitle className="text-[14px] font-medium leading-tight">
							{provider.label}
						</CardTitle>
						<Badge
							variant={provider.stable ? "success" : "outline"}
							className="h-6 px-2 text-[10px]"
						>
							{provider.id}
						</Badge>
					</div>
					<CardDescription className="mt-1 text-[12px] leading-relaxed">
						{t("settings.model.runtimeCardBody")}
					</CardDescription>
				</div>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="h-8 gap-1.5"
					onClick={() => onClearRuntime(provider.id)}
				>
					<RefreshCcw className="size-3.5" />
					{t("settings.model.runtimeReset")}
				</Button>
			</CardHeader>
			<CardContent className="space-y-3 px-4 py-4">
				<div className="space-y-1.5">
					<Label htmlFor={`provider-home-${provider.id}`} className="text-[12px]">
						{t("settings.model.runtimeHomePath")}
					</Label>
					<Input
						id={`provider-home-${provider.id}`}
						value={draft.homePath}
						onChange={(event) =>
							onChangeRuntime(provider.id, {
								...draft,
								homePath: event.target.value,
							})
						}
						placeholder={t("settings.model.runtimeHomePlaceholder")}
						autoComplete="off"
					/>
				</div>

				<div className="space-y-1.5">
					<Label htmlFor={`provider-shadow-${provider.id}`} className="text-[12px]">
						{t("settings.model.runtimeShadowHomePath")}
					</Label>
					<Input
						id={`provider-shadow-${provider.id}`}
						value={draft.shadowHomePath}
						onChange={(event) =>
							onChangeRuntime(provider.id, {
								...draft,
								shadowHomePath: event.target.value,
							})
						}
						placeholder={t("settings.model.runtimeShadowHomePlaceholder")}
						autoComplete="off"
					/>
				</div>
			</CardContent>
		</Card>
	);
}

export function ProviderRuntimePanel({
	providers,
	runtimeSettings,
	onChangeRuntime,
	onClearRuntime,
	className,
}: ProviderRuntimePanelProps) {
	const { t } = useTranslation("common");
	const supportedProviders = useMemo(
		() => providers.filter((provider) => supportsProviderRuntime(provider.id)),
		[providers],
	);

	return (
		<div
			className={cn(
				"space-y-3 rounded-2xl border border-border/60 bg-background p-4",
				className,
			)}
		>
			<div className="flex items-start justify-between gap-4">
				<div className="min-w-0">
					<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						{t("settings.model.runtimeTitle")}
					</p>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
						{t("settings.model.runtimeHint")}
					</p>
				</div>
				<Badge
					variant="outline"
					className="h-8 px-3 text-[12px] font-normal text-muted-foreground"
				>
					{t("settings.model.runtimeBadge")}
				</Badge>
			</div>

			{supportedProviders.length === 0 ? (
				<div className="rounded-xl border border-dashed border-border/60 bg-muted/10 p-4 text-[12px] text-muted-foreground">
					{t("settings.model.runtimeNoSupportedProviders")}
				</div>
			) : (
				<div className="grid gap-3 xl:grid-cols-2">
					{supportedProviders.map((provider) => (
						<ProviderRuntimeCard
							key={provider.id}
							provider={provider}
							draft={getProviderRuntimeDraft(runtimeSettings, provider.id)}
							onChangeRuntime={onChangeRuntime}
							onClearRuntime={onClearRuntime}
						/>
					))}
				</div>
			)}
		</div>
	);
}
