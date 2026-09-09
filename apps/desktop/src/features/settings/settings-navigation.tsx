import { useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	FolderGit2,
	Search,
	Settings2,
	X,
	type LucideIcon,
} from "lucide-react";

export type SettingsSectionId =
	| "general"
	| "appearance"
	| "model"
	| "integrations"
	| "connections"
	| "shortcuts"
	| "git"
	| "experimental"
	| "account";
const groups = ["workspace", "services", "advanced"] as const;
export type SettingsSectionMeta = {
	id: SettingsSectionId;
	group: (typeof groups)[number];
	label: string;
	description: string;
	keywords: string;
	icon: LucideIcon;
};
const normalize = (text: string) =>
	text
		.normalize("NFD")
		.replace(/[\u0300-\u036f]/g, "")
		.toLocaleLowerCase();

export function SettingsNavigation({
	sections,
	activeSection,
	onSelect,
	panelId,
	workspaceName,
	workspaceRoot,
}: {
	sections: SettingsSectionMeta[];
	activeSection: SettingsSectionId;
	onSelect: (id: SettingsSectionId) => void;
	panelId: string;
	workspaceName: string | null;
	workspaceRoot: string | null;
}) {
	const { t } = useTranslation("common");
	const [query, setQuery] = useState("");
	const searchRef = useRef<HTMLInputElement>(null);
	const selectId = useId();
	const terms = normalize(query).trim().split(/\s+/).filter(Boolean);
	const matches = sections.filter((section) =>
		terms.every((term) =>
			normalize(
				`${section.label} ${section.description} ${section.keywords}`,
			).includes(term),
		),
	);
	const ordered = groups.flatMap((group) =>
		matches.filter((section) => section.group === group),
	);
	return (
		<aside className="dcc-settings-sidebar">
			<div className="dcc-settings-brand">
				<span>
					<Settings2 size={17} aria-hidden />
				</span>
				<strong>{t("settings.title")}</strong>
			</div>
			<div className="dcc-settings-desktop-nav">
				<div className="dcc-settings-search">
					<Search size={14} aria-hidden />
					<input
						data-settings-search
					ref={searchRef}
						value={query}
						onChange={(event) => setQuery(event.target.value)}
						placeholder={t("settings.navigation.search")}
						aria-label={t("settings.navigation.search")}
						onKeyDown={(event) => {
							if (event.key === "Escape" && query) {
								event.preventDefault();
								event.stopPropagation();
								setQuery("");
							}
							if (event.key === "Enter" && ordered[0]) {
								event.preventDefault();
								onSelect(ordered[0].id);
								document.getElementById(`${panelId}-${ordered[0].id}`)?.focus();
							}
						}}
					/>
					{query && (
						<button
							type="button"
							aria-label={t("settings.navigation.clearSearch")}
							onClick={() => {
								setQuery("");
								searchRef.current?.focus();
							}}
						>
							<X size={13} aria-hidden />
						</button>
					)}
				</div>
				<nav aria-label={t("settings.navigation.label")}>
					{groups.map((group) => {
						const members = ordered.filter(
							(section) => section.group === group,
						);
						if (!members.length) return null;
						return (
							<div className="dcc-settings-nav-group" key={group}>
								<p>{t(`settings.navigation.groups.${group}`)}</p>
								{members.map(({ id, label, description, icon: Icon }) => (
									<button
										type="button"
										key={id}
										id={`${panelId}-${id}`}
										className="dcc-settings-section-link"
										aria-current={activeSection === id ? "page" : undefined}
										aria-controls={panelId}
										title={description}
										onClick={() => onSelect(id)}
									>
										<Icon size={16} strokeWidth={1.8} aria-hidden />
										<span>{label}</span>
										<i aria-hidden />
									</button>
								))}
							</div>
						);
					})}
					{query.trim() && (
						<p className="dcc-settings-search-result" role="status">
							{t("settings.navigation.results", { count: ordered.length })}
						</p>
					)}
					{!ordered.length && (
						<button
							type="button"
							className="dcc-settings-reset"
							onClick={() => {
								setQuery("");
								searchRef.current?.focus();
							}}
						>
							{t("settings.navigation.showAll")}
						</button>
					)}
				</nav>
			</div>
			<div className="dcc-settings-mobile-nav">
				<label htmlFor={selectId}>
					{t("settings.navigation.chooseSection")}
				</label>
				<select
					id={selectId}
					value={activeSection}
					onChange={(event) =>
						onSelect(event.target.value as SettingsSectionId)
					}
					aria-controls={panelId}
				>
					{groups.map((group) => (
						<optgroup
							key={group}
							label={t(`settings.navigation.groups.${group}`)}
						>
							{sections
								.filter((section) => section.group === group)
								.map((section) => (
									<option key={section.id} value={section.id}>
										{section.label}
									</option>
								))}
						</optgroup>
					))}
				</select>
			</div>
			<div className="dcc-settings-workspace">
				<FolderGit2 size={16} aria-hidden />
				<div>
					<span>{t("settings.currentWorkspace")}</span>
					<strong title={workspaceRoot ?? undefined}>
						{workspaceName || workspaceRoot || t("settings.git.noWorkspace")}
					</strong>
				</div>
			</div>
		</aside>
	);
}
