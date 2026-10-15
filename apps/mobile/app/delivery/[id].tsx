/**
 * Delivering a task from the phone: the diff, commit (the desktop's
 * deterministic message, editable), push and "Atualizar e enviar", the PR
 * (draft or not, title and description editable like the desktop's modal),
 * PR/CI state, and completing or archiving the task. The next step and the
 * actions offered are the desktop's (`deliveryActions`); conflicts, merge
 * and base sync stay on the computer. Push and archive ask first.
 */
import {
	changeTotals,
	deliveryActionLabel,
	deliveryActions,
	pipelineText,
	pullRequestStateText,
	t,
	type DeliveryChange,
	type DeliveryForge,
	type DeliverySummary,
	type ExecutionDockAction,
} from "@dcc/mobile-core";
import { useBackend } from "@/backend/session";
import { ApiError, errorMessage } from "@/backend/types";
import { makeStyles, radius, useTheme } from "@/theme";
import { Button, Loading, Mono, Panel } from "@/ui/primitives";
import Feather from "@expo/vector-icons/Feather";
import { router, useLocalSearchParams } from "expo-router";
import { useCallback, useEffect, useState } from "react";
import { Alert, Linking, Platform, Pressable, ScrollView, StyleSheet, Switch, Text, TextInput, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

/** Pushes and PRs run hooks, checks and the forge on the computer. */
const SLOW = { timeoutMs: 180_000 };

type Busy = ExecutionDockAction["id"] | "sync" | "complete" | "archive" | "request" | null;

export default function DeliveryScreen() {
	const { color } = useTheme();
	const styles = useStyles();
	const { id } = useLocalSearchParams<{ id: string }>();
	const workspaceId = String(id);
	const backend = useBackend();
	const insets = useSafeAreaInsets();
	const [summary, setSummary] = useState<DeliverySummary | null>(null);
	const [forge, setForge] = useState<DeliveryForge | null>(null);
	const [forgeLoading, setForgeLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [notice, setNotice] = useState<string | null>(null);
	const [busy, setBusy] = useState<Busy>(null);
	const [message, setMessage] = useState("");
	const [body, setBody] = useState("");
	const [behind, setBehind] = useState<string | null>(null);
	const [request, setRequest] = useState<{ draft: boolean } | null>(null);
	const [prTitle, setPrTitle] = useState("");
	const [prBody, setPrBody] = useState("");
	const [includeLocal, setIncludeLocal] = useState(true);
	const base = `/api/v1/mobile/workspaces/${encodeURIComponent(workspaceId)}/delivery`;

	const load = useCallback(async () => {
		try {
			const next = await backend.request<DeliverySummary>("GET", base);
			setSummary(next);
			setMessage(next.commitMessage);
			setError(null);
		} catch (err) {
			setError(errorMessage(err, "Não deu para ler o git da tarefa."));
		}
		setForgeLoading(true);
		try {
			const next = await backend.request<DeliveryForge>("GET", `${base}/forge`, undefined, { timeoutMs: 60_000 });
			setForge(next);
			setPrTitle((current) => current || next.requestTitle || "");
		} catch {
			setForge(null);
		} finally {
			setForgeLoading(false);
		}
	}, [backend, base]);

	useEffect(() => {
		void load();
	}, [load]);

	const run = async (what: Busy, action: string, payload?: unknown, done?: string) => {
		setBusy(what);
		setNotice(null);
		try {
			await backend.request("POST", `${base}/${action}`, payload, SLOW);
			setBehind(null);
			if (done) setNotice(done);
			return true;
		} catch (err) {
			const code = err instanceof ApiError ? (err.body as { error?: { code?: string; outcome?: { target?: string } } } | null)?.error : null;
			if (code?.code === "behind_remote") setBehind(code.outcome?.target ?? summary?.branch ?? "");
			setNotice(errorMessage(err, "A ação git falhou."));
			return false;
		} finally {
			setBusy(null);
			void load();
		}
	};

	const confirm = (title: string, text: string, label: string, go: () => void, destructive = false) =>
		Alert.alert(title, text, [
			{ text: "Cancelar", style: "cancel" },
			{ text: label, style: destructive ? "destructive" : "default", onPress: go },
		]);

	if (!summary) {
		return (
			<Screen insets={insets}>
				{error ? <Text style={styles.error}>{error}</Text> : <Loading />}
			</Screen>
		);
	}

	const local = changeTotals(summary.changes);
	const committed = changeTotals(summary.branchChanges);
	const { actions, primaryId, desktopOnly } = deliveryActions(summary, forge, busy !== null || forgeLoading);
	const pr = forge?.pullRequest ?? null;
	const ci = pipelineText(forge?.pipeline ?? null);
	const target = `origin/${summary.branch ?? ""}`;
	const editedMessage = message.trim() && message.trim() !== summary.commitMessage ? message.trim() : undefined;
	const commitPayload = (push: boolean) => ({
		changesToken: summary.changesToken,
		push,
		...(editedMessage ? { message: editedMessage } : {}),
		...(body.trim() ? { body: body.trim() } : {}),
	});

	const press = (action: ExecutionDockAction) => {
		switch (action.id) {
			case "commit":
				return void run("commit", "commit", commitPayload(false), t("composer.executionDock.actions.committed"));
			case "commit-and-push":
				return confirm("Commitar e enviar?", `O commit vai para ${target}.`, "Enviar", () =>
					void run("commit-and-push", "commit", commitPayload(true), t("composer.executionDock.actions.committedAndPushed")),
				);
			case "push":
				return confirm("Enviar?", `${summary.aheadOfRemoteCount} commit(s) vão para ${target}.`, "Enviar", () =>
					void run("push", "push", undefined, t("composer.executionDock.actions.pushed")),
				);
			case "create-pr":
			case "create-draft-pr":
				return setRequest({ draft: action.id === "create-draft-pr" });
			case "open-pr":
				if (pr?.url) void Linking.openURL(pr.url);
				return;
		}
	};

	const submitRequest = async () => {
		if (!request) return;
		const ok = await run(
			"request",
			"request",
			{
				title: prTitle.trim() || null,
				body: prBody.trim() || null,
				draft: request.draft,
				includeLocalChanges: includeLocal && summary.changes.length > 0,
				changesToken: summary.changesToken,
			},
			t("composer.executionDock.createRequest.created", { requestLabel: "PR" }),
		);
		if (ok) setRequest(null);
	};

	return (
		<Screen insets={insets}>
			<View style={styles.route}>
				<Feather name="git-branch" size={13} color={color.mute} />
				<Mono style={styles.routeText} numberOfLines={1}>
					{t("composer.executionDock.createRequest.route", { head: summary.branch ?? "HEAD destacado", base: summary.baseBranch })}
				</Mono>
			</View>
			{summary.aheadOfRemoteCount + summary.behindOfRemoteCount > 0 ? (
				<Mono style={styles.muted}>
					{summary.aheadOfRemoteCount} à frente · {summary.behindOfRemoteCount} atrás do remoto
				</Mono>
			) : null}
			{notice ? (
				<Pressable onPress={() => setNotice(null)}>
					<Text style={styles.notice}>{notice}</Text>
				</Pressable>
			) : null}
			{error ? <Text style={styles.error}>{error}</Text> : null}

			{behind !== null || summary.pushFailure?.canSync ? (
				<Panel tone="wait" style={{ gap: 10 }}>
					<Text style={styles.strong}>
						{t("composer.executionDock.actions.pushBehindRemote", { target: behind || summary.pushFailure?.target || target })}
					</Text>
					<Text style={styles.body}>{t("composer.executionDock.actions.pushBehindRemoteHint")}</Text>
					<Button
						label={t("composer.executionDock.actions.syncAndPush")}
						variant="wait"
						busy={busy === "sync"}
						disabled={busy !== null}
						onPress={() =>
							confirm(t("composer.executionDock.actions.syncAndPush"), t("composer.executionDock.actions.pushBehindRemoteHint"), "Atualizar e enviar", () =>
								void run("sync", "sync-and-push", undefined, t("composer.executionDock.actions.syncAndPushSuccess", { target: behind || target })),
							)
						}
					/>
				</Panel>
			) : null}

			<Section title="Mudanças locais" detail={local.files > 0 ? t("composer.executionDock.createRequest.localSummary", { files: local.files, additions: local.additions, deletions: local.deletions }) : "nenhuma"}>
				<Files changes={summary.changes} />
				{summary.stagedCount > 0 && summary.changes.some((c) => !c.staged) ? (
					<Text style={styles.muted}>Só os arquivos já preparados (staged) entram no commit, como no computador.</Text>
				) : null}
				{summary.changes.length > 0 ? (
					<View style={{ gap: 8 }}>
						<Mono style={styles.label}>mensagem do commit</Mono>
						<TextInput
							value={message}
							onChangeText={setMessage}
							style={styles.input}
							autoCorrect={false}
							autoCapitalize="none"
							accessibilityLabel="Mensagem do commit"
						/>
						<TextInput
							value={body}
							onChangeText={setBody}
							placeholder="Detalhes (opcional)"
							placeholderTextColor={color.faint}
							style={[styles.input, { minHeight: 64 }]}
							multiline
							accessibilityLabel="Detalhes do commit"
						/>
						{summary.beforePush.length > 0 ? (
							<Text style={styles.muted}>Antes de enviar, o computador roda: {summary.beforePush.join(", ")}.</Text>
						) : null}
					</View>
				) : null}
			</Section>

			{summary.branchChanges.length > 0 ? (
				<Section title={`Na branch, além de ${summary.baseBranch}`} detail={t("composer.executionDock.createRequest.localSummary", { files: committed.files, additions: committed.additions, deletions: committed.deletions })}>
					<Files changes={summary.branchChanges} />
				</Section>
			) : null}

			<Section title="Pull request" detail={forgeLoading ? "lendo…" : pr ? `#${pr.number} · ${pullRequestStateText(pr)}` : "nenhum"}>
				{pr ? (
					<View style={{ gap: 4 }}>
						{pr.title ? <Text style={styles.strong}>{pr.title}</Text> : null}
						{ci ? <Mono style={[styles.muted, /falhou/.test(ci) && { color: color.danger }]}>{ci}</Mono> : null}
						{pr.mergeable === "CONFLICTING" ? <Mono style={[styles.muted, { color: color.danger }]}>Em conflito com a base</Mono> : null}
					</View>
				) : null}
				{request ? (
					<View style={{ gap: 8 }}>
						<Mono style={styles.label}>{t("composer.executionDock.createRequest.titleLabel").toLowerCase()}</Mono>
						<TextInput value={prTitle} onChangeText={setPrTitle} style={styles.input} accessibilityLabel="Título do PR" />
						<Mono style={styles.label}>{t("composer.executionDock.createRequest.descriptionLabel").toLowerCase()}</Mono>
						<TextInput
							value={prBody}
							onChangeText={setPrBody}
							placeholder={t("composer.executionDock.createRequest.descriptionPlaceholder")}
							placeholderTextColor={color.faint}
							style={[styles.input, { minHeight: 80 }]}
							multiline
							accessibilityLabel="Descrição do PR"
						/>
						<Text style={styles.muted}>Sem descrição, o forge preenche com os commits.</Text>
						<Toggle label={t("composer.executionDock.createRequest.draft")} value={request.draft} onChange={(draft) => setRequest({ draft })} />
						{summary.changes.length > 0 ? (
							<Toggle label={t("composer.executionDock.createRequest.includeLocalChanges")} value={includeLocal} onChange={setIncludeLocal} />
						) : null}
						<View style={styles.row}>
							<Button label="Cancelar" variant="ghost" style={{ flex: 1 }} disabled={busy !== null} onPress={() => setRequest(null)} />
							<Button
								label={request.draft ? t("composer.executionDock.actions.createDraft", { requestLabel: "PR" }) : t("commit.modes.create-pr.idle", { requestLabel: "PR" })}
								style={{ flex: 1 }}
								busy={busy === "request"}
								disabled={busy !== null}
								onPress={() => void submitRequest()}
							/>
						</View>
					</View>
				) : null}
			</Section>

			{desktopOnly ? <Text style={styles.muted}>{desktopOnly}</Text> : null}
			{!request ? (
				<View style={{ gap: 8 }}>
					{actions.map((action) => (
						<Button
							key={action.id}
							label={deliveryActionLabel(action)}
							variant={action.id === primaryId ? "primary" : "ghost"}
							busy={busy === action.id}
							disabled={action.disabled || busy !== null || (action.id === "open-pr" && !pr?.url)}
							onPress={() => press(action)}
						/>
					))}
				</View>
			) : null}

			<Section title="Tarefa">
				<View style={styles.row}>
					<Button
						label="Concluir"
						variant="ghost"
						style={{ flex: 1 }}
						busy={busy === "complete"}
						disabled={busy !== null}
						onPress={() =>
							confirm("Concluir a tarefa?", "Ela sai de \"Precisa de você\" e da lista de ativas, como no computador.", "Concluir", () =>
								void run("complete", "complete").then((ok) => ok && router.back()),
							)
						}
					/>
					<Button
						label="Arquivar"
						variant="danger"
						style={{ flex: 1 }}
						busy={busy === "archive"}
						disabled={busy !== null}
						onPress={() =>
							confirm("Arquivar a tarefa?", "O workspace fica arquivado no computador. Mudanças não enviadas continuam na pasta.", "Arquivar", () =>
								void run("archive", "archive").then((ok) => ok && router.dismissTo("/")),
							true)
						}
					/>
				</View>
			</Section>
		</Screen>
	);
}

function Screen({ insets, children }: { insets: { top: number; bottom: number }; children: React.ReactNode }) {
	const styles = useStyles();
	return (
		<ScrollView
			style={styles.root}
			keyboardShouldPersistTaps="handled"
			contentContainerStyle={[
				styles.content,
				{ paddingTop: Platform.OS === "ios" ? 20 : insets.top + 12, paddingBottom: insets.bottom + 32 },
			]}
		>
			<View style={styles.header}>
				<Text style={styles.title}>Entrega</Text>
				<Pressable onPress={() => router.back()} hitSlop={10} accessibilityRole="button">
					<Text style={styles.close}>Fechar</Text>
				</Pressable>
			</View>
			{children}
		</ScrollView>
	);
}

function Section({ title, detail, children }: { title: string; detail?: string; children?: React.ReactNode }) {
	const styles = useStyles();
	return (
		<Panel style={{ gap: 10 }}>
			<View style={styles.sectionHead}>
				<Text style={styles.strong}>{title}</Text>
				{detail ? <Mono style={styles.muted}>{detail}</Mono> : null}
			</View>
			{children}
		</Panel>
	);
}

function Files({ changes }: { changes: DeliveryChange[] }) {
	const styles = useStyles();
	const [all, setAll] = useState(false);
	const shown = all ? changes : changes.slice(0, 8);
	return (
		<View style={{ gap: 4 }}>
			{shown.map((change) => (
				<View key={change.path} style={styles.file}>
					<Mono style={styles.status}>{change.status === "??" || change.status === "?" ? "U" : change.status}</Mono>
					<Mono style={styles.path} numberOfLines={1}>
						{change.path}
					</Mono>
					<Mono style={styles.plus}>+{change.insertions}</Mono>
					<Mono style={styles.minus}>−{change.deletions}</Mono>
				</View>
			))}
			{changes.length > shown.length ? (
				<Pressable onPress={() => setAll(true)}>
					<Mono style={styles.more}>mais {changes.length - shown.length} arquivo(s)</Mono>
				</Pressable>
			) : null}
		</View>
	);
}

function Toggle({ label, value, onChange }: { label: string; value: boolean; onChange: (value: boolean) => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<View style={styles.toggle}>
			<Text style={[styles.body, { flex: 1 }]}>{label}</Text>
			<Switch value={value} onValueChange={onChange} trackColor={{ true: color.accent, false: color.border }} />
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		content: { paddingHorizontal: 16, gap: 12 },
		header: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", marginBottom: 4 },
		title: { color: color.fg, fontSize: 20, fontWeight: "600" },
		close: { color: color.mute, fontSize: 15 },
		route: { flexDirection: "row", alignItems: "center", gap: 6 },
		routeText: { color: color.fg, fontSize: 12, flexShrink: 1 },
		sectionHead: { flexDirection: "row", alignItems: "baseline", justifyContent: "space-between", gap: 8 },
		strong: { color: color.fg, fontSize: 14, fontWeight: "600" },
		body: { color: color.fg, fontSize: 13, lineHeight: 19 },
		muted: { color: color.mute, fontSize: 12, lineHeight: 17 },
		label: { color: color.faint, fontSize: 10, letterSpacing: 0.6 },
		notice: { color: color.mute, fontSize: 13, padding: 10, borderRadius: radius.sm, backgroundColor: color.panel, borderWidth: 1, borderColor: color.border },
		error: { color: color.danger, fontSize: 13 },
		input: {
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.bg,
			borderRadius: radius.md,
			color: color.fg,
			paddingHorizontal: 12,
			paddingVertical: 10,
			fontSize: 14,
		},
		file: { flexDirection: "row", alignItems: "center", gap: 8 },
		status: { width: 14, color: color.mute, fontSize: 11 },
		path: { flex: 1, color: color.fg, fontSize: 12 },
		plus: { color: color.accent, fontSize: 11 },
		minus: { color: color.danger, fontSize: 11 },
		more: { color: color.mute, fontSize: 11, marginTop: 4 },
		row: { flexDirection: "row", gap: 8 },
		toggle: { flexDirection: "row", alignItems: "center", gap: 10, paddingVertical: 2 },
	}),
);
