import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowUpRight, Database, Loader2, ShieldAlert, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import type { McpIntegrationRecord } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	cancelMcpConnect,
	removeMcpIntegration,
	testMcpIntegration,
} from "@/lib/mcp-api";
import { openExternal } from "@/lib/shell-api";
import { connectorBrandColor, ConnectorGlyph } from "./connector-logos";
import {
	connectIntegration,
	ensureConnectorIntegration,
	findIntegrationForUrl,
	type ConnectOutcome,
	type ConnectorTarget,
} from "./connector-model";

type Phase =
	| { type: "intro" }
	| { type: "waiting" }
	| { type: "failed"; message: string };

type ConnectDialogProps = {
	target: ConnectorTarget | null;
	integrations: McpIntegrationRecord[];
	onOpenChange: (open: boolean) => void;
	onFinished: () => void;
};

export function ConnectDialog({
	target,
	integrations,
	onOpenChange,
	onFinished,
}: ConnectDialogProps) {
	const { t } = useTranslation();
	const [phase, setPhase] = useState<Phase>({ type: "intro" });
	const [token, setToken] = useState("");
	const loginId = useRef<string | null>(null);
	const createdId = useRef<string | null>(null);
	const active = useRef(false);

	useEffect(() => {
		setPhase({ type: "intro" });
		setToken("");
		loginId.current = null;
	}, [target]);

	if (!target) {
		return (
			<Dialog open={false} onOpenChange={onOpenChange}>
				{null}
			</Dialog>
		);
	}
	const name = target.name;
	const usesToken = target.preset?.auth === "token";
	// An uploaded logo, or the one already saved for this connector.
	const logoUri =
		target.logo ??
		(target.preset ? null : (findIntegrationForUrl(integrations, target.url)?.logo ?? null));
	const glow = connectorBrandColor(target.preset?.logoId ?? null);

	const close = () => {
		if (loginId.current) {
			void cancelMcpConnect({ loginId: loginId.current }).catch(() => undefined);
			loginId.current = null;
		}
		active.current = false;
		onOpenChange(false);
	};

	const discardCreated = async () => {
		const definitionId = createdId.current;
		createdId.current = null;
		if (definitionId) {
			await removeMcpIntegration({ definitionId, deleteCredentials: true }).catch(
				() => undefined,
			);
		}
	};

	const failureText = (outcome: Extract<ConnectOutcome, { kind: "failed" }>) =>
		t(`settings.connectors.failure.${outcome.reason}`, {
			name,
			message: outcome.message,
		});

	const finishConnected = (toolCount: number | null) => {
		toast.success(
			toolCount === null
				? t("settings.connectors.toast.connected", { name })
				: t("settings.connectors.toast.connectedTools", { name, count: toolCount }),
		);
		active.current = false;
		onFinished();
		onOpenChange(false);
	};

	const run = async () => {
		active.current = true;
		setPhase({ type: "waiting" });
		try {
			if (usesToken) {
				// A new token replaces the old one rather than adding a second
				// connector with the same URL.
				const previous = findIntegrationForUrl(integrations, target.url);
				const integration = await ensureConnectorIntegration(
					target,
					integrations,
					token.trim(),
				);
				const { check } = await testMcpIntegration({
					definitionId: integration.definition.id,
				});
				if (check.state !== "connected") {
					await removeMcpIntegration({
						definitionId: integration.definition.id,
						deleteCredentials: true,
					});
					setPhase({
						type: "failed",
						message: check.message ?? t("settings.connectors.status.failed"),
					});
					return;
				}
				if (previous) {
					await removeMcpIntegration({
						definitionId: previous.definition.id,
						deleteCredentials: true,
					}).catch(() => undefined);
				}
				finishConnected(check.tools?.length ?? 0);
				return;
			}

			// A connector added by this attempt is removed again if the sign-in
			// does not finish, so a half-set-up connector never reaches the
			// agents.
			const isNew = !findIntegrationForUrl(integrations, target.url);
			createdId.current = null;
			const integration = await ensureConnectorIntegration(target, integrations);
			if (isNew) createdId.current = integration.definition.id;
			const outcome = await connectIntegration(integration, {
				openBrowser: openExternal,
				onLoginStarted: (id) => {
					loginId.current = id;
				},
			});
			loginId.current = null;
			if (outcome.kind !== "connected") {
				await discardCreated();
			} else {
				createdId.current = null;
			}
			if (!active.current) return;
			if (outcome.kind === "connected") {
				finishConnected(outcome.check?.tools?.length ?? null);
			} else if (outcome.reason === "cancelled") {
				close();
			} else {
				setPhase({ type: "failed", message: failureText(outcome) });
			}
		} catch (error) {
			await discardCreated();
			if (!active.current) return;
			setPhase({
				type: "failed",
				message: error instanceof Error ? error.message : String(error),
			});
		} finally {
			onFinished();
		}
	};

	return (
		<Dialog
			open
			onOpenChange={(open) => {
				if (!open) close();
			}}
		>
			<DialogContent className="w-[440px] max-w-[calc(100vw-4rem)] gap-0 overflow-hidden p-0 sm:max-w-[440px]">
				<div
					className="flex flex-col items-center px-6 pt-8 pb-5"
					style={
						glow
							? {
									background: `radial-gradient(ellipse at top, color-mix(in oklab, ${glow} 22%, transparent), transparent 70%)`,
								}
							: undefined
					}
				>
					<ConnectorGlyph
						logoId={target.preset?.logoId ?? null}
						logoUri={logoUri}
						name={name}
						className="size-12 rounded-xl [&_svg]:size-6"
					/>
					<DialogTitle className="mt-3 text-[16px] font-semibold">
						{t("settings.connectors.connect.title", { name })}
					</DialogTitle>
					<DialogDescription className="sr-only">
						{t("settings.connectors.connect.controlBody", { name })}
					</DialogDescription>
				</div>

				{phase.type === "waiting" ? (
					<div className="flex flex-col items-center gap-2 px-6 pb-6 text-center">
						<Loader2 className="size-5 animate-spin text-muted-foreground" />
						<p className="text-[13px] font-medium text-foreground">
							{t("settings.connectors.connect.waiting", { name })}
						</p>
						{usesToken ? null : (
							<p className="text-[12px] text-muted-foreground">
								{t("settings.connectors.connect.waitingHint")}
							</p>
						)}
						<Button type="button" variant="ghost" size="sm" className="mt-2" onClick={close}>
							<span className="text-[12px]">{t("settings.connectors.connect.cancel")}</span>
						</Button>
					</div>
				) : (
					<div className="space-y-4 px-6 pb-6">
						<Point icon={ShieldCheck} title={t("settings.connectors.connect.controlTitle")}>
							{t("settings.connectors.connect.controlBody", { name })}
						</Point>
						<Point icon={ShieldAlert} title={t("settings.connectors.connect.riskTitle")}>
							{t("settings.connectors.connect.riskBody", { name })}
						</Point>
						<Point icon={Database} title={t("settings.connectors.connect.dataTitle")}>
							{t("settings.connectors.connect.dataBody", { name })}
							{target.preset ? (
								<>
									{" "}
									<button
										type="button"
										className="text-foreground underline underline-offset-2"
										onClick={() => void openExternal(target.preset!.docsUrl)}
									>
										<span className="text-[12px]">
											{t("settings.connectors.connect.learnMore")}
										</span>
									</button>
								</>
							) : null}
						</Point>

						{usesToken ? (
							<div className="space-y-1.5 rounded-xl border border-border/60 p-3">
								<Label htmlFor="connector-token" className="text-[12px]">
									{t("settings.connectors.connect.tokenLabel")}
								</Label>
								<p className="text-[11px] leading-relaxed text-muted-foreground">
									{t("settings.connectors.connect.tokenHint", { name })}{" "}
									{target.preset?.tokenHelpUrl ? (
										<button
											type="button"
											className="text-foreground underline underline-offset-2"
											onClick={() => void openExternal(target.preset!.tokenHelpUrl!)}
										>
											<span className="text-[11px]">
												{t("settings.connectors.connect.createToken", { name })}
											</span>
										</button>
									) : null}
								</p>
								<Input
									id="connector-token"
									type="password"
									autoComplete="off"
									spellCheck={false}
									value={token}
									onChange={(event) => setToken(event.target.value)}
									className="h-8 font-mono text-[12px]"
								/>
							</div>
						) : null}

						{phase.type === "failed" ? (
							<p className="rounded-lg bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
								{phase.message}
							</p>
						) : null}

						<Button
							type="button"
							className="h-9 w-full rounded-full"
							disabled={usesToken && token.trim().length === 0}
							onClick={() => void run()}
						>
							<span className="text-[13px]">
								{usesToken
									? t("settings.connectors.connect.connectWithToken")
									: t("settings.connectors.connect.continue", { name })}
							</span>
							{usesToken ? null : <ArrowUpRight className="size-4" />}
						</Button>
					</div>
				)}
			</DialogContent>
		</Dialog>
	);
}

function Point({
	icon: Icon,
	title,
	children,
}: {
	icon: typeof ShieldCheck;
	title: string;
	children: React.ReactNode;
}) {
	return (
		<div className="flex gap-3">
			<Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
			<div>
				<p className="text-[12px] font-medium text-foreground">{title}</p>
				<p className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">{children}</p>
			</div>
		</div>
	);
}
