/**
 * Delivering a task from the phone, as a sheet over the thread: the changed
 * files (local or on the branch) with each one's diff and the whole file a
 * tap away, then commit (the desktop's
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
	diffBlocks,
	parsePatch,
	pipelineText,
	pullRequestStateText,
	splitPath,
	t,
	workspaceDiffStat,
	type DeliveryChange,
	type DeliveryForge,
	type DeliverySummary,
	type ExecutionDockAction,
	type WorkspacePatch,
} from "@dcc/mobile-core";
import { useBackend } from "@/backend/session";
import { ApiError, errorMessage } from "@/backend/types";
import { alpha, font, makeStyles, radius, useTheme } from "@/theme";
import { haptic } from "@/ui/haptics";
import { Button, Loading, Mono, Panel } from "@/ui/primitives";
import { HighlightedCode, languageOf } from "@/ui/RichText";
import { GroupedList, SheetHeader, SheetScroll } from "@/ui/surfaces";
import Feather from "@expo/vector-icons/Feather";
import { router, useLocalSearchParams } from "expo-router";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Alert, Linking, Pressable, ScrollView, StyleSheet, Switch, Text, TextInput, View } from "react-native";
import { KeyboardStickyView } from "react-native-keyboard-controller";
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
	const [chosenTab, setChosenTab] = useState<"local" | "branch" | null>(null);
	const [openPath, setOpenPath] = useState<string | null>(null);
	/** The footer floats over the list; the list ends above it. */
	const [footerHeight, setFooterHeight] = useState(140);
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
			haptic("success");
			setBehind(null);
			if (done) setNotice(done);
			return true;
		} catch (err) {
			const code = err instanceof ApiError ? (err.body as { error?: { code?: string; outcome?: { target?: string } } } | null)?.error : null;
			if (code?.code === "behind_remote") setBehind(code.outcome?.target ?? summary?.branch ?? "");
			haptic("warning");
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
			<SheetScroll header={<SheetHeader title="Mudanças" onClose={() => router.back()} />} contentStyle={styles.content}>
				{error ? <Text style={styles.error}>{error}</Text> : <Loading />}
			</SheetScroll>
		);
	}

	const local = changeTotals(summary.changes);
	const committed = changeTotals(summary.branchChanges);
	const all = workspaceDiffStat(summary);
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
	// Local first when there is anything to commit; the branch otherwise.
	const tab = chosenTab ?? (summary.changes.length > 0 || summary.branchChanges.length === 0 ? "local" : "branch");
	const shown = tab === "local" ? summary.changes : summary.branchChanges;
	// The footer holds the next step plus one git alternative; PR actions sit in the PR card.
	const primary = actions.find((a) => a.id === primaryId) ?? null;
	const isPrAction = (a: ExecutionDockAction) => a.id === "create-pr" || a.id === "create-draft-pr" || a.id === "open-pr";
	const footerAlt = actions.find((a) => a.id !== primaryId && !isPrAction(a) && !a.disabled) ?? null;
	const inCard = actions.filter((a) => a.id !== primaryId && a !== footerAlt);

	const press = (action: ExecutionDockAction) => {
		haptic("tap");
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
		<View style={styles.root}>
			<SheetScroll
				contentStyle={[styles.content, { paddingBottom: footerHeight }]}
				// Inputs in the list stop above the footer, which rides on the keyboard.
				bottomOffset={footerHeight - insets.bottom + 12}
				header={
					<SheetHeader
						title="Mudanças"
						onClose={() => router.back()}
						detail={
							<View style={styles.route}>
								<Mono style={styles.routeText} numberOfLines={1}>
									{summary.branch ?? "HEAD destacado"} → {summary.baseBranch}
								</Mono>
								{all.files > 0 ? (
									<Mono style={styles.routeText}>
										{" · "}
										<Text style={{ color: color.accent }}>+{all.additions}</Text> <Text style={{ color: color.danger }}>−{all.deletions}</Text>
									</Mono>
								) : null}
							</View>
						}
					/>
				}
			>
				{summary.aheadOfRemoteCount + summary.behindOfRemoteCount > 0 ? (
					<Mono style={styles.muted}>
						{summary.aheadOfRemoteCount} à frente · {summary.behindOfRemoteCount} atrás do remoto
					</Mono>
				) : null}
				{notice ? (
					<Pressable onPress={() => setNotice(null)}>
						<Panel>
							<Text style={styles.body}>{notice}</Text>
						</Panel>
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

				<View style={styles.segments} accessibilityRole="tablist">
					{(
						[
							["local", `Não commitado · ${local.files}`],
							["branch", `Na branch · ${committed.files}`],
						] as const
					).map(([key, label]) => (
						<Pressable
							key={key}
							onPress={() => {
								haptic("select");
								setChosenTab(key);
								setOpenPath(null);
							}}
							accessibilityRole="tab"
							accessibilityState={{ selected: tab === key }}
							style={[styles.segment, tab === key && styles.segmentOn]}
						>
							<Text style={[styles.segmentText, tab === key && styles.segmentTextOn]}>{label}</Text>
						</Pressable>
					))}
				</View>

				{shown.length === 0 ? (
					<Text style={styles.empty}>{tab === "local" ? "Nada por commitar." : `Nada além de ${summary.baseBranch}.`}</Text>
				) : (
					<GroupedList inset={44}>
						{shown.map((change) => (
							<FileRow
								key={change.path}
								change={change}
								workspaceId={workspaceId}
								local={tab === "local"}
								open={openPath === change.path}
								onToggle={() => setOpenPath((current) => (current === change.path ? null : change.path))}
							/>
						))}
					</GroupedList>
				)}
				{tab === "local" && summary.stagedCount > 0 && summary.changes.some((c) => !c.staged) ? (
					<Text style={styles.muted}>Só os arquivos já preparados (staged) entram no commit, como no computador.</Text>
				) : null}

				{tab === "local" && summary.changes.length > 0 && !request ? (
					<View style={{ gap: 8 }}>
						<TextInput
							value={body}
							onChangeText={setBody}
							placeholder="Detalhes do commit (opcional)"
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

				<View style={styles.card}>
					<View style={styles.cardHead}>
						<Feather name="git-pull-request" size={15} color={color.mute} />
						<Text style={[styles.strong, { flex: 1 }]}>Pull request</Text>
						<Mono style={styles.muted}>{forgeLoading ? "lendo…" : pr ? `#${pr.number} · ${pullRequestStateText(pr)}` : "nenhum"}</Mono>
					</View>
					{pr?.title ? <Text style={styles.body}>{pr.title}</Text> : null}
					{ci ? <Mono style={[styles.muted, /falhou/.test(ci) && { color: color.danger }]}>{ci}</Mono> : null}
					{pr?.mergeable === "CONFLICTING" ? <Mono style={[styles.muted, { color: color.danger }]}>Em conflito com a base</Mono> : null}
					{request ? (
						<View style={{ gap: 8, marginTop: 4 }}>
							<TextInput value={prTitle} onChangeText={setPrTitle} placeholder={t("composer.executionDock.createRequest.titleLabel")} placeholderTextColor={color.faint} style={styles.input} accessibilityLabel="Título do PR" />
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
						</View>
					) : inCard.length > 0 ? (
						<View style={[styles.actions, { flexWrap: "wrap", marginTop: 4 }]}>
							{inCard.map((action) => (
								<Pressable
									key={action.id}
									onPress={() => press(action)}
									disabled={action.disabled || busy !== null || (action.id === "open-pr" && !pr?.url)}
									accessibilityRole="button"
									style={({ pressed }) => [
										styles.cardAction,
										(action.disabled || busy !== null) && { opacity: 0.45 },
										pressed && { opacity: 0.7 },
									]}
								>
									{busy === action.id ? <ActivityIndicator size="small" color={color.fg} /> : null}
									<Text style={styles.cardActionText}>{deliveryActionLabel(action)}</Text>
								</Pressable>
							))}
						</View>
					) : null}
				</View>

				{desktopOnly ? <Text style={styles.muted}>{desktopOnly}</Text> : null}

				<View style={styles.taskRow}>
					<Button
						label="Concluir tarefa"
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
							confirm(
								"Arquivar a tarefa?",
								"O workspace fica arquivado no computador. Mudanças não enviadas continuam na pasta.",
								"Arquivar",
								() => void run("archive", "archive").then((ok) => ok && router.dismissTo("/")),
								true,
							)
						}
					/>
				</View>
			</SheetScroll>

			{/* Rides on the keyboard, so the commit message stays in sight while editing;
			    with the keyboard up it already covers the home indicator. */}
			<KeyboardStickyView
				offset={{ opened: insets.bottom }}
				style={[styles.footer, { paddingBottom: insets.bottom + 10 }]}
				onLayout={(e) => setFooterHeight(e.nativeEvent.layout.height)}
			>
				{request ? (
					<View style={styles.actions}>
						<Button label="Cancelar" variant="ghost" style={{ flex: 1 }} disabled={busy !== null} onPress={() => setRequest(null)} />
						<Button
							label={request.draft ? t("composer.executionDock.actions.createDraft", { requestLabel: "PR" }) : t("commit.modes.create-pr.idle", { requestLabel: "PR" })}
							style={{ flex: 1.4 }}
							busy={busy === "request"}
							disabled={busy !== null}
							onPress={() => void submitRequest()}
						/>
					</View>
				) : (
					<>
						{summary.changes.length > 0 ? (
							<TextInput
								value={message}
								onChangeText={setMessage}
								style={styles.input}
								autoCorrect={false}
								autoCapitalize="none"
								placeholder="Mensagem do commit"
								placeholderTextColor={color.faint}
								accessibilityLabel="Mensagem do commit"
							/>
						) : null}
						<View style={styles.actions}>
							{footerAlt ? (
								<Button
									label={deliveryActionLabel(footerAlt)}
									variant="ghost"
									style={{ flex: 1 }}
									busy={busy === footerAlt.id}
									disabled={busy !== null}
									onPress={() => press(footerAlt)}
								/>
							) : null}
							{primary ? (
								<Button
									label={deliveryActionLabel(primary)}
									style={{ flex: 1.4 }}
									busy={busy === primary.id}
									disabled={primary.disabled || busy !== null || (primary.id === "open-pr" && !pr?.url)}
									onPress={() => press(primary)}
								/>
							) : null}
						</View>
					</>
				)}
			</KeyboardStickyView>
		</View>
	);
}

/** One changed file: name over its folder, +/− with blocks; opens to its diff and the file. */
function FileRow({
	change,
	workspaceId,
	local,
	open,
	onToggle,
}: {
	change: DeliveryChange;
	workspaceId: string;
	/** Local changes have a patch against HEAD; committed ones open as the file. */
	local: boolean;
	open: boolean;
	onToggle: () => void;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	const { name, dir } = splitPath(change.path);
	const added = change.status === "??" || change.status === "?" || change.status === "A";
	const deleted = change.status === "D";
	const blocks = diffBlocks(change.insertions, change.deletions);
	return (
		<View>
			<Pressable
				onPress={() => {
					haptic("select");
					onToggle();
				}}
				accessibilityRole="button"
				accessibilityState={{ expanded: open }}
				accessibilityLabel={`${change.path}, ${change.insertions} linhas a mais, ${change.deletions} a menos`}
				style={({ pressed }) => [styles.fileRow, pressed && { backgroundColor: color.elevated }]}
			>
				<Feather
					name={added ? "file-plus" : deleted ? "file-minus" : "file-text"}
					size={17}
					color={added ? color.accent : deleted ? color.danger : color.mute}
				/>
				<View style={{ flex: 1, minWidth: 0, gap: 1 }}>
					<Text style={styles.fileName} numberOfLines={1}>
						{name}
					</Text>
					<Mono style={styles.fileDir} numberOfLines={1}>
						{[dir.replace(/\/$/, ""), added ? "novo" : deleted ? "removido" : null].filter(Boolean).join(" · ") || "raiz"}
					</Mono>
				</View>
				<View style={styles.stat}>
					<Mono style={styles.statText}>
						<Text style={{ color: color.accent }}>+{change.insertions}</Text> <Text style={{ color: color.danger }}>−{change.deletions}</Text>
					</Mono>
					<View style={styles.blocks}>
						{blocks.map((kind, i) => (
							<View
								key={i}
								style={[styles.block, { backgroundColor: kind === "add" ? color.accent : kind === "del" ? color.danger : color.border }]}
							/>
						))}
					</View>
				</View>
			</Pressable>
			{open ? (
				<View style={styles.expanded}>
					{local ? <PatchView workspaceId={workspaceId} path={change.path} /> : <Text style={styles.muted}>Mudança já commitada na branch.</Text>}
					{!deleted ? (
						<Pressable
							onPress={() => router.push({ pathname: "/file", params: { workspace: workspaceId, path: change.path } })}
							accessibilityRole="button"
							style={({ pressed }) => [styles.openFile, pressed && { opacity: 0.7 }]}
						>
							<Feather name="maximize-2" size={13} color={color.fg} />
							<Text style={styles.openFileText}>Ver o arquivo inteiro</Text>
						</Pressable>
					) : null}
				</View>
			) : null}
		</View>
	);
}

/** The file's patch against HEAD, line by line with old/new numbers. */
function PatchView({ workspaceId, path }: { workspaceId: string; path: string }) {
	const backend = useBackend();
	const { color } = useTheme();
	const styles = useStyles();
	const [patch, setPatch] = useState<WorkspacePatch | null>(null);
	const [error, setError] = useState<string | null>(null);
	useEffect(() => {
		let cancelled = false;
		backend
			.request<WorkspacePatch>(
				"GET",
				`/api/v1/mobile/workspaces/${encodeURIComponent(workspaceId)}/patch?path=${encodeURIComponent(path)}`,
			)
			.then((next) => {
				if (!cancelled) setPatch(next);
			})
			.catch((err) => {
				if (!cancelled) setError(errorMessage(err, "Não deu para ler o diff."));
			});
		return () => {
			cancelled = true;
		};
	}, [backend, workspaceId, path]);
	const lines = useMemo(() => (patch ? parsePatch(patch.patch).slice(0, MAX_PATCH_LINES) : []), [patch]);
	if (error) return <Text style={styles.error}>{error}</Text>;
	if (!patch) return <ActivityIndicator color={color.mute} style={{ alignSelf: "flex-start" }} />;
	if (lines.length === 0) return <Text style={styles.muted}>Sem diferença em texto.</Text>;
	const lang = languageOf(path);
	return (
		<View style={styles.patch}>
			<ScrollView horizontal showsHorizontalScrollIndicator={false}>
				<View style={{ minWidth: "100%" }}>
					{lines.map((line, i) =>
						line.kind === "hunk" || line.kind === "note" ? (
							<Text key={i} style={styles.hunk}>
								{line.text}
							</Text>
						) : (
							<View
								key={i}
								style={[
									styles.patchLine,
									line.kind === "add" && { backgroundColor: alpha(color.accent, 0.12) },
									line.kind === "del" && { backgroundColor: alpha(color.danger, 0.12) },
								]}
							>
								<Text style={styles.lineNo}>{line.oldLine ?? ""}</Text>
								<Text style={styles.lineNo}>{line.newLine ?? ""}</Text>
								<Text style={[styles.sign, { color: line.kind === "add" ? color.accent : line.kind === "del" ? color.danger : color.faint }]}>
									{line.kind === "add" ? "+" : line.kind === "del" ? "−" : " "}
								</Text>
								<Text style={styles.patchCode}>
									<HighlightedCode code={line.text || " "} lang={lang} />
								</Text>
							</View>
						),
					)}
				</View>
			</ScrollView>
			{patch.truncated || lines.length === MAX_PATCH_LINES ? <Text style={[styles.muted, { padding: 10 }]}>O diff continua — veja o arquivo inteiro.</Text> : null}
		</View>
	);
}

/** Long patches stay readable on a phone; the full file is a tap away. */
const MAX_PATCH_LINES = 400;

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
		content: { paddingHorizontal: 16, gap: 14 },
		route: { flexDirection: "row", alignItems: "center", flexWrap: "wrap" },
		routeText: { color: color.mute, fontSize: 12 },
		strong: { color: color.fg, fontSize: 15, fontWeight: "600" },
		body: { color: color.fg, fontSize: 14, lineHeight: 20 },
		muted: { color: color.mute, fontSize: 12, lineHeight: 17 },
		empty: { color: color.mute, fontSize: 14, textAlign: "center", paddingVertical: 20 },
		error: { color: color.danger, fontSize: 13 },
		segments: { flexDirection: "row", gap: 2, padding: 3, borderRadius: 12, backgroundColor: color.chip },
		segment: { flex: 1, height: 34, borderRadius: 9, alignItems: "center", justifyContent: "center" },
		segmentOn: { backgroundColor: color.panel, shadowColor: color.shadow, shadowOpacity: 1, shadowRadius: 3, shadowOffset: { width: 0, height: 1 } },
		segmentText: { color: color.mute, fontSize: 13, fontWeight: "500" },
		segmentTextOn: { color: color.fg, fontWeight: "600" },
		fileRow: { flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 14, paddingVertical: 11, minHeight: 56 },
		fileName: { color: color.fg, fontSize: 15, fontWeight: "500" },
		fileDir: { color: color.mute, fontSize: 11 },
		stat: { alignItems: "flex-end", gap: 5 },
		statText: { fontSize: 12, color: color.mute },
		blocks: { flexDirection: "row", gap: 2 },
		block: { width: 7, height: 7, borderRadius: 2 },
		expanded: { paddingHorizontal: 14, paddingBottom: 12, gap: 10 },
		patch: { borderRadius: radius.md, overflow: "hidden", backgroundColor: color.bg, borderWidth: StyleSheet.hairlineWidth, borderColor: color.border },
		hunk: { fontFamily: font.mono, fontSize: 11, color: color.info, backgroundColor: alpha(color.info, 0.08), paddingHorizontal: 10, paddingVertical: 4 },
		patchLine: { flexDirection: "row", minHeight: 20, paddingRight: 12 },
		lineNo: { width: 30, fontFamily: font.mono, fontSize: 10, lineHeight: 20, color: color.faint, textAlign: "right" },
		sign: { width: 18, fontFamily: font.mono, fontSize: 12, lineHeight: 20, textAlign: "center" },
		patchCode: { fontFamily: font.mono, fontSize: 12, lineHeight: 20, color: color.fg },
		openFile: { flexDirection: "row", alignItems: "center", gap: 6, alignSelf: "flex-start", height: 34, paddingHorizontal: 12, borderRadius: 999, backgroundColor: color.chip },
		openFileText: { color: color.fg, fontSize: 13, fontWeight: "500" },
		card: { gap: 8, padding: 14, borderRadius: radius.lg, backgroundColor: color.panel },
		cardHead: { flexDirection: "row", alignItems: "center", gap: 8 },
		taskRow: { flexDirection: "row", gap: 8 },
		cardAction: { flexDirection: "row", alignItems: "center", gap: 6, height: 36, paddingHorizontal: 14, borderRadius: 999, backgroundColor: color.chip },
		cardActionText: { color: color.fg, fontSize: 13, fontWeight: "600" },
		input: {
			borderWidth: StyleSheet.hairlineWidth,
			borderColor: color.border,
			backgroundColor: color.panel,
			borderRadius: radius.md,
			color: color.fg,
			paddingHorizontal: 14,
			paddingVertical: 12,
			fontSize: 15,
		},
		toggle: { flexDirection: "row", alignItems: "center", gap: 10 },
		footer: {
			position: "absolute",
			left: 0,
			right: 0,
			bottom: 0,
			gap: 8,
			paddingHorizontal: 16,
			paddingTop: 12,
			borderTopWidth: StyleSheet.hairlineWidth,
			borderTopColor: color.border,
			backgroundColor: color.bg,
		},
		actions: { flexDirection: "row", gap: 8 },
	}),
);
