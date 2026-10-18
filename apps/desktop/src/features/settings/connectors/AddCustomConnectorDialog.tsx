import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { findPresetForUrl } from "./catalog";
import type { ConnectorTarget } from "./connector-model";

/** https (or loopback http), no credentials, a host with a dot. */
export function parseCustomConnectorUrl(value: string): URL | null {
	let url: URL;
	try {
		url = new URL(value.trim());
	} catch {
		return null;
	}
	const loopback = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
	if (
		!(url.protocol === "https:" || (url.protocol === "http:" && loopback)) ||
		url.username ||
		url.password ||
		(!loopback && !url.hostname.includes(".")) ||
		value.trim().length > 2048
	) {
		return null;
	}
	url.hash = "";
	return url;
}

const SECOND_LEVEL_SUFFIXES = new Set(["com", "co", "org", "net", "gov", "edu", "ac"]);

/** "mcp.example.com" → "Example", "app.zedy.com.br" → "Zedy"; an editable default. */
export function defaultConnectorName(url: URL): string {
	const parts = url.hostname.split(".").filter((part) => part !== "www" && part !== "mcp");
	let end = parts.length - 1;
	// Skip a country suffix such as ".com.br" or ".co.uk".
	if (end >= 2 && parts[end]!.length === 2 && SECOND_LEVEL_SUFFIXES.has(parts[end - 1]!)) {
		end -= 1;
	}
	const base = end >= 1 ? parts[end - 1]! : (parts[0] ?? url.hostname);
	return base.charAt(0).toUpperCase() + base.slice(1);
}

export function AddCustomConnectorDialog({
	open,
	onOpenChange,
	onSubmit,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onSubmit: (target: ConnectorTarget) => void;
}) {
	const { t } = useTranslation();
	const [url, setUrl] = useState("");
	const [name, setName] = useState("");
	const [nameEdited, setNameEdited] = useState(false);
	const [showError, setShowError] = useState(false);

	useEffect(() => {
		if (open) {
			setUrl("");
			setName("");
			setNameEdited(false);
			setShowError(false);
		}
	}, [open]);

	const parsed = parseCustomConnectorUrl(url);
	const submit = () => {
		if (!parsed) {
			setShowError(true);
			return;
		}
		const preset = findPresetForUrl(parsed.toString());
		onSubmit({
			name: preset?.name ?? (name.trim() || defaultConnectorName(parsed)),
			url: preset?.url ?? parsed.toString(),
			preset,
		});
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="w-[440px] max-w-[calc(100vw-4rem)] sm:max-w-[440px]">
				<div>
					<DialogTitle className="text-[15px] font-semibold">
						{t("settings.connectors.custom.title")}
					</DialogTitle>
					<DialogDescription className="mt-1 text-[12px] text-muted-foreground">
						{t("settings.connectors.custom.hint")}
					</DialogDescription>
				</div>
				<form
					className="space-y-3"
					onSubmit={(event) => {
						event.preventDefault();
						submit();
					}}
				>
					<div className="space-y-1.5">
						<Label htmlFor="custom-connector-url" className="text-[12px]">
							{t("settings.connectors.custom.urlLabel")}
						</Label>
						<Input
							id="custom-connector-url"
							autoFocus
							inputMode="url"
							spellCheck={false}
							placeholder="https://mcp.example.com/mcp"
							value={url}
							onChange={(event) => {
								setUrl(event.target.value);
								setShowError(false);
								const next = parseCustomConnectorUrl(event.target.value);
								if (next && !nameEdited) setName(defaultConnectorName(next));
							}}
							className="h-8 font-mono text-[12px]"
						/>
						{showError ? (
							<p className="text-[11px] text-destructive">
								{t("settings.connectors.custom.invalidUrl")}
							</p>
						) : null}
					</div>
					<div className="space-y-1.5">
						<Label htmlFor="custom-connector-name" className="text-[12px]">
							{t("settings.connectors.custom.nameLabel")}
						</Label>
						<Input
							id="custom-connector-name"
							value={name}
							onChange={(event) => {
								setName(event.target.value);
								setNameEdited(true);
							}}
							className="h-8 text-[12px]"
						/>
					</div>
					<div className="flex justify-end">
						<Button type="submit" size="sm" disabled={url.trim().length === 0}>
							<span className="text-[12px]">{t("settings.connectors.custom.continue")}</span>
						</Button>
					</div>
				</form>
			</DialogContent>
		</Dialog>
	);
}
