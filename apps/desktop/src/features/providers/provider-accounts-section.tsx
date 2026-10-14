import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
	CheckCircle2,
	Copy,
	LoaderCircle,
	LogIn,
	Pencil,
	Plus,
	RefreshCcw,
	Trash2,
} from "lucide-react";
import { toast } from "sonner";
import type { ProviderAccount } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	cancelProviderAccountSignIn,
	createProviderAccount,
	getProviderAccountStatus,
	listProviderAccounts,
	removeProviderAccount,
	renameProviderAccount,
	signInProviderAccount,
} from "@/lib/provider-api";
import { accountsOfProvider, PROVIDER_ACCOUNTS_QUERY_KEY } from "./provider-accounts";

function errorMessage(error: unknown, fallback: string) {
	return error instanceof Error ? error.message : String(error || fallback);
}

function AccountStatusLine({
	providerId,
	accountId,
}: {
	providerId: string;
	accountId: string | null;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [signingIn, setSigningIn] = useState(false);
	const statusKey = [...PROVIDER_ACCOUNTS_QUERY_KEY, "status", providerId, accountId];
	const statusQuery = useQuery({
		queryKey: statusKey,
		queryFn: () => getProviderAccountStatus(providerId, accountId),
		staleTime: 60_000,
	});

	async function signIn() {
		setSigningIn(true);
		try {
			const { status } = await signInProviderAccount(providerId, accountId);
			await queryClient.invalidateQueries({ queryKey: statusKey });
			toast.success(
				status.email
					? t("settings.model.accountsSignInDoneAs", { email: status.email })
					: t("settings.model.accountsSignInDone"),
			);
		} catch (error) {
			const message = errorMessage(error, t("settings.model.accountsSignInFailed"));
			if (message !== "sign-in cancelled") toast.error(message);
		} finally {
			setSigningIn(false);
		}
	}

	if (statusQuery.isPending) {
		return (
			<p className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
				<LoaderCircle className="size-3 animate-spin" aria-hidden />
				{t("settings.model.accountsChecking")}
			</p>
		);
	}
	if (statusQuery.isError) {
		return (
			<p className="text-[11px] text-destructive">
				{errorMessage(statusQuery.error, t("settings.model.accountsCheckFailed"))}
			</p>
		);
	}

	const status = statusQuery.data?.status;
	if (status?.loggedIn) {
		const who = [status.email, status.plan].filter(Boolean).join(" · ");
		return (
			<p className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
				<CheckCircle2 className="size-3 text-emerald-500" aria-hidden />
				{who || t("settings.model.accountsSignedIn")}
			</p>
		);
	}

	const loginCommand = statusQuery.data?.loginCommand;
	return (
		<div className="space-y-1.5">
			<div className="flex min-w-0 items-center gap-2">
				<p className="min-w-0 flex-1 text-[11px] text-amber-600 dark:text-amber-400">
					{signingIn
						? t("settings.model.accountsSigningIn")
						: t("settings.model.accountsSignedOut")}
				</p>
				{signingIn ? (
					<Button
						type="button"
						variant="ghost"
						size="sm"
						className="h-7 shrink-0 gap-1.5 px-2 text-[11px]"
						onClick={() => void cancelProviderAccountSignIn(providerId, accountId)}
					>
						<LoaderCircle className="size-3 animate-spin" aria-hidden />
						{t("settings.model.accountsSignInCancel")}
					</Button>
				) : (
					<Button
						type="button"
						size="sm"
						className="h-7 shrink-0 gap-1.5 px-2 text-[11px]"
						onClick={() => void signIn()}
					>
						<LogIn className="size-3" aria-hidden />
						{t("settings.model.accountsSignIn")}
					</Button>
				)}
			</div>
			{loginCommand && !signingIn ? (
				<div className="flex min-w-0 items-center gap-1.5">
					<span className="shrink-0 text-[10px] text-muted-foreground">
						{t("settings.model.accountsOrTerminal")}
					</span>
					<code className="min-w-0 flex-1 truncate rounded-md border border-border/50 bg-muted/40 px-2 py-1 font-mono text-[11px]">
						{loginCommand}
					</code>
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="h-7 shrink-0 gap-1.5 px-2 text-[11px]"
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
						{t("settings.model.accountsCopyCommand")}
					</Button>
				</div>
			) : null}
		</div>
	);
}

function AccountRow({
	account,
	onChanged,
}: {
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

	return (
		<li className="space-y-1 rounded-lg border border-border/50 bg-background/60 px-3 py-2">
			<div className="flex min-w-0 items-center gap-2">
				{editing ? (
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
						className="h-7 text-[12px]"
						aria-label={t("settings.model.accountsName")}
					/>
				) : (
					<p className="min-w-0 flex-1 truncate text-[12px] font-medium">
						{account.label}
					</p>
				)}
				{editing ? null : (
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
				)}
			</div>
			<AccountStatusLine providerId={account.providerId} accountId={account.id} />
		</li>
	);
}

/** Named sign-ins of one provider; the system login is always listed first. */
export function ProviderAccountsSection({ providerId }: { providerId: string }) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [newLabel, setNewLabel] = useState("");
	const [adding, setAdding] = useState(false);
	const accountsQuery = useQuery({
		queryKey: PROVIDER_ACCOUNTS_QUERY_KEY,
		queryFn: listProviderAccounts,
		staleTime: 30_000,
	});
	const accounts = accountsOfProvider(accountsQuery.data ?? [], providerId);

	async function refresh() {
		await queryClient.invalidateQueries({ queryKey: PROVIDER_ACCOUNTS_QUERY_KEY });
	}

	async function add() {
		if (!newLabel.trim()) return;
		setAdding(true);
		try {
			await createProviderAccount({ providerId, label: newLabel });
			setNewLabel("");
			await refresh();
		} catch (error) {
			toast.error(errorMessage(error, t("settings.model.accountsAddFailed")));
		} finally {
			setAdding(false);
		}
	}

	return (
		<div className="space-y-2 rounded-xl border border-border/50 bg-background/60 p-3">
			<div className="flex items-start justify-between gap-2">
				<div className="min-w-0">
					<p className="text-[12px] font-medium">{t("settings.model.accountsTitle")}</p>
					<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
						{t("settings.model.accountsHint")}
					</p>
				</div>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="h-7 shrink-0 gap-1.5 px-2 text-[11px]"
					onClick={() => void refresh()}
				>
					<RefreshCcw className="size-3" aria-hidden />
					{t("settings.model.accountsRefresh")}
				</Button>
			</div>
			<ul className="space-y-1.5">
				<li className="space-y-1 rounded-lg border border-border/50 bg-background/60 px-3 py-2">
					<div className="flex items-center gap-2">
						<p className="text-[12px] font-medium">
							{t("settings.model.accountsDefault")}
						</p>
						<Badge variant="outline" className="h-5 px-1.5 text-[10px] font-normal">
							{t("settings.model.accountsDefaultBadge")}
						</Badge>
					</div>
					<AccountStatusLine providerId={providerId} accountId={null} />
				</li>
				{accounts.map((account) => (
					<AccountRow key={account.id} account={account} onChanged={refresh} />
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
					className="h-8 text-[12px]"
					aria-label={t("settings.model.accountsName")}
				/>
				<Button
					type="submit"
					size="sm"
					className="h-8 shrink-0 gap-1.5"
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
