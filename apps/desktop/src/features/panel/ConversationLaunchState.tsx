import { useTranslation } from "react-i18next";

type ConversationLaunchStateProps = {
	workspaceName: string;
};

/**
 * A task starts through its composer. Keep the empty viewport calm and avoid
 * exposing the runtime session that DCC will create on the first prompt.
 */
export function ConversationLaunchState({
	workspaceName,
}: ConversationLaunchStateProps) {
	const { t } = useTranslation("common");

	return (
		<div className="flex min-h-full flex-1 items-center justify-center px-6 py-10">
			<div className="max-w-2xl text-center">
				<h2 className="text-balance text-[26px] font-medium tracking-[-0.035em] text-foreground">
					{t("conversationLaunch.title", { workspaceName })}
				</h2>
				<p className="mx-auto mt-3 max-w-xl text-[13px] leading-6 text-muted-foreground">
					{t("conversationLaunch.hint")}
				</p>
			</div>
		</div>
	);
}
