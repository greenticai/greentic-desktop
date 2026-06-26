import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures/greentic";

type RecordingSummary = {
  sessionId: string;
  state: string;
  captureState: string;
  captureBackend: string;
  captureBlockedReasons: string[];
  realEvents: number;
  screenshots: number;
};

type RecordingEvents = {
  events: Array<{ type: string; summary: string; value?: string }>;
};

type RecordingNormalise = {
  steps: string[];
  inputs: string[];
  outputs: string[];
  warnings: string[];
  yamlPreview: string;
};

type RecordingTestResult = {
  status: string;
  evidenceRef: string;
  outputs: Record<string, string>;
};

type RecordingFinalise = {
  runnerId: string;
  saved: boolean;
};

type EvidenceList = {
  bundles: Array<{ bundleId: string; runnerId: string; artifacts: Array<{ id: string }> }>;
};

type EvidenceBundle = {
  artifacts: Array<{ id: string; redacted: boolean; url: string }>;
  outputs: Record<string, string>;
};

test.describe("@recording @replay active recording flows", () => {
  test.use({ testPermissions: "all_ready" });

  test("@recording @replay records, normalises, saves, and replays a browser fixture", async ({
    page,
    api,
    guiUrl,
    expectNoRedErrors,
  }) => {
    const finalised = await recordThroughGui(page, guiUrl, {
      name: "Web invoice lookup",
      targetLabel: "Browser task",
      expectedBackend: "greentic.desktop.playwright",
      inputField: "invoice_id",
      inputValue: "INV-100",
      outputField: "result",
      expectedOutput: "42.50",
    });

    await api.post(`/recordings/${finalised.sessionId}/mark-secret`, {
      value: "password=super-secret-token",
    });
    const events = await api.get<RecordingEvents>(`/recordings/${finalised.sessionId}/events`);
    expect(JSON.stringify(events)).not.toContain("super-secret-token");

    await runSavedRunner(page, finalised.runnerId, "invoice_id", "INV-100", "result", "42.50");
    const evidence = await api.get<EvidenceList>("/evidence");
    const bundle = evidence.bundles.find((entry) => entry.runnerId === finalised.runnerId);
    expect(bundle, "runner evidence bundle should be listed").toBeTruthy();
    const detail = await api.get<EvidenceBundle>(`/evidence/${bundle!.bundleId}`);
    expect(detail.artifacts[0]?.redacted).toBe(true);
    expect(JSON.stringify(detail)).not.toContain("super-secret-token");
    await expectNoRedErrors();
  });

  test("@recording @replay records and replays a terminal fixture", async ({
    page,
    guiUrl,
    expectNoRedErrors,
  }) => {
    const finalised = await recordThroughGui(page, guiUrl, {
      name: "Terminal balance lookup",
      targetLabel: "Terminal/mainframe task",
      expectedBackend: "greentic.desktop.terminal.tn3270",
      inputField: "account_id",
      inputValue: "ACC-100",
      outputField: "balance",
      expectedOutput: "100.00",
    });

    await runSavedRunner(page, finalised.runnerId, "account_id", "ACC-100", "balance", "100.00");
    await expectNoRedErrors();
  });
});

test.describe("@recording blocked capture", () => {
  test.use({ testPermissions: "screen_capture_missing" });

  test("@recording explains blocked desktop capture without pretending success", async ({
    page,
    api,
    guiUrl,
  }) => {
    await page.goto(appUrl(guiUrl, "/create", { mode: "record" }));
    await page.getByRole("textbox").fill("Blocked desktop capture");
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Desktop app task/i }).click();
    await expect(page.getByText("Screen recording permission is required.").last()).toBeVisible();
    const createResponse = page.waitForResponse(
      (response) =>
        response.url().includes("/api/v1/recordings") &&
        response.request().method() === "POST",
    );
    await page.getByRole("button", { name: /^Continue/i }).click();
    const create = await expectData<RecordingSummary>(await createResponse);
    expect(create.captureState).toBe("blocked");
    expect(create.realEvents).toBe(0);
    expect(create.captureBlockedReasons.join(" ")).toContain("Screen recording permission");

    await expect(page.getByText(/Capture backend is not active|No desktop events captured yet/i)).toBeVisible();
    await expect(page.getByText(/Grant Screen Recording|Screen recording permission/i)).toBeVisible();

    await page.getByRole("button", { name: /^Stop Recording/i }).nth(1).click();
    await expect(page.getByText(/No captured app events were found/i)).toBeVisible();

    const normalised = await api.post<RecordingNormalise>(
      `/recordings/${create.sessionId}/normalise`,
      {},
    );
    expect(normalised.warnings.join(" ")).toContain("No captured app events");
  });
});

async function recordThroughGui(
  page: Page,
  guiUrl: string,
  scenario: {
    name: string;
    targetLabel: string;
    expectedBackend: string;
    inputField: string;
    inputValue: string;
    outputField: string;
    expectedOutput: string;
  },
): Promise<RecordingFinalise & { sessionId: string }> {
  await page.goto(appUrl(guiUrl, "/create", { mode: "record" }));
  await page.getByRole("textbox").fill(scenario.name);
  await page.getByRole("button", { name: /Continue/i }).click();
  await page.getByRole("button", { name: scenario.targetLabel }).click();

  const startResponse = page.waitForResponse(
    (response) =>
      response.url().includes("/api/v1/recordings") &&
      response.request().method() === "POST",
  );
  await page.getByRole("button", { name: /^Continue/i }).click();
  const start = await expectData<RecordingSummary>(await startResponse);
  expect(start.state).toBe("recording");
  expect(start.captureState).toBe("active");
  expect(start.captureBackend).toBe(scenario.expectedBackend);
  expect(start.realEvents).toBeGreaterThan(0);
  expect(start.screenshots).toBeGreaterThan(0);

  await expect(page.getByText(new RegExp(`${start.realEvents} captured app events`))).toBeVisible();
  await page.getByRole("button", { name: /^Stop Recording/i }).first().click();

  const normaliseResponse = page.waitForResponse(
    (response) =>
      response.url().includes(`/api/v1/recordings/${start.sessionId}/normalise`) &&
      response.request().method() === "POST",
  );
  await page.getByRole("button", { name: /^Stop Recording/i }).nth(1).click();
  const normalised = await expectData<RecordingNormalise>(await normaliseResponse);
  expect(normalised.warnings).toEqual([]);
  expect(normalised.steps.length).toBeGreaterThanOrEqual(3);
  expect(normalised.inputs).toContain(`inputs.${scenario.inputField}`);
  expect(normalised.outputs).toContain(`outputs.${scenario.outputField}`);
  expect(normalised.yamlPreview).toContain("required_capability");

  await expect(page.getByText(/required_capability/).first()).toBeVisible();
  await page.getByRole("button", { name: /^Continue/i }).click();

  const testResponse = page.waitForResponse(
    (response) =>
      response.url().includes(`/api/v1/recordings/${start.sessionId}/test`) &&
      response.request().method() === "POST",
  );
  await page.getByRole("button", { name: /Run Test/i }).click();
  const testResult = await expectData<RecordingTestResult>(await testResponse);
  expect(testResult.status).toBe("passed");
  expect(testResult.evidenceRef).toContain(`/recordings/${start.sessionId}/test-results/latest`);
  await expect(page.getByText(/Test passed/i)).toBeVisible();

  const finaliseResponse = page.waitForResponse(
    (response) =>
      response.url().includes(`/api/v1/recordings/${start.sessionId}/finalise`) &&
      response.request().method() === "POST",
  );
  await page.getByRole("button", { name: /Save Runner/i }).click();
  const finalised = await expectData<RecordingFinalise>(await finaliseResponse);
  expect(finalised.saved).toBe(true);
  await page.getByRole("link", { name: /View Runner/i }).click();
  await expect(page.getByTestId(`runner-${finalised.runnerId}`)).toBeVisible();
  return { ...finalised, sessionId: start.sessionId };
}

async function runSavedRunner(
  page: Page,
  runnerId: string,
  inputField: string,
  inputValue: string,
  outputField: string,
  expectedOutput: string,
) {
  const runner = page.getByTestId(`runner-${runnerId}`);
  await expect(runner).toBeVisible();
  await runner.getByTestId(`runner-${runnerId}-run`).click();
  await runner.getByTestId(`runner-${runnerId}-input-${inputField}`).fill(inputValue);
  await runner.getByTestId(`runner-${runnerId}-run-confirm`).click();
  await expect(runner.getByText(outputField, { exact: true })).toBeVisible();
  await expect(runner.getByText(expectedOutput)).toBeVisible();
}

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
