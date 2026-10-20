import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	ArrowUpRight,
	Check,
	Copy,
	Database,
	Loader2,
	ShieldAlert,
	ShieldCheck,
} from "lucide-react";
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
import {
	slackAppManifest,
	slackCreateAppUrl,
	USER_APP_REDIRECT_URI,
} from "./user-app";

type Phase =
	| { type: "intro" }
	| { type: "waiting" }
	| { type: "failed"; message: string }
	/** The server registers no clients: the user's own app or a token. */
	| { type: "needsApp" };

type Attempt =
	| { kind: "oauth"; clientId?: string; clientSecret?: string }
	| { kind: "token"; token: string };

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
	const [clientId, setClientId] = useState("");
	const [clientSecret, setClientSecret] = useState("");
	const loginId = useRef<string | null>(null);
	const createdId = useRef<string | null>(null);
	const active = useRef(false);

	useEffect(() => {
		setPhase({ type: "intro" });
		setToken("");
		setClientId("");
		setClientSecret("");
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
	const usesSlackApp = target.preset?.auth === "slackApp";
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

	const connectWithToken = async (value: string) => {
		// A new token replaces the old one rather than adding a second
		// connector with the same URL.
		const previous = findIntegrationForUrl(integrations, target.url);
		const integration = await ensureConnectorIntegration(target, integrations, value);
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
	};

	const run = async (attempt: Attempt) => {
		active.current = true;
		setPhase({ type: "waiting" });
		try {
			if (attempt.kind === "token") {
				await connectWithToken(attempt.token);
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
				clientId: attempt.clientId,
				clientSecret: attempt.clientSecret,
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
			} else if (outcome.reason === "clientRequired" && !usesSlackApp) {
				setPhase({ type: "needsApp" });
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

	const primary = () => {
		if (usesToken) return run({ kind: "token", token: token.trim() });
		if (usesSlackApp) return run({ kind: "oauth", clientId: clientId.trim() });
		return run({ kind: "oauth" });
	};
	const primaryDisabled =
		(usesToken && token.trim().length === 0) ||
		(usesSlackApp && clientId.trim().length === 0);

	return (
		<Dialog
			open
			onOpenChange={(open) => {
				if (!open) close();
			}}
		>
			<DialogContent className="max-h-[calc(100vh-4rem)] w-[440px] max-w-[calc(100vw-4rem)] gap-0 overflow-y-auto p-0 sm:max-w-[440px]">
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
				) : phase.type === "needsApp" ? (
					<OwnAppPanel
						name={name}
						clientId={clientId}
						clientSecret={clientSecret}
						token={token}
						onClientIdChange={setClientId}
						onClientSecretChange={setClientSecret}
						onTokenChange={setToken}
						onConnectApp={() =>
							void run({
								kind: "oauth",
								clientId: clientId.trim(),
								clientSecret: clientSecret.trim() || undefined,
							})
						}
						onConnectToken={() => void run({ kind: "token", token: token.trim() })}
					/>
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

						{usesSlackApp ? (
							<SlackAppSteps clientId={clientId} onClientIdChange={setClientId} />
						) : null}

						{phase.type === "failed" ? (
							<p className="rounded-lg bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
								{phase.message}
							</p>
						) : null}

						<Button
							type="button"
							className="h-9 w-full rounded-full"
							disabled={primaryDisabled}
							onClick={() => void primary()}
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

/** Slack registers no clients: three steps to the user's own internal app. */
function SlackAppSteps({
	clientId,
	onClientIdChange,
}: {
	clientId: string;
	onClientIdChange: (value: string) => void;
}) {
	const { t } = useTranslation();
	return (
		<div className="space-y-3 rounded-xl border border-border/60 p-3">
			<p className="text-[11px] leading-relaxed text-muted-foreground">
				{t("settings.connectors.connect.slack.intro")}
			</p>
			<Step number={1} title={t("settings.connectors.connect.slack.createTitle")}>
				<div className="mt-1.5 flex flex-wrap items-center gap-2">
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="h-7"
						onClick={() => void openExternal(slackCreateAppUrl())}
					>
						<span className="text-[12px]">{t("settings.connectors.connect.slack.createButton")}</span>
						<ArrowUpRight className="size-3.5" />
					</Button>
					<CopyButton
						value={JSON.stringify(slackAppManifest(), null, 2)}
						label={t("settings.connectors.connect.slack.copyManifest")}
					/>
				</div>
			</Step>
			<Step number={2} title={t("settings.connectors.connect.slack.installTitle")}>
				<p className="text-[11px] leading-relaxed text-muted-foreground">
					{t("settings.connectors.connect.slack.installBody")}
				</p>
			</Step>
			<Step number={3} title={t("settings.connectors.connect.slack.clientIdTitle")}>
				<p className="text-[11px] leading-relaxed text-muted-foreground">
					{t("settings.connectors.connect.slack.clientIdBody")}
				</p>
				<Input
					aria-label={t("settings.connectors.connect.clientIdLabel")}
					autoComplete="off"
					spellCheck={false}
					value={clientId}
					onChange={(event) => onClientIdChange(event.target.value)}
					placeholder="1234567890.1234567890123"
					className="mt-1.5 h-8 font-mono text-[12px]"
				/>
			</Step>
		</div>
	);
}

/**
 * Shown when a server refuses DCC's own registration: the user's app (with
 * DCC's redirect to register) or a token.
 */
function OwnAppPanel({
	name,
	clientId,
	clientSecret,
	token,
	onClientIdChange,
	onClientSecretChange,
	onTokenChange,
	onConnectApp,
	onConnectToken,
}: {
	name: string;
	clientId: string;
	clientSecret: string;
	token: string;
	onClientIdChange: (value: string) => void;
	onClientSecretChange: (value: string) => void;
	onTokenChange: (value: string) => void;
	onConnectApp: () => void;
	onConnectToken: () => void;
}) {
	const { t } = useTranslation();
	return (
		<div className="space-y-4 px-6 pb-6">
			<p className="text-[12px] leading-relaxed text-muted-foreground">
				{t("settings.connectors.connect.ownApp.intro", { name })}
			</p>
			<div className="space-y-2 rounded-xl border border-border/60 p-3">
				<p className="text-[12px] font-medium text-foreground">
					{t("settings.connectors.connect.ownApp.title")}
				</p>
				<div className="space-y-1">
					<Label className="text-[11px] text-muted-foreground">
						{t("settings.connectors.connect.ownApp.redirectLabel")}
					</Label>
					<div className="flex items-center gap-1.5">
						<code className="min-w-0 flex-1 truncate rounded-md bg-muted px-2 py-1 font-mono text-[11px]">
							{USER_APP_REDIRECT_URI}
						</code>
						<CopyButton value={USER_APP_REDIRECT_URI} label={t("settings.connectors.connect.copy")} />
					</div>
				</div>
				<Input
					aria-label={t("settings.connectors.connect.clientIdLabel")}
					placeholder={t("settings.connectors.connect.clientIdLabel")}
					autoComplete="off"
					spellCheck={false}
					value={clientId}
					onChange={(event) => onClientIdChange(event.target.value)}
					className="h-8 font-mono text-[12px]"
				/>
				<Input
					aria-label={t("settings.connectors.connect.ownApp.secretLabel")}
					placeholder={t("settings.connectors.connect.ownApp.secretLabel")}
					type="password"
					autoComplete="off"
					spellCheck={false}
					value={clientSecret}
					onChange={(event) => onClientSecretChange(event.target.value)}
					className="h-8 font-mono text-[12px]"
				/>
				<Button
					type="button"
					className="h-8 w-full rounded-full"
					disabled={clientId.trim().length === 0}
					onClick={onConnectApp}
				>
					<span className="text-[12px]">{t("settings.connectors.connect.continue", { name })}</span>
					<ArrowUpRight className="size-4" />
				</Button>
			</div>
			<div className="space-y-2 rounded-xl border border-border/60 p-3">
				<p className="text-[12px] font-medium text-foreground">
					{t("settings.connectors.connect.ownApp.tokenTitle")}
				</p>
				<Input
					aria-label={t("settings.connectors.connect.tokenLabel")}
					placeholder={t("settings.connectors.connect.tokenLabel")}
					type="password"
					autoComplete="off"
					spellCheck={false}
					value={token}
					onChange={(event) => onTokenChange(event.target.value)}
					className="h-8 font-mono text-[12px]"
				/>
				<Button
					type="button"
					variant="outline"
					className="h-8 w-full rounded-full"
					disabled={token.trim().length === 0}
					onClick={onConnectToken}
				>
					<span className="text-[12px]">{t("settings.connectors.connect.connectWithToken")}</span>
				</Button>
			</div>
		</div>
	);
}

function Step({
	number,
	title,
	children,
}: {
	number: number;
	title: string;
	children: React.ReactNode;
}) {
	return (
		<div className="flex gap-2.5">
			<span className="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full bg-muted text-[10px] font-medium text-muted-foreground">
				{number}
			</span>
			<div className="min-w-0 flex-1">
				<p className="text-[12px] font-medium text-foreground">{title}</p>
				{children}
			</div>
		</div>
	);
}

function CopyButton({ value, label }: { value: string; label: string }) {
	const { t } = useTranslation();
	const [copied, setCopied] = useState(false);
	return (
		<Button
			type="button"
			variant="ghost"
			size="sm"
			className="h-7 shrink-0"
			onClick={() => {
				void navigator.clipboard
					.writeText(value)
					.then(() => {
						setCopied(true);
						window.setTimeout(() => setCopied(false), 1500);
					})
					.catch(() => toast.error(t("settings.connectors.connect.copyFailed")));
			}}
		>
			{copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
			<span className="text-[12px]">{copied ? t("settings.connectors.connect.copied") : label}</span>
		</Button>
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
