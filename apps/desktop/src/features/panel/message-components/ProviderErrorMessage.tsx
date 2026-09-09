import { Copy } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { providerErrorMessage } from "./provider-error-message";

export function ProviderErrorMessage({ reason }: { reason: string }) {
	const { t } = useTranslation("common");
	const message = providerErrorMessage(reason);
	return (
		<div className="mt-0.5 space-y-1.5 text-[11px] leading-4 text-muted-foreground">
			<p className="whitespace-pre-wrap [overflow-wrap:anywhere]">{message}</p>
			{message !== reason ? (
				<details>
					<summary className="cursor-pointer text-foreground">
						{t("conversation.message.errorDetails")}
					</summary>
					<pre className="mt-1 max-h-60 overflow-auto whitespace-pre-wrap rounded border border-border/60 p-2 text-[11px] [overflow-wrap:anywhere]">
						{reason}
					</pre>
				</details>
			) : null}
			<Button
				type="button"
				variant="ghost"
				size="sm"
				className="h-6 gap-1.5 px-1.5 text-[11px]"
				onClick={async () => {
					try {
						await navigator.clipboard.writeText(reason);
						toast.success(t("conversation.message.errorCopied"));
					} catch {
						toast.error(t("conversation.message.errorCopyFailed"));
					}
				}}
			>
				<Copy className="size-3" aria-hidden />
				{t("conversation.message.copyError")}
			</Button>
		</div>
	);
}
