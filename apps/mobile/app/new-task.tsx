/**
 * Nova tarefa: project, request, agent (and model), worktree — the desktop's
 * launch screen on the phone. Creation runs on the desktop
 * (`POST /api/v1/mobile/tasks`, idempotent by `requestId`); the phone polls
 * its progress and opens the thread as soon as the session exists.
 */
import {
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
import { alpha, color, radius } from "@/theme";
import { AccessChoices } from "@/ui/AccessPicker";
import { ConnectionBanner } from "@/ui/ConnectionBanner";
import { AttachButton, AttachmentTray, DictationStatus, MicButton, useAttachments } from "@/ui/ComposerTools";
import { FileMentionList } from "@/ui/FileMentionList";
import { useDictation } from "@/dictation/useDictation";
import { Button, Loading, Mono, Panel } from "@/ui/primitives";
import { AgentMascot, ProjectGlyph } from "@/ui/sprites";
import Feather from "@expo/vector-icons/Feather";
import * as Crypto from "expo-crypto";
import { router, useLocalSearchParams } from "expo-router";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Platform, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
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
	const backend = useBackend();
	const insets = useSafeAreaInsets();
	const params = useLocalSearchParams<{ repositoryId?: string }>();
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

	return (
		<View style={styles.root}>
			{/* An iOS sheet already starts below the status bar; Android modals are full screen. */}
			<View style={[styles.header, { paddingTop: Platform.OS === "ios" ? 16 : insets.top + 8 }]}>
				<Text style={styles.title}>Nova tarefa</Text>
				<Pressable onPress={() => router.back()} hitSlop={10} accessibilityRole="button" disabled={creating}>
					<Text style={[styles.close, creating && { opacity: 0.4 }]}>Fechar</Text>
				</Pressable>
			</View>
			<KeyboardAwareScrollView
				style={{ flex: 1 }}
				contentContainerStyle={[styles.content, { paddingBottom: insets.bottom + 24 }]}
				keyboardShouldPersistTaps="handled"
				bottomOffset={96}
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
					<View style={{ gap: 22 }}>
						<Section label="Projeto">
							<View style={styles.chips}>
								{catalog.repositories.map((repo) => {
									const selected = repo.id === repositoryId;
									return (
										<Chip
											key={repo.id}
											selected={selected}
											disabled={creating}
											onPress={() => setRepositoryId(repo.id)}
											accessibilityLabel={`Projeto ${repositoryLabel(repo)}`}
										>
											<ProjectGlyph
												icon={repo.icon === "logo" ? repo.logo : repo.icon}
												color={repo.color}
												seed={repo.rootPath}
												size={22}
											/>
											<Text style={[styles.chipText, selected && styles.chipTextOn]} numberOfLines={1}>
												{repositoryLabel(repo)}
											</Text>
										</Chip>
									);
								})}
							</View>
						</Section>

						<Section label="Pedido">
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
								placeholder="O que o agente deve fazer?"
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
							{imagesUnsupported ? (
								<Text style={styles.hint}>{t("composer.attachments.imagesUnsupported")}</Text>
							) : null}
							<DictationStatus dictation={dictation} />
							<View style={styles.promptTools}>
								<AttachButton attachments={attachments} disabled={creating} />
								<MicButton dictation={dictation} disabled={creating} />
							</View>
							{mention && repositoryId ? (
								<FileMentionList
									backend={backend}
									scope={{ repositoryId }}
									query={mention.query}
									onPick={insertMention}
								/>
							) : (
								<Text style={styles.hint}>Digite @ para mencionar um arquivo do projeto.</Text>
							)}
						</Section>

						<Section label="Agente">
							<View style={styles.chips}>
								{catalog.providers.map((p) => {
									const reason = providerUnavailableReason(p);
									const selected = p.id === providerId;
									return (
										<Chip
											key={p.id}
											selected={selected}
											disabled={creating || reason !== null}
											onPress={() => chooseProvider(p)}
											accessibilityLabel={`Agente ${p.label}${reason ? `, ${reason}` : ""}`}
										>
											<AgentMascot provider={p.id} state="idle" size={22} />
											<Text style={[styles.chipText, selected && styles.chipTextOn]} numberOfLines={1}>
												{p.label}
											</Text>
										</Chip>
									);
								})}
							</View>
							{catalog.providers.some((p) => providerUnavailableReason(p)) ? (
								<Text style={styles.hint}>
									{catalog.providers
										.filter((p) => providerUnavailableReason(p))
										.map((p) => `${p.label}: ${providerUnavailableReason(p)}`)
										.join(" · ")}
								</Text>
							) : null}
						</Section>

						{provider && provider.models.length > 0 ? (
							<Section label="Modelo">
								<View style={styles.chips}>
									{provider.models.map((m) => {
										const selected = m.id === model;
										return (
											<Chip
												key={m.id}
												selected={selected}
												disabled={creating}
												onPress={() => setModel(m.id)}
												accessibilityLabel={`Modelo ${m.label}`}
											>
												<Text style={[styles.chipText, selected && styles.chipTextOn]} numberOfLines={1}>
													{m.label}
												</Text>
												{m.recommended ? <Mono style={styles.recommended}>rec.</Mono> : null}
											</Chip>
										);
									})}
								</View>
							</Section>
						) : null}

						{provider ? (
							<Section label="Acesso">
								<AccessChoices
									available={availableAccess}
									value={access}
									onChange={chooseAccess}
									providerLabel={provider.label}
									disabled={creating}
								/>
							</Section>
						) : null}

						<Pressable
							onPress={() => setWorktree((value) => !value)}
							disabled={creating}
							style={styles.worktree}
							accessibilityRole="checkbox"
							accessibilityState={{ checked: worktree, disabled: creating }}
							accessibilityLabel="Worktree"
						>
							<View style={[styles.box, worktree && styles.boxOn]}>
								{worktree ? <Feather name="check" size={13} color={color.accentInk} /> : null}
							</View>
							<Text style={styles.worktreeLabel}>Worktree</Text>
							<Text style={styles.worktreeHint} numberOfLines={2}>
								· {worktree ? "Isolado do checkout original" : "Usa o checkout atual"}
							</Text>
						</Pressable>

						<Pressable
							onPress={() => setAutoResume((value) => !value)}
							disabled={creating}
							style={styles.worktree}
							accessibilityRole="checkbox"
							accessibilityState={{ checked: autoResume === true, disabled: creating }}
							accessibilityLabel="Continuar sozinho se o limite acabar"
						>
							<View style={[styles.box, autoResume === true && styles.boxOn]}>
								{autoResume === true ? <Feather name="check" size={13} color={color.accentInk} /> : null}
							</View>
							<Text style={styles.worktreeLabel} numberOfLines={2}>
								Continuar sozinho se o limite acabar
							</Text>
						</Pressable>

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

						<Button
							label="Criar tarefa"
							busy={creating}
							disabled={!ready}
							onPress={() => void create()}
							accessibilityLabel="Criar tarefa"
						/>
						{creating ? <Text style={styles.progress}>{CREATING_TEXT[phase.step]}</Text> : null}
					</View>
				)}
			</KeyboardAwareScrollView>
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

function Section({ label, children }: { label: string; children: ReactNode }) {
	return (
		<View style={{ gap: 10 }}>
			<Mono style={styles.eyebrow}>{label}</Mono>
			{children}
		</View>
	);
}

function Chip({
	selected,
	disabled,
	onPress,
	accessibilityLabel,
	children,
}: {
	selected: boolean;
	disabled?: boolean;
	onPress: () => void;
	accessibilityLabel: string;
	children: ReactNode;
}) {
	return (
		<Pressable
			onPress={onPress}
			disabled={disabled}
			accessibilityRole="radio"
			accessibilityState={{ selected, disabled: !!disabled }}
			accessibilityLabel={accessibilityLabel}
			style={({ pressed }) => [
				styles.chip,
				selected && styles.chipOn,
				disabled && !selected && { opacity: 0.4 },
				pressed && { backgroundColor: color.elevated },
			]}
		>
			{children}
		</Pressable>
	);
}

const styles = StyleSheet.create({
	root: { flex: 1, backgroundColor: color.bg },
	header: {
		flexDirection: "row",
		alignItems: "center",
		justifyContent: "space-between",
		paddingHorizontal: 16,
		paddingBottom: 12,
		borderBottomWidth: 1,
		borderBottomColor: color.border,
	},
	title: { color: color.fg, fontSize: 17, fontWeight: "700" },
	close: { color: color.mute, fontSize: 14 },
	content: { paddingHorizontal: 16, paddingTop: 16 },
	body: { color: color.mute, fontSize: 13, lineHeight: 19 },
	error: { color: color.danger, fontSize: 12, lineHeight: 17 },
	eyebrow: { fontSize: 10, fontWeight: "600", letterSpacing: 1.8, textTransform: "uppercase", color: color.mute },
	chips: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
	chip: {
		flexDirection: "row",
		alignItems: "center",
		gap: 8,
		maxWidth: "100%",
		paddingLeft: 6,
		paddingRight: 12,
		paddingVertical: 6,
		minHeight: 38,
		borderRadius: radius.md,
		borderWidth: 1,
		borderColor: color.border,
		backgroundColor: color.panel,
	},
	chipOn: { borderColor: color.accent, backgroundColor: alpha(color.accent, 0.08) },
	chipText: { color: color.mute, fontSize: 13, fontWeight: "500", flexShrink: 1 },
	chipTextOn: { color: color.fg },
	recommended: { fontSize: 9, color: color.accent },
	hint: { color: color.faint, fontSize: 11, lineHeight: 16 },
	promptTools: { flexDirection: "row", gap: 8 },
	prompt: {
		minHeight: 120,
		maxHeight: 260,
		borderWidth: 1,
		borderColor: color.border,
		backgroundColor: color.panel,
		borderRadius: radius.md,
		color: color.fg,
		paddingHorizontal: 12,
		paddingTop: 12,
		paddingBottom: 12,
		fontSize: 15,
		lineHeight: 21,
	},
	worktree: { flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 4 },
	box: {
		width: 20,
		height: 20,
		borderRadius: 5,
		borderWidth: 1.5,
		borderColor: color.mute,
		alignItems: "center",
		justifyContent: "center",
	},
	boxOn: { backgroundColor: color.accent, borderColor: color.accent },
	worktreeLabel: { color: color.fg, fontSize: 14, fontWeight: "600" },
	worktreeHint: { color: color.mute, fontSize: 13, flexShrink: 1 },
	progress: { color: color.mute, fontSize: 12, textAlign: "center" },
});
