import { useContext } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderCatalog } from "@dcc/contracts";
import { cn } from "@/lib/utils";
import {
	accountsOfProvider,
	ProviderAccountsContext,
	selectProviderAccount,
	shownAccountId,
	useProviderAccountSelection,
} from "./provider-accounts";

/**
 * Account choice under the model picker's provider tabs. Shown only when the
 * provider has accounts besides the system login. An open conversation keeps
 * its account, so the other choices only apply to new conversations.
 */
export function ProviderAccountStrip({
	provider,
	disabled = false,
}: {
	provider: ProviderCatalog["providers"][number];
	disabled?: boolean;
}) {
	const { t } = useTranslation("common");
	const { accounts, conversation } = useContext(ProviderAccountsContext);
	const selection = useProviderAccountSelection();
	const providerAccounts = accountsOfProvider(accounts, provider.id);
	if (!provider.capabilities.supportsAccounts || providerAccounts.length === 0) {
		return null;
	}
	const { accountId, locked } = shownAccountId(
		provider.id,
		conversation,
		selection,
		accounts,
	);
	const choices = [
		{ id: null, label: t("composer.picker.accountDefault") },
		...providerAccounts.map((account) => ({ id: account.id, label: account.label })),
	];

	return (
		<div
			role="radiogroup"
			aria-label={t("composer.picker.account")}
			className="flex h-9 shrink-0 items-center gap-1 overflow-x-auto border-b border-border/60 px-2"
			title={locked ? t("composer.picker.accountLocked") : undefined}
		>
			<span className="shrink-0 px-1 text-[11px] text-muted-foreground">
				{t("composer.picker.account")}
			</span>
			{choices.map((choice) => {
				const checked = choice.id === accountId;
				return (
					<button
						key={choice.id ?? "default"}
						type="button"
						role="radio"
						aria-checked={checked}
						disabled={disabled || (locked && !checked)}
						onClick={() => selectProviderAccount(provider.id, choice.id)}
						className={cn(
							"h-6 shrink-0 rounded-md px-2 text-[11px] transition-colors",
							checked
								? "bg-accent font-medium text-foreground"
								: "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
							"disabled:pointer-events-none disabled:opacity-40",
						)}
					>
						{choice.label}
					</button>
				);
			})}
		</div>
	);
}
