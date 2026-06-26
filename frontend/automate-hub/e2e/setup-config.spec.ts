import { expect, test } from "./fixtures/greentic";
import type { APIResponse, Page } from "@playwright/test";

type SetupChecklist = {
  items: Array<{ id: string; label: string; ok: boolean; status: string; help: string }>;
};

type RecordingTargets = {
  screenCapture: { id: string; status: string; help: string };
  targets: Array<{ id: string; available: boolean; unavailableReason?: string }>;
};

type Extensions = {
  extensions: Array<{ id: string; installed?: boolean; enabled?: boolean; status?: string }>;
};

type LlmSettings = {
  provider: string;
  model: string;
  endpoint: string | null;
  requiresApiKey: boolean;
  hasApiKey: boolean;
  apiKey?: string;
  providers: Array<{
    id: string;
    label: string;
    defaultModel: string;
    endpoint: string | null;
    requiresApiKey: boolean;
  }>;
};

test.describe("@setup deterministic setup and recording permission state", () => {
  test.use({ testPermissions: "screen_capture_missing" });

  test("@setup setup actions are tokenized and recording matches checklist permission state", async ({
    page,
    api,
    guiUrl,
    expectNoRedErrors,
  }) => {
    const setup = await api.get<SetupChecklist>("/setup/checklist");
    const screen = item(setup, "screen_capture_permission");
    expect(screen.status).toBe("warning");
    expect(item(setup, "accessibility_permission").status).toBe("ready");
    expect(item(setup, "input_control_permission").status).toBe("ready");

    const targets = await api.get<RecordingTargets>("/recording-targets");
    expect(targets.screenCapture.status).toBe(screen.status);
    expect(targets.screenCapture.help).toBe(screen.help);
    expect(targets.targets.find((target) => target.id === "desktop")?.available).toBe(false);

    await expect(page.getByTestId("setup-screen_capture_permission")).toContainText(
      "Screen capture permission",
    );
    const fixResponse = page.waitForResponse(
      (response) =>
        response.url().includes("/api/v1/setup/fix") &&
        response.request().method() === "POST",
    );
    await page
      .getByTestId("setup-screen_capture_permission")
      .getByRole("button", { name: /set up/i })
      .click();
    await expectOk(await fixResponse);
    await expect(page.getByText(/Opened .*Recording|Automatic opening was not available/i)).toBeVisible();
    await expectNoRedErrors();

    await page.goto(appUrl(guiUrl, "/"));
    await page.getByRole("link", { name: /Start Recording/i }).click();
    await expect(page.getByRole("heading", { name: /Name your runner/i })).toBeVisible();
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Desktop app task/i }).click();
    await expect(page.getByText("Screen recording permission is required.").last()).toBeVisible();
    await expect(page.getByText(screen.help)).toBeVisible();
  });
});

test.describe("@setup all-ready permission state", () => {
  test.use({ testPermissions: "all_ready" });

  test("@setup setup checklist and recording target both report desktop capture ready", async ({
    page,
    api,
    guiUrl,
  }) => {
    const setup = await api.get<SetupChecklist>("/setup/checklist");
    expect(item(setup, "screen_capture_permission").status).toBe("ready");
    expect(item(setup, "accessibility_permission").status).toBe("ready");
    expect(item(setup, "input_control_permission").status).toBe("ready");

    const targets = await api.get<RecordingTargets>("/recording-targets");
    expect(targets.screenCapture.status).toBe("ready");
    expect(targets.targets.find((target) => target.id === "desktop")?.available).toBe(true);

    await page.goto(appUrl(guiUrl, "/create", { mode: "record" }));
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Desktop app task/i }).click();
    await expect(page.getByText("Screen recording permission is required.")).toHaveCount(0);
  });
});

test.describe("@extensions extension lifecycle", () => {
  test.use({ testPermissions: "all_ready" });

  test("@extensions searches, installs, refreshes, disables, enables, tests, and removes extensions", async ({
    page,
    api,
    guiUrl,
    expectNoRedErrors,
  }) => {
    await page.goto(appUrl(guiUrl, "/settings"));

    for (const query of ["playwright", "vision", "java", "terminal"]) {
      const result = await api.get<Extensions>(`/extensions/search?q=${encodeURIComponent(query)}`);
      expect(result.extensions.length, `${query} should return a store result`).toBeGreaterThan(0);
    }

    await page.getByPlaceholder("Search extensions").fill("playwright");
    const playwrightRow = page.getByTestId("extension-greentic.desktop.playwright");
    await expect(playwrightRow).toBeVisible();
    await clickApiAction(
      page,
      "/api/v1/extensions/install",
      () => playwrightRow.getByRole("button", { name: /install/i }).click(),
    );
    await expect(playwrightRow.getByText("Installed")).toBeVisible();
    await expect(playwrightRow.getByRole("button", { name: /disable/i })).toBeVisible();

    await clickApiAction(
      page,
      "/api/v1/extensions/greentic.desktop.playwright/disable",
      () => playwrightRow.getByRole("button", { name: /disable/i }).click(),
    );
    await expect(playwrightRow.getByRole("button", { name: /enable/i })).toBeVisible();

    await clickApiAction(
      page,
      "/api/v1/extensions/greentic.desktop.playwright/enable",
      () => playwrightRow.getByRole("button", { name: /enable/i }).click(),
    );
    await expect(playwrightRow.getByRole("button", { name: /disable/i })).toBeVisible();

    await clickApiAction(
      page,
      "/api/v1/extensions/greentic.desktop.playwright/health",
      () => playwrightRow.getByRole("button", { name: /test/i }).click(),
    );
    await expect(page.getByText(/greentic\.desktop\.playwright: healthy/i)).toBeVisible();

    const rejectedVision = await api.raw.post("/api/v1/extensions/install", {
      data: { source: "store://greentic.desktop.vision" },
    });
    expect(rejectedVision.status()).toBe(400);
    expect(((await rejectedVision.json()) as { error: { code: string } }).error.code).toBe(
      "extension.permission_approval_required",
    );

    await page.getByPlaceholder("Search extensions").fill("vision");
    const visionRow = page.getByTestId("extension-greentic.desktop.vision");
    await expect(visionRow).toBeVisible();
    page.once("dialog", async (dialog) => {
      expect(dialog.message()).toContain("Install Vision Screenshot Fallback Adapter");
      await dialog.accept();
    });
    await clickApiAction(
      page,
      "/api/v1/extensions/install",
      () => visionRow.getByRole("button", { name: /install/i }).click(),
    );
    await expect(visionRow.getByText("Installed")).toBeVisible();

    page.once("dialog", async (dialog) => {
      expect(dialog.message()).toContain("Remove Vision Screenshot Fallback Adapter");
      await dialog.accept();
    });
    await clickApiAction(
      page,
      "/api/v1/extensions/greentic.desktop.vision/remove",
      () => visionRow.getByRole("button", { name: /remove/i }).click(),
    );
    await expect(visionRow.getByRole("button", { name: /install/i })).toBeVisible();

    await expectNoRedErrors();
  });
});

test.describe("@config LLM settings", () => {
  test.use({ testPermissions: "all_ready" });

  test("@config lists greentic-llm providers and keeps API keys write-only", async ({
    page,
    api,
    guiUrl,
    expectNoRedErrors,
  }) => {
    await page.goto(appUrl(guiUrl, "/settings"));

    const settings = await api.get<LlmSettings>("/settings/llm");
    expect(settings.providers.map((provider) => provider.id)).toEqual([
      "local",
      "openai",
      "anthropic",
      "azure_openai",
      "google",
      "mistral",
      "deepseek",
      "openai_compatible",
      "nvidia_nim",
      "ollama",
    ]);

    await page.getByTestId("llm-provider-select").click();
    for (const label of [
      "Local heuristic",
      "OpenAI",
      "Anthropic",
      "Azure OpenAI",
      "Google Gemini",
      "Mistral",
      "DeepSeek",
      "OpenAI compatible",
      "NVIDIA NIM",
      "Ollama",
    ]) {
      await expect(page.getByRole("option", { name: label, exact: true })).toBeVisible();
    }
    await page.getByRole("option", { name: "DeepSeek", exact: true }).click();
    await expect(page.getByTestId("llm-model-input")).toHaveValue("deepseek-chat");
    await expect(page.getByTestId("llm-endpoint-input")).toHaveValue(
      "https://api.deepseek.com/v1",
    );

    await page.getByTestId("llm-api-key-input").fill("sk-e2e-secret-value");
    await clickApiAction(page, "/api/v1/settings/llm", () =>
      page.getByTestId("llm-save-button").click(),
      "PUT",
    );
    await expect(page.getByTestId("llm-api-key-input")).toHaveValue("");
    await expect(page.getByTestId("llm-api-key-status")).toHaveText("Saved");

    const saved = await api.get<LlmSettings>("/settings/llm");
    expect(saved.provider).toBe("deepseek");
    expect(saved.hasApiKey).toBe(true);
    expect(saved.apiKey).toBeUndefined();

    await clickApiAction(page, "/api/v1/settings/llm/test", () =>
      page.getByTestId("llm-test-button").click(),
    );
    await expect(page.getByText(/DeepSeek is configured with model deepseek-chat/i)).toBeVisible();

    await page.getByTestId("llm-clear-api-key").check();
    await clickApiAction(page, "/api/v1/settings/llm", () =>
      page.getByTestId("llm-save-button").click(),
      "PUT",
    );
    await expect(page.getByTestId("llm-api-key-status")).toHaveText("Not saved");
    expect((await api.get<LlmSettings>("/settings/llm")).hasApiKey).toBe(false);

    await clickApiAction(page, "/api/v1/settings/llm/test", () =>
      page.getByTestId("llm-test-button").click(),
    );
    await expect(page.getByText(/requires an API key/i)).toBeVisible();
    await expectNoRedErrors();
  });
});

function item(setup: SetupChecklist, id: string) {
  const found = setup.items.find((entry) => entry.id === id);
  expect(found, `${id} should be present in setup checklist`).toBeTruthy();
  return found!;
}

function appUrl(guiUrl: string, pathname: string, params: Record<string, string> = {}) {
  const url = new URL(guiUrl);
  url.pathname = pathname;
  for (const [key, value] of Object.entries(params)) {
    url.searchParams.set(key, value);
  }
  return url.toString();
}

async function clickApiAction(
  page: Page,
  path: string,
  action: () => Promise<unknown>,
  method = "POST",
) {
  const responsePromise = page.waitForResponse(
    (response) =>
      response.url().includes(path) && response.request().method() === method,
  );
  await action();
  await expectOk(await responsePromise);
}

async function expectOk(response: APIResponse) {
  expect(response.ok()).toBe(true);
  const payload = (await response.json()) as { ok: boolean; error?: { message: string } };
  expect(payload.ok, payload.error?.message).toBe(true);
}
