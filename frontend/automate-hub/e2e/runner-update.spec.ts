import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures/greentic";

const initialDraft = JSON.stringify({
  runner_id: "native.calculator_update",
  version: "0.1.0-draft",
  summary: "Native calculator update fixture",
  risk_level: "medium",
  required_capabilities: ["macos.activate_app", "macos.type_text", "macos.read_text"],
  inputs: {
    number_1: { type: "string", required: true },
    number_2: { type: "string", required: true },
    operation: { type: "string", required: true },
  },
  outputs: {
    result: { type: "string" },
  },
  steps: [
    { id: "open", action: "activate_app", required_capability: "macos.activate_app" },
    { id: "enter", action: "type_text", required_capability: "macos.type_text" },
    { id: "read", action: "read_text", required_capability: "macos.read_text" },
  ],
  assertions: ["result is visible"],
  open_questions: [],
});

const updatedDraft = JSON.stringify({
  runner_id: "native.calculator_update",
  version: "0.1.0-draft",
  summary: "Native calculator update fixture",
  risk_level: "medium",
  required_capabilities: ["macos.activate_app", "macos.type_text", "macos.read_text"],
  inputs: {
    number_1: { type: "string", required: true },
    number_2: { type: "string", required: true },
    operation: { type: "string", required: true },
    rounding_mode: { type: "string", required: false },
  },
  outputs: {
    result: { type: "string" },
    result_text: { type: "string" },
  },
  steps: [
    { id: "open", action: "activate_app", required_capability: "macos.activate_app" },
    { id: "enter", action: "type_text", required_capability: "macos.type_text" },
    { id: "read", action: "read_text", required_capability: "macos.read_text" },
  ],
  assertions: ["result_text is visible"],
  open_questions: [],
});

type PlannerDraft = {
  draftId: string;
  runnerId: string;
  inputs: string[];
  outputs: string[];
};

type RunnerDetail = {
  runner: {
    id: string;
    name: string;
    inputFields: string[];
    outputFields: string[];
  };
  yamlPreview: string;
};

type RunnerAction = {
  runnerId: string;
  status: string;
  outputs: Record<string, string>;
};

type Refinement = {
  refinementId: string;
  runnerId: string;
  status: string;
  applied: boolean;
  diff: { before: string; after: string };
};

test.describe("@runner-update @refinement runner update and refinement", () => {
  test.use({
    llmMockDraftJsonSequence: `${initialDraft}\n---GREENTIC-LLM-RESPONSE---\n${updatedDraft}\n---GREENTIC-LLM-RESPONSE---\n${updatedDraft}`,
    testPermissions: "all_ready",
  });

  test("@runner-update @refinement edits, updates, discards, and refines a saved runner", async ({
    page,
    api,
    guiUrl,
    expectNoRedErrors,
  }) => {
    await api.put("/settings/llm", {
      provider: "openai_compatible",
      model: "fixture-update-planner",
      endpoint: "http://127.0.0.1:9/v1",
    });

    const runnerId = await createInitialRunner(page, guiUrl);
    await page.goto(appUrl(guiUrl, "/runners"));
    const runner = page.getByTestId(`runner-${runnerId}`);
    await expect(runner).toBeVisible();

    await runner.getByRole("button", { name: /Rename native calculator_update/i }).click();
    await runner.getByRole("textbox").fill("Calculator updated by prompt");
    await runner.getByRole("button", { name: "Save" }).click();
    await expect(runner.getByText("Calculator updated by prompt", { exact: true })).toBeVisible();
    await expect(runner.getByText(/MCP tool runner\.native\.calculator_update · Calculator updated by prompt/)).toBeVisible();

    await runner.getByRole("link", { name: /Edit steps/i }).click();
    await expect(page.getByTestId("prompt-textarea")).toHaveValue(/Calculator updated by prompt/);

    await page
      .getByTestId("prompt-textarea")
      .fill("Also support multiply, accept rounding_mode, and return result_text");
    const updateResponse = page.waitForResponse(
      (response) =>
        response.url().includes("/api/v1/planner/drafts") &&
        response.request().method() === "POST",
    );
    await page.getByRole("button", { name: /Generate Draft Runner/i }).click();
    const updateDraft = await expectData<PlannerDraft>(await updateResponse);
    expect(updateDraft.runnerId).toBe(runnerId);
    expect(updateDraft.inputs).toEqual(["number_1", "number_2", "operation"]);
    expect(updateDraft.outputs).toContain("result");

    await addField(page, "inputs", "rounding_mode");
    await addField(page, "outputs", "result_text");
    await expect(fieldValues(page, "inputs")).resolves.toEqual([
      "number_1",
      "number_2",
      "operation",
      "rounding_mode",
    ]);
    await expect(fieldValues(page, "outputs")).resolves.toEqual(["result", "result_text"]);

    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Save Runner/i }).click();
    await page.getByRole("link", { name: /View Runner/i }).click();

    const detail = await api.get<RunnerDetail>(`/runners/${runnerId}`);
    expect(detail.runner.inputFields).toEqual([
      "number_1",
      "number_2",
      "operation",
      "rounding_mode",
    ]);
    expect(detail.runner.outputFields).toEqual(["result", "result_text"]);
    expect(detail.yamlPreview).toContain("rounding_mode");
    expect(detail.yamlPreview).toContain("result_text");

    await page.getByTestId(`runner-${runnerId}`).getByRole("link", { name: /Edit steps/i }).click();
    await page.getByTestId("prompt-textarea").fill("Discard this update and do not save it");
    await page.getByRole("button", { name: /Generate Draft Runner/i }).click();
    await expect(page.getByRole("heading", { name: /Confirm inputs and outputs/i })).toBeVisible();
    await page.goto(appUrl(guiUrl, "/runners"));
    const unchanged = await api.get<RunnerDetail>(`/runners/${runnerId}`);
    expect(unchanged.yamlPreview).toBe(detail.yamlPreview);

    await causeMissingInputFailure(page, api, runnerId);
    const failedRunner = page.getByTestId(`runner-${runnerId}`);
    await expect(failedRunner.getByRole("button", { name: /Fix/i })).toBeVisible();
    await failedRunner.getByRole("button", { name: /Fix/i }).click();
    await failedRunner.getByPlaceholder("Describe the correction").fill("Use the visible result_text output locator");
    await failedRunner.getByRole("button", { name: /Preview fix/i }).click();
    await expect(failedRunner.getByText("click old target")).toBeVisible();
    await expect(failedRunner.getByText("Use the visible result_text output locator")).toBeVisible();
    const applyResponse = page.waitForResponse(
      (response) =>
        response.url().includes(`/api/v1/runners/${runnerId}/refinement/`) &&
        response.url().includes("/apply") &&
        response.request().method() === "POST",
    );
    await failedRunner.getByRole("button", { name: /Apply fix/i }).click();
    const applied = await expectData<Refinement>(await applyResponse);
    expect(applied.applied).toBe(true);

    const refined = await api.get<RunnerDetail>(`/runners/${runnerId}`);
    expect(refined.yamlPreview).toContain("refinement");
    expect(refined.yamlPreview).toContain("result_text output locator");

    await runCalculator(page, runnerId);
    await expectNoRedErrors();
  });
});

async function createInitialRunner(page: Page, guiUrl: string) {
  await page.goto(appUrl(guiUrl, "/create", { mode: "prompt" }));
  await page
    .getByTestId("prompt-textarea")
    .fill("Open calculator, enter two numbers and operation, and return the result");
  const draftResponse = page.waitForResponse(
    (response) =>
      response.url().includes("/api/v1/planner/drafts") &&
      response.request().method() === "POST",
  );
  await page.getByRole("button", { name: /Generate Draft Runner/i }).click();
  const draft = await expectData<PlannerDraft>(await draftResponse);
  await page.getByRole("button", { name: /Continue/i }).click();
  await page.getByRole("button", { name: /Continue/i }).click();
  await page.getByRole("button", { name: /Continue/i }).click();
  await page.getByRole("button", { name: /Save Runner/i }).click();
  await page.getByRole("link", { name: /View Runner/i }).click();
  return draft.runnerId;
}

async function causeMissingInputFailure(
  page: Page,
  api: { get: <T>(path: string) => Promise<T> },
  runnerId: string,
) {
  await page.goto(appUrl(page.url(), "/runners"));
  const runner = page.getByTestId(`runner-${runnerId}`);
  await runner.getByTestId(`runner-${runnerId}-run`).click();
  await runner.getByTestId(`runner-${runnerId}-run-confirm`).click();
  await expect(runner.getByText("failed")).toBeVisible();
  const detail = await api.get<RunnerDetail>(`/runners/${runnerId}`);
  expect(detail.runner.id).toBe(runnerId);
}

async function runCalculator(page: Page, runnerId: string) {
  const runner = page.getByTestId(`runner-${runnerId}`);
  await runner.getByTestId(`runner-${runnerId}-run`).click();
  await runner.getByTestId(`runner-${runnerId}-input-number_1`).fill("2");
  await runner.getByTestId(`runner-${runnerId}-input-number_2`).fill("3");
  await runner.getByTestId(`runner-${runnerId}-input-operation`).fill("multiply");
  await runner.getByTestId(`runner-${runnerId}-input-rounding_mode`).fill("none");
  await runner.getByTestId(`runner-${runnerId}-run-confirm`).click();
  await expect(runner.getByText("result", { exact: true })).toBeVisible();
  await expect(runner.getByText("6", { exact: true })).toBeVisible();
}

async function fieldValues(page: Page, kind: "inputs" | "outputs") {
  return page
    .locator(`[data-testid="field-list-${kind}"] input`)
    .evaluateAll((inputs) => inputs.map((input) => (input as HTMLInputElement).value));
}

async function addField(page: Page, kind: "inputs" | "outputs", value: string) {
  const list = page.getByTestId(`field-list-${kind}`);
  await list.getByTestId(`field-list-${kind}-add`).click();
  const inputs = list.locator("input");
  const last = inputs.nth((await inputs.count()) - 1);
  await last.fill(value);
  await list.getByTestId(`field-list-${kind}-save`).click();
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
