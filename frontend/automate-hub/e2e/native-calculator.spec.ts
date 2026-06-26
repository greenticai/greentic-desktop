import { expect, test } from "./fixtures/greentic";

const calculatorPrompt =
  "open the calculator app. Take three inputs: two numbers and one operation (plus, minus, divide or multiply) and make the calculator do the operation and return the result";

type PlannerDraft = {
  draftId: string;
  runnerId: string;
  requiredAdapters: string[];
  inputs: string[];
  outputs: string[];
};

type RecordingTargets = {
  screenCapture: { status: string; help: string };
  targets: Array<{ id: string; available: boolean; unavailableReason?: string }>;
};

test.describe("@desktop-fake native calculator automation", () => {
  test.use({ testPermissions: "all_ready" });

  test("@desktop-fake creates and runs a native calculator runner with deterministic outputs", async ({
    page,
    guiUrl,
    expectNoRedErrors,
  }) => {
    await page.goto(appUrl(guiUrl, "/create", { mode: "prompt" }));
    await page.getByTestId("prompt-textarea").fill(calculatorPrompt);
    const draftResponse = page.waitForResponse(
      (response) =>
        response.url().includes("/api/v1/planner/drafts") &&
        response.request().method() === "POST",
    );
    await page.getByRole("button", { name: /Generate Draft Runner/i }).click();
    const draft = await expectData<PlannerDraft>(await draftResponse);

    expect(draft.inputs).toEqual(["number_1", "number_2", "operation"]);
    expect(draft.outputs).toEqual(["result"]);
    expect(draft.requiredAdapters.some((adapter) => /macos|linux|windows|vision/.test(adapter))).toBe(
      true,
    );

    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Save Runner/i }).click();
    await page.getByRole("link", { name: /View Runner/i }).click();

    const runner = page.getByTestId(`runner-${draft.runnerId}`);
    await expect(runner).toBeVisible();
    await runner.getByTestId(`runner-${draft.runnerId}-run`).click();
    await runner.getByTestId(`runner-${draft.runnerId}-input-number_1`).fill("1");
    await runner.getByTestId(`runner-${draft.runnerId}-input-number_2`).fill("1");
    await runner.getByTestId(`runner-${draft.runnerId}-input-operation`).fill("plus");
    await runner.getByTestId(`runner-${draft.runnerId}-run-confirm`).click();
    await expect(runner.getByText("result")).toBeVisible();
    await expect(runner.getByText("2")).toBeVisible();
    await expectNoRedErrors();
  });
});

test.describe("@desktop-fake native permission gating", () => {
  test.use({ testPermissions: "screen_capture_missing" });

  test("@desktop-fake desktop recording is unavailable when screen capture is missing", async ({
    page,
    api,
    guiUrl,
  }) => {
    const targets = await api.get<RecordingTargets>("/recording-targets");
    expect(targets.screenCapture.status).toBe("warning");
    expect(targets.targets.find((target) => target.id === "desktop")?.available).toBe(false);
    expect(targets.targets.find((target) => target.id === "remote")?.available).toBe(false);

    await page.goto(appUrl(guiUrl, "/create", { mode: "record" }));
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Desktop app task/i }).click();
    await expect(page.getByText("Screen recording permission is required.").last()).toBeVisible();
    await expect(page.getByText(targets.screenCapture.help)).toBeVisible();
  });
});

test.describe("@manual @desktop-real native calculator automation", () => {
  test("@manual @desktop-real opens the real platform calculator when explicitly enabled", async () => {
    test.skip(
      process.env.GREENTIC_DESKTOP_REAL_DESKTOP !== "1",
      "Set GREENTIC_DESKTOP_REAL_DESKTOP=1 and grant OS permissions to run real Calculator automation.",
    );
  });
});

function appUrl(guiUrl: string, pathname: string, params: Record<string, string> = {}) {
  const url = new URL(guiUrl);
  url.pathname = pathname;
  for (const [key, value] of Object.entries(params)) {
    url.searchParams.set(key, value);
  }
  return url.toString();
}

async function expectData<T>(response: { ok: () => boolean; json: () => Promise<unknown> }) {
  expect(response.ok()).toBe(true);
  const payload = (await response.json()) as { ok: boolean; data: T; error?: { message: string } };
  expect(payload.ok, payload.error?.message).toBe(true);
  return payload.data;
}
