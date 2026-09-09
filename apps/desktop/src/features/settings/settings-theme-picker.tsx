import { Check, Moon, SunMedium } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DccTheme } from "@/components/theme-provider";

export function SettingsThemePicker({
	theme,
	onChange,
}: {
	theme: DccTheme;
	onChange: (theme: DccTheme) => void;
}) {
	const { t } = useTranslation("common");
	return (
		<div className="dcc-settings-preference">
			<h3>{t("settings.appearance.theme")}</h3>
			<p className="dcc-settings-preference-hint">
				{t("settings.appearance.themeHint")}
			</p>
			<div
				className="dcc-settings-theme-options"
				role="group"
				aria-label={t("settings.appearance.theme")}
			>
				{(["light", "dark"] as const).map((value) => {
					const Icon = value === "light" ? SunMedium : Moon;
					return (
						<button
							type="button"
							key={value}
							className="dcc-settings-theme"
							aria-pressed={theme === value}
							onClick={() => onChange(value)}
						>
							<span
								className="dcc-settings-theme-preview"
								data-theme={value}
								aria-hidden
							>
								<span className="dcc-settings-preview-sidebar">
									<i />
									<i />
									<i />
								</span>
								<span className="dcc-settings-preview-conversation">
									<i />
									<i />
									<i />
									<span />
								</span>
							</span>
							<span className="dcc-settings-theme-label">
								<Icon size={15} aria-hidden />
								{t(`settings.appearance.${value}`)}
								<span className="dcc-settings-theme-check" aria-hidden>
									{theme === value && <Check size={12} />}
								</span>
							</span>
						</button>
					);
				})}
			</div>
		</div>
	);
}
