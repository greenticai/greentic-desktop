import { expect, test } from "./fixtures/greentic";
import type { APIResponse, Page } from "@playwright/test";

const calculatorPrompt =
  "open the calculator app. Take three inputs: two numbers and one operation (plus, minus, divide or multiply) and make the calculator do the operation and return the result";

const validRemoteDraft = JSON.stringify({
  runner_id: "web.invoice_lookup",
  version: "0.1.0-draft",
  summary: "Look up invoice total",
  risk_level: "medium",
  required_capabilities: ["web.goto", "web.fill", "web.click", "web.extract_text"],
  inputs: {
    invoice_id: { type: "string", required: true },
  },
  outputs: {
    total: { type: "string" },
  },
  steps: [
    { id: "open", action: "goto", required_capability: "web.goto" },
    { id: "enter", action: "fill", required_capability: "web.fill" },
    { id: "submit", action: "click", required_capability: "web.click" },
    { id: "read", action: "extract_text", required_capability: "web.extract_text" },
  ],
  assertions: ["total is visible"],
  open_questions: [],
});

const unsupportedCapabilityDraft = JSON.stringify({
  runner_id: "sap.unsupported",
  version: "0.1.0-draft",
  summary: "Unsupported SAP click",
  risk_level: "medium",
  required_capabilities: ["sap.click"],
  inputs: {
    account_id: { type: "string", required: true },
  },
  outputs: {
    status: { type: "string" },
  },
  steps: [{ id: "click", action: "click", required_capability: "sap.click" }],
  assertions: [],
  open_questions: [],
});

type PlannerDraft = {
  draftId: string;
  traceId?: string | null;
  runnerId: string;
  requiredAdapters: string[];
  inputs: string[];
  outputs: string[];
  openQuestions: string[];
};

type Runners = {
  runners: Array<{
    id: string;
    inputFields?: string[];
    outputFields?: string[];
  }>;
};

type RunnerDetail = {
  runner: {
    id: string;
    inputFields?: string[];
    outputFields?: string[];
  } | null;
  yamlPreview?: string;
};

type PlannerTrace = {
  traceId: string;
  provider: string;
  model: string;
  structuredOutputMode: string;
  attempts: number;
  requiredAdapters: string[];
};

test.describe("@prompt local heuristic prompt-to-runner", () => {
  test.use({ testPermissions: "all_ready" });

  test("@prompt calculator prompt derives fields, persists wizard edits, saves, and appears in runners", async ({
    page,
    api,
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
    expect(draft.openQuestions).toEqual([]);
    expect(draft.requiredAdapters.length).toBeGreaterThan(0);

    await expect(page.getByRole("heading", { name: /Confirm inputs and outputs/i })).toBeVisible();
    await expect(fieldValues(page, "inputs")).resolves.toEqual([
      "number_1",
      "number_2",
      "operation",
    ]);
    await expect(fieldValues(page, "outputs")).resolves.toEqual(["result"]);

    await addField(page, "inputs", "rounding_mode");
    await addField(page, "outputs", "audit_log");

    await page.getByRole("button", { name: /Continue/i }).click();
    await expect(page.getByRole("heading", { name: /Review draft steps/i })).toBeVisible();
    await page.getByRole("button", { name: /Continue/i }).click();
    await expect(page.getByRole("heading", { name: /Test runner/i })).toBeVisible();

    await clickApiAction(page, `/api/v1/planner/drafts/${draft.draftId}/test`, () =>
      page.getByRole("button", { name: /Run Test/i }).click(),
    );
    await expect(page.getByText("Test passed")).toBeVisible();

    await page.getByRole("button", { name: /Continue/i }).click();
    await expect(page.getByRole("heading", { name: /Save runner/i })).toBeVisible();
    const saveResponse = page.waitForResponse(
      (response) =>
        response.url().includes(`/api/v1/planner/drafts/${draft.draftId}/save`) &&
        response.request().method() === "POST",
    );
    await page.getByRole("button", { name: /Save Runner/i }).click();
    await expectData(await saveResponse);
    await page.getByRole("link", { name: /View Runner/i }).click();
    await expect(page.getByText(draft.runnerId)).toBeVisible();

    const runners = await api.get<Runners>("/runners");
    const saved = runners.runners.find((runner) => runner.id === draft.runnerId);
    expect(saved?.inputFields).toEqual(["number_1", "number_2", "operation", "rounding_mode"]);
    expect(saved?.outputFields).toEqual(expect.arrayContaining(["result", "audit_log"]));
    expect(saved?.outputFields).toHaveLength(2);

    const detail = await api.get<RunnerDetail>(`/runners/${draft.runnerId}`);
    expect(detail.runner?.inputFields).toEqual(saved?.inputFields);
    expect(detail.runner?.outputFields).toEqual(saved?.outputFields);
    expect(detail.yamlPreview).toContain("  - rounding_mode");
    expect(detail.yamlPreview).toContain("  - audit_log");
    await expectNoRedErrors();
  });

  test("@prompt vague login prompt shows open questions instead of silently saving a broken runner", async ({
    page,
    guiUrl,
  }) => {
    await page.goto(appUrl(guiUrl, "/create", { mode: "prompt" }));
    await page.getByTestId("prompt-textarea").fill("automate login");
    await page.getByRole("button", { name: /Generate Draft Runner/i }).click();

    await expect(page.getByTestId("open-questions")).toBeVisible();
    await expect(page.getByTestId("open-questions")).toContainText(/credentials|login/i);
    await expect(page.getByRole("heading", { name: /Confirm inputs and outputs/i })).toBeVisible();
  });
});

test.describe("@llm-mock configured remote LLM prompt planning", () => {
  test.use({
    llmMockDraftJsonSequence: `not json\n---GREENTIC-LLM-RESPONSE---\n${validRemoteDraft}`,
    testPermissions: "all_ready",
  });

  test("@llm-mock configured provider repairs invalid JSON and exposes a trace", async ({
    page,
    api,
    guiUrl,
  }) => {
    await api.put("/settings/llm", {
      provider: "openai_compatible",
      model: "fake-openai-compatible",
      endpoint: "http://127.0.0.1:9/v1",
    });

    await page.goto(appUrl(guiUrl, "/create", { mode: "prompt" }));
    await page
      .getByTestId("prompt-textarea")
      .fill("Open the local invoice web app, enter invoice_id, submit, and return total");
    const draftResponse = page.waitForResponse(
      (response) =>
        response.url().includes("/api/v1/planner/drafts") &&
        response.request().method() === "POST",
    );
    await page.getByRole("button", { name: /Generate Draft Runner/i }).click();
    const draft = await expectData<PlannerDraft>(await draftResponse);

    expect(draft.runnerId).toBe("web.invoice_lookup");
    expect(draft.inputs).toEqual(["invoice_id"]);
    expect(draft.outputs).toEqual(["total"]);
    expect(draft.traceId).toBeTruthy();

    const trace = await api.get<PlannerTrace>(`/planner/traces/${draft.traceId}`);
    expect(trace.provider).toBe("openai_compatible");
    expect(trace.model).toBe("fake-openai-compatible");
    expect(trace.structuredOutputMode).toBe("provider_json_or_repair");
    expect(trace.attempts).toBe(2);
    expect(trace.requiredAdapters).toContain("greentic.desktop.playwright");
  });
});

test.describe("@llm-mock configured remote LLM policy failure", () => {
  test.use({
    llmMockDraftJson: unsupportedCapabilityDraft,
    testPermissions: "all_ready",
  });

  test("@llm-mock unsupported capability surfaces an actionable error and saves no runner", async ({
    page,
    api,
    guiUrl,
  }) => {
    await api.put("/settings/llm", {
      provider: "openai_compatible",
      model: "fake-openai-compatible",
      endpoint: "http://127.0.0.1:9/v1",
    });

    await page.goto(appUrl(guiUrl, "/create", { mode: "prompt" }));
    await page.getByTestId("prompt-textarea").fill("Use SAP GUI to click the custom status field");
    const responsePromise = page.waitForResponse(
      (response) =>
        response.url().includes("/api/v1/planner/drafts") &&
        response.request().method() === "POST",
    );
    await page.getByRole("button", { name: /Generate Draft Runner/i }).click();
    const response = await responsePromise;
    expect(response.status()).toBe(400);
    const payload = (await response.json()) as { ok: boolean; error: { code: string } };
    expect(payload.ok).toBe(false);
    expect(payload.error.code).toBe("planner.unsupported_capability");

    await expect(page.getByRole("main").getByText(/sap\.click|capability/i)).toBeVisible();
    expect((await api.get<Runners>("/runners")).runners).toHaveLength(0);
  });
});

async function addField(page: Page, list: "inputs" | "outputs", value: string) {
  const listId = `field-list-${list}`;
  const before = await page.getByTestId(listId).locator("input").count();
  await page.getByTestId(`${listId}-add`).click();
  await page.getByTestId(`${listId}-${before}`).fill(value);
  await clickApiAction(page, "/api/v1/planner/drafts/", () =>
    page.getByTestId(`${listId}-save`).click(),
    "PATCH",
  );
  await expect(page.getByTestId(`${listId}-${before}`)).toHaveValue(value);
}

async function fieldValues(page: Page, list: "inputs" | "outputs") {
  const values = await page.getByTestId(`field-list-${list}`).locator("input").evaluateAll(
    (inputs) => inputs.map((input) => (input as HTMLInputElement).value),
  );
  return values.filter(Boolean);
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
    (response) => response.url().includes(path) && response.request().method() === method,
  );
  await action();
  await expectData(await responsePromise);
}

async function expectData<T = unknown>(response: APIResponse): Promise<T> {
  expect(response.ok()).toBe(true);
  const payload = (await response.json()) as { ok: boolean; data: T; error?: { message: string } };
  expect(payload.ok, payload.error?.message).toBe(true);
  return payload.data;
}
