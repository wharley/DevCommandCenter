import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, Copy, LoaderCircle, LogIn, Pencil, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import type { ProviderAccount, ProviderCatalog, ProviderRuntimeConfig } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	cancelProviderAccountSignIn,
	createProviderAccount,
	getProviderAccountStatus,
	getProviderAccountUsage,
	listProviderAccounts,
	removeProviderAccount,
	renameProviderAccount,
	signInProviderAccount,
} from "@/lib/provider-api";
import { cn } from "@/lib/utils";
import { ProviderUsageDetails } from "./provider-account-usage-panel";
import {
	mostConstrainedUsageWindow,
	providerUsageSeverity,
	supportsProviderAccountUsage,
} from "./provider-account-usage";
import {
	accountsOfProvider,
	PROVIDER_ACCOUNTS_QUERY_KEY,
	withProviderAccount,
} from "./provider-accounts";

type Provider = ProviderCatalog["providers"][number];

function errorMessage(error: unknown, fallback: string) {
	return error instanceof Error ? error.message : String(error || fallback);
}

/** How much of the plan is left in its tightest window, as a short bar. */
function PlanMeter({
	provider,
	runtime,
}: {
	provider: Provider;
	runtime: ProviderRuntimeConfig | null;
}) {
	const { t } = useTranslation("common");
	const usage = useQuery({
		// Same key as the expanded details, so both read one fetch.
		queryKey: ["provider-account-usage", provider.id, true, runtime],
		queryFn: async () => (await getProviderAccountUsage(provider.id, runtime)).usage,
		staleTime: 60_000,
		retry: false,
	});
	const tightest = mostConstrainedUsageWindow(usage.data);
	if (!tightest) return null;
	const severity = providerUsageSeverity(tightest);
	const left = Math.round(tightest.remainingPercent);
	return (
		<span className="flex shrink-0 items-center gap-2" title={t("settings.model.accountsMeterHint")}>
			<span className="h-1.5 w-16 overflow-hidden rounded-full bg-muted" aria-hidden>
				<span
					className={cn(
						"block h-full rounded-full bg-foreground/50",
						severity === "warning" && "bg-amber-500",
						severity === "critical" && "bg-destructive",
					)}
					// Fills with what is used, like the usage details below it.
					style={{ width: `${Math.min(100, Math.max(0, tightest.usedPercent))}%` }}
				/>
			</span>
			<span
				className={cn(
					"w-[4.5rem] text-right text-[11px] tabular-nums text-muted-foreground",
					severity === "warning" && "text-amber-600 dark:text-amber-400",
					severity === "critical" && "text-destructive",
				)}
			>
				{tightest.isExhausted
					? t("settings.model.accountsExhausted")
					: t("settings.model.accountsLeft", { percent: left })}
			</span>
		</span>
	);
}

/**
 * One sign-in of the provider: who it is, how much of the plan is left, and
 * its usage details on demand. Signed out, it offers to sign in instead.
 */
function AccountRow({
	provider,
	baseRuntime,
	accountId,
	title,
	badge,
	actions,
}: {
	provider: Provider;
	baseRuntime: ProviderRuntimeConfig | null;
	accountId: string | null;
	title: React.ReactNode;
	badge?: string;
	actions?: React.ReactNode;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [open, setOpen] = useState(false);
	const [signingIn, setSigningIn] = useState(false);
	const runtime = withProviderAccount(baseRuntime, accountId);
	const statusKey = [...PROVIDER_ACCOUNTS_QUERY_KEY, "status", provider.id, accountId];
	const statusQuery = useQuery({
		queryKey: statusKey,
		queryFn: () => getProviderAccountStatus(provider.id, accountId),
		staleTime: 60_000,
	});
	const status = statusQuery.data?.status;
	const signedIn = status?.loggedIn === true;
	const showsUsage = signedIn && supportsProviderAccountUsage(provider);

	async function signIn() {
		setSigningIn(true);
		try {
			const { status: signed } = await signInProviderAccount(provider.id, accountId);
			await queryClient.invalidateQueries({ queryKey: statusKey });
			toast.success(
				signed.email
					? t("settings.model.accountsSignInDoneAs", { email: signed.email })
					: t("settings.model.accountsSignInDone"),
			);
		} catch (error) {
			const message = errorMessage(error, t("settings.model.accountsSignInFailed"));
			if (message !== "sign-in cancelled") toast.error(message);
		} finally {
			setSigningIn(false);
		}
	}

	const who = signedIn ? [status?.email, status?.plan].filter(Boolean).join(" · ") : null;
	const loginCommand = statusQuery.data?.loginCommand;

	return (
		<li className="group">
			<div className="flex min-h-12 items-center gap-3 px-3 py-2">
				<div className="min-w-0 flex-1">
					<div className="flex min-w-0 items-center gap-2">
						<span className="truncate text-[13px] font-medium text-foreground">{title}</span>
						{badge ? (
							<span className="shrink-0 text-[11px] text-muted-foreground">{badge}</span>
						) : null}
					</div>
					<p
						className={cn(
							"mt-0.5 truncate text-[12px]",
							statusQuery.isError || (!statusQuery.isPending && !signedIn)
								? "text-amber-600 dark:text-amber-400"
								: "text-muted-foreground",
						)}
					>
						{statusQuery.isPending
							? t("settings.model.accountsChecking")
							: statusQuery.isError
								? errorMessage(statusQuery.error, t("settings.model.accountsCheckFailed"))
								: signingIn
									? t("settings.model.accountsSigningIn")
									: signedIn
										? who || t("settings.model.accountsSignedIn")
										: t("settings.model.accountsSignedOut")}
					</p>
				</div>
				{actions ? (
					<div className="flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
						{actions}
					</div>
				) : null}
				{showsUsage ? <PlanMeter provider={provider} runtime={runtime} /> : null}
				{signedIn ? (
					showsUsage ? (
						<Button
							type="button"
							variant="ghost"
							size="icon"
							className="size-7 shrink-0"
							aria-expanded={open}
							aria-label={
								open ? t("settings.model.accountsHideUsage") : t("settings.model.accountsShowUsage")
							}
							onClick={() => setOpen((value) => !value)}
						>
							<ChevronDown
								className={cn("size-3.5 transition-transform", open && "rotate-180")}
								aria-hidden
							/>
						</Button>
					) : null
				) : statusQuery.isPending ? null : signingIn ? (
					<Button
						type="button"
						variant="ghost"
						size="sm"
						className="h-7 shrink-0 gap-1.5 px-2 text-[12px]"
						onClick={() => void cancelProviderAccountSignIn(provider.id, accountId)}
					>
						<LoaderCircle className="size-3.5 animate-spin" aria-hidden />
						{t("settings.model.accountsSignInCancel")}
					</Button>
				) : (
					<Button
						type="button"
						size="sm"
						className="h-7 shrink-0 gap-1.5 px-2.5 text-[12px]"
						onClick={() => void signIn()}
					>
						<LogIn className="size-3.5" aria-hidden />
						{t("settings.model.accountsSignIn")}
					</Button>
				)}
			</div>
			{!statusQuery.isPending && !signedIn && !signingIn && loginCommand ? (
				<div className="flex min-w-0 items-center gap-1.5 px-3 pb-2.5">
					<span className="shrink-0 text-[11px] text-muted-foreground">
						{t("settings.model.accountsOrTerminal")}
					</span>
					<code className="min-w-0 flex-1 truncate rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
						{loginCommand}
					</code>
					<Button
						type="button"
						variant="ghost"
						size="icon"
						className="size-6 shrink-0"
						aria-label={t("settings.model.accountsCopyCommand")}
						onClick={async () => {
							try {
								await navigator.clipboard.writeText(loginCommand);
								toast.success(t("settings.model.accountsCommandCopied"));
							} catch {
								toast.error(t("settings.model.accountsCommandCopyFailed"));
							}
						}}
					>
						<Copy className="size-3" aria-hidden />
					</Button>
				</div>
			) : null}
			{open && showsUsage ? (
				<div className="border-t border-border/50 bg-muted/20 px-3 py-3">
					<ProviderUsageDetails provider={provider} runtime={runtime} />
				</div>
			) : null}
		</li>
	);
}

function NamedAccountRow({
	provider,
	baseRuntime,
	account,
	onChanged,
}: {
	provider: Provider;
	baseRuntime: ProviderRuntimeConfig | null;
	account: ProviderAccount;
	onChanged: () => Promise<void>;
}) {
	const { t } = useTranslation("common");
	const [editing, setEditing] = useState(false);
	const [label, setLabel] = useState(account.label);
	const [busy, setBusy] = useState(false);

	async function saveLabel() {
		if (label.trim() === account.label) {
			setEditing(false);
			return;
		}
		setBusy(true);
		try {
			await renameProviderAccount(account.id, label);
			await onChanged();
			setEditing(false);
		} catch (error) {
			toast.error(errorMessage(error, t("settings.model.accountsRenameFailed")));
		} finally {
			setBusy(false);
		}
	}

	async function remove() {
		if (!window.confirm(t("settings.model.accountsRemoveConfirm", { name: account.label }))) {
			return;
		}
		setBusy(true);
		try {
			await removeProviderAccount(account.id);
			await onChanged();
		} catch (error) {
			toast.error(errorMessage(error, t("settings.model.accountsRemoveFailed")));
			setBusy(false);
		}
	}

	const title = editing ? (
		<Input
			autoFocus
			value={label}
			disabled={busy}
			onChange={(event) => setLabel(event.target.value)}
			onBlur={() => void saveLabel()}
			onKeyDown={(event) => {
				if (event.key === "Enter") void saveLabel();
				if (event.key === "Escape") {
					setLabel(account.label);
					setEditing(false);
				}
			}}
			className="h-7 w-48 text-[13px]"
			aria-label={t("settings.model.accountsName")}
		/>
	) : (
		account.label
	);

	return (
		<AccountRow
			provider={provider}
			baseRuntime={baseRuntime}
			accountId={account.id}
			title={title}
			actions={
				editing ? null : (
					<>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							className="size-7"
							disabled={busy}
							onClick={() => setEditing(true)}
							aria-label={t("settings.model.accountsRename")}
						>
							<Pencil className="size-3.5" aria-hidden />
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							className="size-7"
							disabled={busy}
							onClick={() => void remove()}
							aria-label={t("settings.model.accountsRemove")}
						>
							<Trash2 className="size-3.5" aria-hidden />
						</Button>
					</>
				)
			}
		/>
	);
}

/** The provider's sign-ins: the system login first, then named accounts. */
export function ProviderAccountsList({
	provider,
	baseRuntime,
}: {
	provider: Provider;
	baseRuntime: ProviderRuntimeConfig | null;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [newLabel, setNewLabel] = useState("");
	const [adding, setAdding] = useState(false);
	const accountsQuery = useQuery({
		queryKey: PROVIDER_ACCOUNTS_QUERY_KEY,
		queryFn: listProviderAccounts,
		staleTime: 30_000,
	});
	const accounts = accountsOfProvider(accountsQuery.data ?? [], provider.id);

	async function refresh() {
		await queryClient.invalidateQueries({ queryKey: PROVIDER_ACCOUNTS_QUERY_KEY });
	}

	async function add() {
		if (!newLabel.trim()) return;
		setAdding(true);
		try {
			await createProviderAccount({ providerId: provider.id, label: newLabel });
			setNewLabel("");
			await refresh();
		} catch (error) {
			toast.error(errorMessage(error, t("settings.model.accountsAddFailed")));
		} finally {
			setAdding(false);
		}
	}

	return (
		<div className="space-y-2.5">
			<ul className="divide-y divide-border/50 overflow-hidden rounded-lg border border-border/60 bg-background">
				<AccountRow
					provider={provider}
					baseRuntime={baseRuntime}
					accountId={null}
					title={t("settings.model.accountsDefault")}
					badge={t("settings.model.accountsDefaultBadge")}
				/>
				{accounts.map((account) => (
					<NamedAccountRow
						key={account.id}
						provider={provider}
						baseRuntime={baseRuntime}
						account={account}
						onChanged={refresh}
					/>
				))}
			</ul>
			<form
				className="flex items-center gap-2"
				onSubmit={(event) => {
					event.preventDefault();
					void add();
				}}
			>
				<Input
					value={newLabel}
					disabled={adding}
					onChange={(event) => setNewLabel(event.target.value)}
					placeholder={t("settings.model.accountsNamePlaceholder")}
					className="h-8 max-w-xs text-[12px]"
					aria-label={t("settings.model.accountsName")}
				/>
				<Button
					type="submit"
					size="sm"
					variant="outline"
					className="h-8 shrink-0 gap-1.5 text-[12px]"
					disabled={adding || !newLabel.trim()}
				>
					{adding ? (
						<LoaderCircle className="size-3.5 animate-spin" aria-hidden />
					) : (
						<Plus className="size-3.5" aria-hidden />
					)}
					{t("settings.model.accountsAdd")}
				</Button>
			</form>
		</div>
	);
}
