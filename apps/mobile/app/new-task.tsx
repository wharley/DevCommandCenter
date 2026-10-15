/**
 * Nova tarefa: the composer is the screen. The project's mascot over its
 * dither asks what to do; the request is typed below, with the agent and
 * model, access, worktree and auto-resume as chips that open their choices. Creation runs on the desktop
 * (`POST /api/v1/mobile/tasks`, idempotent by `requestId`); the phone polls
 * its progress and opens the thread as soon as the session exists.
 */
import {
	accessLabel,
	accessPolicies,
	applyFileMention,
	defaultModelId,
	defaultProviderId,
	findActiveFileMention,
	pickAccessPolicy,
	providerUnavailableReason,
	repositoryLabel,
	t,
	taskRequestBody,
	type AccessPolicy,
	type CatalogProvider,
	type MobileCatalog,
	type TaskProgress,
} from "@dcc/mobile-core";
import { loadProjectAccess, saveProjectAccess } from "@/backend/accessStore";
import { cachedCatalog, fetchCatalog } from "@/backend/catalogCache";
import { useBackend } from "@/backend/session";
import { errorMessage } from "@/backend/types";
import { makeStyles, useTheme } from "@/theme";
import { AccessList, confirmThen, PolicyIcon } from "@/ui/AccessPicker";
import { ConnectionBanner } from "@/ui/ConnectionBanner";
import { AttachButton, AttachmentTray, DictationStatus, MicButton, useAttachments } from "@/ui/ComposerTools";
import { FileMentionList } from "@/ui/FileMentionList";
import { useDictation } from "@/dictation/useDictation";
import { GlassButton, GlassPill } from "@/ui/glass";
import { haptic } from "@/ui/haptics";
import { PickerSheet } from "@/ui/PickerSheet";
import { Button, Loading, Mono, Panel } from "@/ui/primitives";
import { AgentMascot, ProjectGlyph, projectTint } from "@/ui/sprites";
import { Chip, DitherHero, GroupedList, Row } from "@/ui/surfaces";
import Feather from "@expo/vector-icons/Feather";
import * as Crypto from "expo-crypto";
import { router, useLocalSearchParams } from "expo-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActivityIndicator, Platform, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { KeyboardAwareScrollView } from "react-native-keyboard-controller";
import { useSafeAreaInsets } from "react-native-safe-area-context";

const POLL_MS = 800;
/** Worktree creation plus setup scripts can take a while on big repositories. */
const GIVE_UP_MS = 3 * 60_000;

/** The last agent picked on this phone, so the next task starts from it. */
let rememberedPick: { providerId: string; model: string | null } | null = null;

type Phase =
	/** `sessionId`: the session exists but its first turn failed — it can still be opened. */
	| { kind: "editing"; error?: string; sessionId?: string }
	| { kind: "creating"; step: "sending" | "workspace" | "agent" };

export default function NewTaskScreen() {
	const { color } = useTheme();
	const styles = useStyles();
	const backend = useBackend();
	const insets = useSafeAreaInsets();
	const params = useLocalSearchParams<{ repositoryId?: string }>();
	// An iOS sheet already starts below the status bar; Android modals are full screen.
	const topInset = Platform.OS === "ios" ? 8 : insets.top;
	const [catalog, setCatalog] = useState<MobileCatalog | null>(null);
	const [loadError, setLoadError] = useState<string | null>(null);
	const [repositoryId, setRepositoryId] = useState<string | null>(null);
	const [prompt, setPrompt] = useState("");
	const [providerId, setProviderId] = useState<string | null>(null);
	const [model, setModel] = useState<string | null>(null);
	const [worktree, setWorktree] = useState(true);
	/** Starts from the desktop's default; null until it is read. */
	const [autoResume, setAutoResume] = useState<boolean | null>(null);
	const [access, setAccess] = useState<AccessPolicy | null>(null);
	const [selection, setSelection] = useState({ start: 0, end: 0 });
	/** Set once after a mention is inserted, to move the cursor past it. */
	const [forcedSelection, setForcedSelection] = useState<{ start: number; end: number } | undefined>();
	const [promptFocused, setPromptFocused] = useState(false);
	const input = useRef<TextInput>(null);
	const [phase, setPhase] = useState<Phase>({ kind: "editing" });
	const [picker, setPicker] = useState<"project" | "agent" | "access" | null>(null);
	const alive = useRef(true);
	const initialized = useRef(false);
	/** Kept while the draft is unchanged, so a retry can't create the task twice. */
	const requestId = useRef<string | null>(null);

	const adopt = useCallback(
		(next: MobileCatalog) => {
			setCatalog(next);
			if (initialized.current) return;
			initialized.current = true;
			setRepositoryId(pickRepository(next, params.repositoryId));
			const id = defaultProviderId(next.providers, rememberedPick?.providerId);
			const picked = next.providers.find((p) => p.id === id);
			const remembered = rememberedPick?.providerId === id ? rememberedPick.model : null;
			setProviderId(id);
			setModel(remembered && picked?.models.some((m) => m.id === remembered) ? remembered : defaultModelId(picked));
		},
		[params.repositoryId],
	);

	const load = useCallback(async () => {
		setLoadError(null);
		const cached = cachedCatalog(backend);
		if (cached) adopt(cached);
		try {
			const next = await fetchCatalog(backend);
			if (alive.current) adopt(next);
		} catch (error) {
			if (alive.current && !cached) setLoadError(errorMessage(error, "Não consegui ler os projetos do computador."));
		}
	}, [backend, adopt]);

	useEffect(() => {
		alive.current = true;
		void load();
		return () => {
			alive.current = false;
		};
	}, [load]);

	useEffect(() => {
		backend
			.request<{ autoResume?: boolean }>("GET", "/api/v1/usage-limit/settings")
			.then((settings) => {
				if (alive.current) setAutoResume((current) => current ?? settings.autoResume === true);
			})
			.catch(() => {});
	}, [backend]);

	const attachments = useAttachments(backend);
	const dictation = useDictation(prompt, setPrompt);
	const attached = attachments.items.map((item) => item.path ?? item.id).join("|");
	useEffect(() => {
		requestId.current = null;
	}, [repositoryId, prompt, providerId, model, worktree, access, autoResume, attached]);

	const provider = useMemo(
		() => catalog?.providers.find((p) => p.id === providerId) ?? null,
		[catalog, providerId],
	);
	const repository = catalog?.repositories.find((r) => r.id === repositoryId) ?? null;
	const projectKey = repository?.projectId ?? null;
	const availableAccess = useMemo(() => accessPolicies(provider), [provider]);

	// The level last picked for this project on this phone, if this agent offers it.
	useEffect(() => {
		let cancelled = false;
		if (!projectKey) {
			setAccess(pickAccessPolicy(availableAccess));
			return;
		}
		void loadProjectAccess(projectKey).then((remembered) => {
			if (!cancelled) setAccess(pickAccessPolicy(availableAccess, remembered));
		});
		return () => {
			cancelled = true;
		};
	}, [projectKey, availableAccess]);

	const chooseAccess = (policy: AccessPolicy) => {
		setAccess(policy);
		if (projectKey) saveProjectAccess(projectKey, policy);
	};

	const mention = promptFocused && repositoryId ? findActiveFileMention(prompt, selection.end) : null;
	const insertMention = (path: string) => {
		if (!mention) return;
		const next = applyFileMention(prompt, mention, path);
		setPrompt(next.text);
		setForcedSelection({ start: next.cursor, end: next.cursor });
		setSelection({ start: next.cursor, end: next.cursor });
		// Tapping the list takes focus away; keep typing where the mention ended.
		input.current?.focus();
	};
	const creating = phase.kind === "creating";
	const uploading = attachments.items.some((item) => item.status === "uploading");
	const hasReadyAttachment = attachments.items.some((item) => item.status === "ready");
	const ready =
		!!repositoryId &&
		(!!prompt.trim() || hasReadyAttachment) &&
		!uploading &&
		!!provider &&
		providerUnavailableReason(provider) === null;
	const imagesUnsupported =
		provider?.capabilities?.vision === false && attachments.items.some((item) => item.kind === "image");

	const chooseProvider = (next: CatalogProvider) => {
		setProviderId(next.id);
		setModel(defaultModelId(next));
	};

	const create = async () => {
		if (!ready || creating || !repositoryId || !providerId) return;
		// Dictation never sends by itself; creating stops it with what was heard.
		if (dictation.listening) dictation.stop();
		const id = (requestId.current ??= Crypto.randomUUID());
		rememberedPick = { providerId, model };
		setPhase({ kind: "creating", step: "sending" });
		try {
			let progress = await backend.request<TaskProgress>(
				"POST",
				"/api/v1/mobile/tasks",
				taskRequestBody({
					requestId: id,
					repositoryId,
					prompt: attachments.queue.prompt(prompt),
					providerId,
					model,
					worktree,
					access,
					autoResume,
				}),
			);
			const deadline = Date.now() + GIVE_UP_MS;
			while (alive.current && progress.state === "creating") {
				setPhase({ kind: "creating", step: progress.workspaceId ? "agent" : "workspace" });
				if (Date.now() > deadline) {
					setPhase({
						kind: "editing",
						error: "O computador ainda está criando a tarefa. Ela aparece em Agentes quando o agente começar.",
					});
					return;
				}
				await new Promise((resolve) => setTimeout(resolve, POLL_MS));
				progress = await backend.request<TaskProgress>("GET", `/api/v1/mobile/tasks/${encodeURIComponent(id)}`);
			}
			if (!alive.current) return;
			if (progress.state === "started" && progress.sessionId) {
				router.replace({ pathname: "/thread/[id]", params: { id: progress.sessionId } });
				return;
			}
			requestId.current = null;
			setPhase({
				kind: "editing",
				error: progress.error ?? "O computador não conseguiu criar a tarefa.",
				sessionId: progress.sessionId,
			});
		} catch (error) {
			// The id stays: sending again resumes the same creation instead of starting another.
			if (alive.current) setPhase({ kind: "editing", error: errorMessage(error, "Falha ao criar a tarefa.") });
		}
	};

	const repoLabel = repository ? repositoryLabel(repository) : null;
	const modelLabel = provider?.models.find((m) => m.id === model)?.label ?? null;
	const tint = repository ? projectTint(repository.color, repository.rootPath) : color.accent;
	const providerProblems = catalog?.providers.filter((p) => providerUnavailableReason(p)) ?? [];
	const close = () => router.back();

	return (
		<View style={styles.root}>
			<DitherHero tint={tint} height={440} />
			<KeyboardAwareScrollView
				style={{ flex: 1 }}
				contentContainerStyle={[styles.content, { paddingTop: topInset + 64, paddingBottom: insets.bottom + 16 }]}
				keyboardShouldPersistTaps="handled"
				bottomOffset={24}
			>
				<ConnectionBanner backend={backend} />
				{!catalog ? (
					loadError ? (
						<Panel tone="danger">
							<Text style={styles.error}>{loadError}</Text>
							<Button label="Tentar de novo" variant="ghost" style={{ marginTop: 12 }} onPress={() => void load()} />
						</Panel>
					) : (
						<Loading />
					)
				) : catalog.repositories.length === 0 ? (
					<Panel>
						<Text style={styles.body}>Nenhum projeto no computador. Adicione um pelo app do DCC no computador.</Text>
					</Panel>
				) : (
					<>
						<View style={styles.hero}>
							{repository ? (
								<ProjectGlyph
									icon={repository.icon === "logo" ? repository.logo : repository.icon}
									color={repository.color}
									seed={repository.rootPath}
									size={96}
									active={creating}
								/>
							) : null}
							<View style={styles.heroText}>
								<Text style={styles.lead}>O que vamos fazer em</Text>
								<Text style={styles.heroTitle} numberOfLines={2} accessibilityRole="header">
									{repoLabel ?? "qual projeto"}?
								</Text>
								<View style={styles.where}>
									<Feather name={worktree ? "git-branch" : "folder"} size={12} color={color.mute} />
									<Mono style={styles.whereText}>{worktree ? "num worktree novo" : "no checkout atual"}</Mono>
								</View>
							</View>
						</View>

						<View style={styles.composer}>
							<TextInput
								ref={input}
								value={prompt}
								onChangeText={setPrompt}
								selection={forcedSelection}
								onSelectionChange={(event) => {
									setSelection(event.nativeEvent.selection);
									setForcedSelection(undefined);
								}}
								onFocus={() => setPromptFocused(true)}
								onBlur={() => setPromptFocused(false)}
								placeholder="Descreva a tarefa… use @ para citar um arquivo"
								placeholderTextColor={color.faint}
								style={styles.prompt}
								multiline
								textAlignVertical="top"
								// Requests carry identifiers and commands; autocorrect mangles them.
								autoCorrect={false}
								editable={!creating && !dictation.listening}
								accessibilityLabel="Pedido para o agente"
							/>
							<AttachmentTray attachments={attachments} />
							{imagesUnsupported ? <Text style={styles.hint}>{t("composer.attachments.imagesUnsupported")}</Text> : null}
							<DictationStatus dictation={dictation} />
							{mention && repositoryId ? (
								<FileMentionList backend={backend} scope={{ repositoryId }} query={mention.query} onPick={insertMention} />
							) : null}

							<View style={styles.chips}>
								<Chip
									label={provider ? `${provider.label}${modelLabel ? ` · ${modelLabel}` : ""}` : "Agente"}
									leading={provider ? <AgentMascot provider={provider.id} state="idle" size={20} /> : undefined}
									menu
									onPress={creating ? undefined : () => setPicker("agent")}
								/>
								{provider && availableAccess.length > 0 && access ? (
									<Chip
										label={accessLabel(access)}
										leading={<PolicyIcon policy={access} size={13} />}
										menu
										onPress={creating ? undefined : () => setPicker("access")}
									/>
								) : null}
								<Chip
									label={worktree ? "Novo worktree" : "Checkout atual"}
									icon={worktree ? "git-branch" : "folder"}
									selected={worktree}
									onPress={creating ? undefined : () => setWorktree((value) => !value)}
								/>
								<Chip
									label="Retomar após o limite"
									icon="rotate-cw"
									selected={autoResume === true}
									onPress={creating ? undefined : () => setAutoResume((value) => !value)}
								/>
							</View>
							{providerProblems.length > 0 ? (
								<Text style={styles.hint}>
									{providerProblems.map((p) => `${p.label}: ${providerUnavailableReason(p)}`).join(" · ")}
								</Text>
							) : null}

							<View style={styles.actions}>
								<AttachButton attachments={attachments} disabled={creating} />
								<MicButton dictation={dictation} disabled={creating} />
								<View style={{ flex: 1 }}>
									{creating ? <Text style={styles.progress}>{CREATING_TEXT[phase.step]}</Text> : null}
								</View>
								<Pressable
									onPress={() => {
										haptic("success");
										void create();
									}}
									disabled={!ready || creating}
									accessibilityRole="button"
									accessibilityLabel="Criar tarefa"
									accessibilityState={{ disabled: !ready || creating, busy: creating }}
									style={({ pressed }) => [styles.send, (!ready || creating) && { opacity: 0.4 }, pressed && { opacity: 0.8 }]}
								>
									{creating ? (
										<ActivityIndicator size="small" color={color.accentInk} />
									) : (
										<Feather name="arrow-up" size={20} color={color.accentInk} />
									)}
								</Pressable>
							</View>
						</View>

						{phase.kind === "editing" && phase.error ? (
							<Panel tone="danger">
								<Text style={styles.error}>{phase.error}</Text>
								{phase.sessionId ? (
									<Button
										label="Abrir a conversa"
										variant="ghost"
										style={{ marginTop: 12 }}
										onPress={() => router.replace({ pathname: "/thread/[id]", params: { id: phase.sessionId! } })}
									/>
								) : null}
							</Panel>
						) : null}
					</>
				)}
			</KeyboardAwareScrollView>

			<View style={[styles.topBar, { top: topInset + 8 }]}>
				<GlassButton icon="x" accessibilityLabel="Fechar" onPress={close} />
				{catalog && catalog.repositories.length > 0 ? (
					<GlassPill
						onPress={creating ? undefined : () => setPicker("project")}
						accessibilityLabel={`Projeto: ${repoLabel ?? "nenhum"}. Trocar`}
						style={styles.projectPill}
					>
						<Text style={styles.projectText} numberOfLines={1}>
							{repoLabel ?? "Projeto"}
						</Text>
						<Feather name="chevron-down" size={15} color={color.mute} />
					</GlassPill>
				) : null}
				<View style={{ width: 44 }} />
			</View>

			<PickerSheet visible={picker === "project"} title="Projeto" onClose={() => setPicker(null)}>
				<GroupedList inset={54}>
					{(catalog?.repositories ?? []).map((repo) => (
						<Row
							key={repo.id}
							onPress={() => {
								haptic("select");
								setRepositoryId(repo.id);
								setPicker(null);
							}}
							leading={
								<ProjectGlyph icon={repo.icon === "logo" ? repo.logo : repo.icon} color={repo.color} seed={repo.rootPath} size={30} />
							}
							title={repositoryLabel(repo)}
							trailing={repo.id === repositoryId ? <Feather name="check" size={18} color={color.accent} /> : null}
						/>
					))}
				</GroupedList>
			</PickerSheet>

			<PickerSheet visible={picker === "agent"} title="Agente e modelo" onClose={() => setPicker(null)}>
				{(catalog?.providers ?? []).map((p) => {
					const reason = providerUnavailableReason(p);
					const chosen = p.id === providerId;
					return (
						<GroupedList key={p.id} inset={54}>
							<Row
								onPress={
									reason
										? undefined
										: () => {
												haptic("select");
												chooseProvider(p);
												if (p.models.length <= 1) setPicker(null);
											}
								}
								leading={<AgentMascot provider={p.id} state="idle" size={30} />}
								title={p.label}
								detail={reason ?? (chosen ? undefined : `${p.models.length} ${p.models.length === 1 ? "modelo" : "modelos"}`)}
								trailing={chosen && p.models.length === 0 ? <Feather name="check" size={18} color={color.accent} /> : null}
							/>
							{chosen
								? p.models.map((m) => (
										<Row
											key={m.id}
											onPress={() => {
												haptic("select");
												setModel(m.id);
												setPicker(null);
											}}
											leading={<View style={{ width: 30 }} />}
											title={m.label}
											detail={m.recommended ? "recomendado" : undefined}
											trailing={m.id === model ? <Feather name="check" size={18} color={color.accent} /> : null}
										/>
									))
								: null}
						</GroupedList>
					);
				})}
			</PickerSheet>

			<PickerSheet visible={picker === "access"} title="Acesso" onClose={() => setPicker(null)}>
				<AccessList
					available={availableAccess}
					value={access}
					onPick={(policy) => {
						setPicker(null);
						// iOS drops an alert raised while a modal is still closing.
						setTimeout(() => confirmThen(policy, access, chooseAccess), policy === "full_access" ? 350 : 0);
					}}
				/>
			</PickerSheet>
		</View>
	);
}

const CREATING_TEXT = {
	sending: "Enviando ao computador…",
	workspace: "Criando o workspace…",
	agent: "Abrindo o agente…",
} as const;

/** The project of the most recently touched workspace, unless one was asked for. */
function pickRepository(catalog: MobileCatalog, wanted?: string): string | null {
	if (wanted && catalog.repositories.some((r) => r.id === wanted)) return wanted;
	const recent = [...catalog.workspaces]
		.filter((w) => w.state !== "archived")
		.sort((a, b) => (b.updatedAt ?? "").localeCompare(a.updatedAt ?? ""))[0];
	const byRecent = recent && catalog.repositories.find((r) => r.rootPath === recent.rootPath);
	return (byRecent ?? catalog.repositories[0])?.id ?? null;
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		content: { flexGrow: 1, paddingHorizontal: 12, gap: 16 },
		body: { color: color.mute, fontSize: 13, lineHeight: 19 },
		error: { color: color.danger, fontSize: 12, lineHeight: 17 },
		hint: { color: color.mute, fontSize: 12, lineHeight: 17, paddingHorizontal: 4 },
		hero: { flexGrow: 1, alignItems: "center", justifyContent: "center", gap: 18, paddingVertical: 24 },
		heroText: { alignItems: "center", gap: 6, paddingHorizontal: 16 },
		lead: { color: color.mute, fontSize: 15 },
		heroTitle: { color: color.fg, fontSize: 28, lineHeight: 33, fontWeight: "700", letterSpacing: -0.7, textAlign: "center" },
		where: { flexDirection: "row", alignItems: "center", gap: 6, marginTop: 4 },
		whereText: { color: color.mute, fontSize: 12 },
		composer: {
			borderRadius: 28,
			paddingHorizontal: 14,
			paddingTop: 14,
			paddingBottom: 10,
			gap: 12,
			backgroundColor: color.panel,
			borderWidth: StyleSheet.hairlineWidth,
			borderColor: color.border,
			shadowColor: color.shadow,
			shadowOpacity: 1,
			shadowRadius: 24,
			shadowOffset: { width: 0, height: 10 },
		},
		prompt: {
			minHeight: 92,
			maxHeight: 240,
			color: color.fg,
			fontSize: 16,
			lineHeight: 23,
			paddingHorizontal: 4,
			paddingTop: 0,
			paddingBottom: 0,
		},
		chips: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
		actions: {
			flexDirection: "row",
			alignItems: "center",
			gap: 6,
			paddingTop: 10,
			borderTopWidth: StyleSheet.hairlineWidth,
			borderTopColor: color.border,
		},
		progress: { color: color.mute, fontSize: 12, textAlign: "right", paddingRight: 6 },
		send: { width: 44, height: 44, borderRadius: 999, alignItems: "center", justifyContent: "center", backgroundColor: color.accent },
		topBar: { position: "absolute", left: 16, right: 16, flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
		projectPill: { paddingHorizontal: 16, maxWidth: 240 },
		projectText: { color: color.fg, fontSize: 15, fontWeight: "600", flexShrink: 1 },
	}),
);
