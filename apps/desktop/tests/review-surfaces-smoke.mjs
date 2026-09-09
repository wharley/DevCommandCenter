import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
const { chromium } = await import(
	process.env.DCC_PLAYWRIGHT_MODULE || "playwright"
);
const browser = await chromium.launch({
	headless: true,
	...(process.env.DCC_CHROMIUM_EXECUTABLE
		? { executablePath: process.env.DCC_CHROMIUM_EXECUTABLE }
		: {}),
});
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
page.setDefaultTimeout(15000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
page.on("console", (message) => {
	if (message.type() === "error") errors.push(message.text());
});
const button = (name) => page.getByRole("button", { name, exact: true });
const output =
	process.env.DCC_SCREENSHOT_DIR || "/tmp/dcc-review-surfaces-smoke";
const shot = (name) => page.screenshot({ path: `${output}/${name}.png` });
const noOverflow = async () =>
	assert.equal(
		await page.evaluate(
			() => document.documentElement.scrollWidth <= innerWidth,
		),
		true,
		"no page-level horizontal overflow",
	);
try {
	await mkdir(output, { recursive: true });
	await page.goto(
		`${process.env.DCC_REVIEW_URL || "http://127.0.0.1:1432"}/tests/review-surfaces.html`,
	);
	await button("Workspace").waitFor();
	await page.locator(".dcc-review-diff-card").waitFor();
	await page.waitForTimeout(1200);
	await shot("inspector-workspace");
	await button("Branch").focus();
	await page.keyboard.press("Enter");
	assert.equal(await button("Branch").getAttribute("aria-pressed"), "true");
	await page.locator(".dcc-review-branches code").first().waitFor();
	assert.equal(
		await page
			.locator(".dcc-review-branches code")
			.first()
			.evaluate((el) => el.scrollWidth <= el.clientWidth),
		true,
	);
	await shot("inspector-branch");
	await button("Último turno").click();
	await page.locator(".dcc-review-diff-card").waitFor();
	await page.waitForTimeout(700);
	await shot("inspector-turn");
	await button("Workspace").click();
	await button("Mostrar como árvore").click();
	await button("Mostrar como lista").click();
	await button("Alternar vazio").click();
	await page.getByText("O workspace não possui mudanças locais.").waitFor();
	await button("Branch").click();
	await page
		.getByText("Este branch ainda não possui mudanças em relação à base.")
		.waitFor();
	await button("Último turno").click();
	assert.equal(await page.locator(".dcc-review-diff-card").count(), 0);
	await button("Alternar vazio").click();
	await button("Pull Requests").click();
	await page.locator(".dcc-pr-card").first().waitFor();
	await page.locator(".dcc-pr-summary-card").first().waitFor();
	assert.equal(await page.locator(".dcc-pr-card").count(), 3);
	await shot("pr-summary");
	await button("Revisando").click();
	assert.equal(await page.locator(".dcc-pr-card").count(), 1);
	await button("Todos").click();
	await page.locator(".dcc-pr-list input").fill("inexistente");
	assert.equal(await page.locator(".dcc-pr-card").count(), 0);
	await page.locator(".dcc-pr-list input").fill("");
	await button("Código").click();
	await page.locator(".dcc-pr-code-files button").waitFor();
	await page
		.getByText("+export const review = true;", { exact: true })
		.waitFor();
	await shot("pr-code");
	await noOverflow();
	await button("Tema").click();
	await shot("pr-code-other-theme");
	await page.setViewportSize({ width: 600, height: 900 });
	assert.equal(await page.locator(".dcc-pr-detail").isVisible(), false);
	await page.locator(".dcc-pr-card").first().click();
	await button("Voltar para pull requests").waitFor();
	assert.equal(
		await button("Voltar para pull requests").evaluate(
			(el) => document.activeElement === el,
		),
		true,
	);
	await noOverflow();
	await shot("pr-compact");
	await button("Código").click();
	const files = await page.locator(".dcc-pr-code-files").boundingBox();
	const diff = await page.locator(".dcc-pr-code-diff").boundingBox();
	assert(
		diff.y >= files.y + files.height - 1,
		"compact file navigation sits above the diff",
	);
	await noOverflow();
	await shot("pr-compact-code");
	await button("Voltar para pull requests").click();
	assert.equal(
		await page
			.locator('.dcc-pr-card[aria-current="true"]')
			.evaluate((el) => document.activeElement === el),
		true,
	);
	await page.setViewportSize({ width: 360, height: 780 });
	await page.emulateMedia({ reducedMotion: "reduce" });
	await page.locator(".dcc-pr-card").first().click();
	await noOverflow();
	await shot("pr-narrow");
	await button("Código").click();
	await noOverflow();
	await shot("pr-narrow-code");
	assert.equal(
		await page
			.locator(".dcc-pr-review-footer")
			.evaluate((el) => el.scrollWidth <= el.clientWidth),
		true,
	);
	await button("Inspector").click();
	await button("Branch").click();
	await noOverflow();
	await shot("inspector-narrow");
	assert(
		await button("Branch").evaluate(
			(el) => parseFloat(getComputedStyle(el).transitionDuration) <= 0.001,
		),
	);
	assert.deepEqual(errors, []);
	assert.equal(
		await page.evaluate(() =>
			window.reviewIpc.some((command) =>
				/stage_file|discard|merge$|comment$|submit_review|execute_guarded/.test(
					command,
				),
			),
		),
		false,
		"no mutation IPC",
	);
	console.log(
		"PASS: review scopes, diffs, empty states, branches, PR filters/search/code, keyboard/focus, themes, compact layout, reduced motion. Screenshots: " +
			output,
	);
} catch (error) {
	console.error(errors);
	console.error(await page.locator("body").innerText());
	await shot("failure");
	throw error;
} finally {
	await browser.close();
}
