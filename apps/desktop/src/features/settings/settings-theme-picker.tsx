import { Check, Monitor, Moon, SunMedium } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DccThemePreference } from "@/components/theme-provider";

const THEME_OPTIONS = [
	{ value: "light", Icon: SunMedium },
	{ value: "dark", Icon: Moon },
	{ value: "system", Icon: Monitor },
] as const;

function ThemePreviewBody() {
	return (
		<>
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
		</>
	);
}

export function SettingsThemePicker({
	theme,
	onChange,
}: {
	theme: DccThemePreference;
	onChange: (theme: DccThemePreference) => void;
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
				{THEME_OPTIONS.map(({ value, Icon }) => (
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
							<ThemePreviewBody />
							{value === "system" && (
								<span
									className="dcc-settings-theme-preview dcc-settings-theme-preview-half"
									data-theme="dark"
								>
									<ThemePreviewBody />
								</span>
							)}
						</span>
						<span className="dcc-settings-theme-label">
							<Icon size={15} aria-hidden />
							{t(`settings.appearance.${value}`)}
							<span className="dcc-settings-theme-check" aria-hidden>
								{theme === value && <Check size={12} />}
							</span>
						</span>
					</button>
				))}
			</div>
		</div>
	);
}
