import { formatDistanceToNow } from "date-fns";
import { enUS, ptBR } from "date-fns/locale";
import { useTranslation } from "react-i18next";

export function formatMessageTimestamp(date: Date, language: string) {
	return formatDistanceToNow(date, {
		addSuffix: true,
		locale: language === "en" || language.startsWith("en-") ? enUS : ptBR,
	});
}

export function MessageTimestamp({ createdAt }: { createdAt?: string }) {
	const { i18n } = useTranslation("common");
	if (!createdAt) {
		return null;
	}

	const date = new Date(createdAt);
	if (Number.isNaN(date.getTime())) {
		return null;
	}

	return (
		<span className="inline-flex h-4 shrink-0 items-center text-[11px] leading-none tabular-nums text-muted-foreground">
			{formatMessageTimestamp(date, i18n.resolvedLanguage ?? i18n.language)}
		</span>
	);
}
