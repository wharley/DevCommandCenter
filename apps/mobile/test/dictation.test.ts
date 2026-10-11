import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DictationController, type DictationState, type SpeechModule, type SpeechPermission } from "../src/dictation/controller";

function fakeModule(overrides: Partial<SpeechModule> & { permission?: SpeechPermission; afterRequest?: SpeechPermission } = {}) {
	const calls: string[] = [];
	const starts: Array<Record<string, unknown>> = [];
	let permission = overrides.permission ?? { granted: false, canAskAgain: true };
	const module: SpeechModule = {
		isRecognitionAvailable: () => true,
		supportsOnDeviceRecognition: () => true,
		getSupportedLocales: async () => ({ installedLocales: ["pt-BR"] }),
		getPermissionsAsync: async () => permission,
		requestPermissionsAsync: async () => {
			calls.push("request");
			permission = overrides.afterRequest ?? { granted: true, canAskAgain: true };
			return permission;
		},
		start: (options) => {
			calls.push("start");
			starts.push(options);
		},
		stop: () => calls.push("stop"),
		abort: () => calls.push("abort"),
		...overrides,
	};
	return { module, calls, starts };
}

function setup(module: SpeechModule, initial = "", platform: "ios" | "android" = "ios") {
	let text = initial;
	const states: DictationState[] = [];
	const controller = new DictationController({
		module,
		platform,
		getText: () => text,
		setText: (next) => (text = next),
		onState: (state) => states.push(state),
	});
	return { controller, text: () => text, state: () => states.at(-1)!, states };
}

describe("dictation", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("asks permission on first use, shows partial text live and never erases what was typed", async () => {
		const { module, calls, starts } = fakeModule();
		const d = setup(module, "Corrija");
		await d.controller.toggle();
		expect(calls).toEqual(["request", "start"]);
		expect(starts[0]).toMatchObject({ lang: "pt-BR", interimResults: true, continuous: true, requiresOnDeviceRecognition: true });
		expect(d.state().phase).toBe("preparing");
		d.controller.handleStart();
		expect(d.state()).toMatchObject({ phase: "listening", onDevice: true });
		d.controller.handleResult("o login", false);
		expect(d.text()).toBe("Corrija o login");
		d.controller.handleResult("o login quebrado", false);
		expect(d.text()).toBe("Corrija o login quebrado");
		await d.controller.toggle();
		expect(d.state().phase).toBe("stopping");
		d.controller.handleResult("o login quebrado", true);
		d.controller.handleEnd();
		expect(d.state().phase).toBe("idle");
		expect(d.text()).toBe("Corrija o login quebrado");

		// Second time: already granted, no prompt.
		await d.controller.toggle();
		expect(calls.filter((c) => c === "request")).toHaveLength(1);
	});

	it("explains a refusal and points to the settings", async () => {
		const { module, calls } = fakeModule({ afterRequest: { granted: false, canAskAgain: false } });
		const d = setup(module);
		await d.controller.toggle();
		expect(calls).toEqual(["request"]);
		expect(d.state()).toMatchObject({ phase: "idle", needsSettings: true });
		expect(d.state().error).toMatch(/permissão/);
		// Denied for good: no second prompt.
		await d.controller.toggle();
		expect(calls).toEqual(["request"]);
	});

	it("falls back from on-device to the system recognizer once", async () => {
		const { module, starts } = fakeModule({ permission: { granted: true, canAskAgain: true } });
		const d = setup(module, "", "android");
		await d.controller.toggle();
		expect(starts[0]).toMatchObject({ requiresOnDeviceRecognition: true, androidRecognitionServicePackage: "com.google.android.as" });
		d.controller.handleError("language-not-supported");
		d.controller.handleEnd();
		expect(starts[1]).toMatchObject({ requiresOnDeviceRecognition: false });
		expect(d.state()).toMatchObject({ phase: "preparing", onDevice: false, error: null });
		d.controller.handleStart();
		d.controller.handleResult("olá", true);
		expect(d.text()).toBe("olá");
		d.controller.handleError("network");
		d.controller.handleEnd();
		expect(d.state().phase).toBe("idle");
		expect(d.state().error).toMatch(/internet/);
	});

	it("uses the network recognizer when pt-BR is not installed for offline use", async () => {
		const { module, starts } = fakeModule({
			permission: { granted: true, canAskAgain: true },
			getSupportedLocales: async () => ({ installedLocales: ["en-US"] }),
		});
		const d = setup(module, "", "android");
		await d.controller.toggle();
		expect(starts[0]).toMatchObject({ requiresOnDeviceRecognition: false });
		expect(starts[0]).not.toHaveProperty("androidRecognitionServicePackage");
	});

	it("ignores events from a session another composer started, and unsticks a stop with no end", async () => {
		const { module, calls } = fakeModule({ permission: { granted: true, canAskAgain: true } });
		const mine = setup(module, "a");
		const other = setup(module, "b");
		await mine.controller.toggle();
		mine.controller.handleStart();
		other.controller.handleStart();
		other.controller.handleResult("x", false);
		expect(other.text()).toBe("b");
		mine.controller.stop();
		await vi.advanceTimersByTimeAsync(3_100);
		expect(calls).toContain("abort");
		expect(mine.state().phase).toBe("idle");
	});
});
