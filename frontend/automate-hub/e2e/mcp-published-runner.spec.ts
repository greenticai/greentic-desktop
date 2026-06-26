import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures/greentic";

const webCalculatorDraft = JSON.stringify({
  runner_id: "web.mcp_calculator",
  version: "0.1.0-draft",
  summary: "MCP calculator fixture",
  risk_level: "medium",
  required_capabilities: ["web.goto", "web.fill", "web.click", "web.extract_text"],
  inputs: {
    number_1: { type: "string", required: true },
    number_2: { type: "string", required: true },
    operation: { type: "string", required: true },
  },
  outputs: {
    result: { type: "string" },
  },
  steps: [
    { id: "open", action: "goto", required_capability: "web.goto" },
    { id: "left", action: "fill", required_capability: "web.fill" },
    { id: "operation", action: "fill", required_capability: "web.fill" },
    { id: "right", action: "fill", required_capability: "web.fill" },
    { id: "calculate", action: "click", required_capability: "web.click" },
    { id: "read", action: "extract_text", required_capability: "web.extract_text" },
  ],
  assertions: ["result is visible"],
  open_questions: [],
});

type McpStatus = {
  status: string;
  bind: string;
  toolCount: number;
};

type McpTools = {
  tools: Array<{ id: string; name: string; status: string }>;
};

type PlannerDraft = {
  draftId: string;
  runnerId: string;
};

type McpRpcResponse = {
  result?: {
    tools?: Array<{ name: string }>;
    structuredContent?: {
      runnerId: string;
      status: string;
      outputs: Record<string, string>;
      evidenceRef: string;
    };
  };
  error?: {
    code: number;
    message: string;
    data?: { missingInputs?: string[] };
  };
};

test.describe("@mcp published runner lifecycle", () => {
  test.use({ llmMockDraftJson: webCalculatorDraft, testPermissions: "all_ready" });

  test("@mcp publishes, runs through GUI and MCP, handles errors, and deletes both lifecycles", async ({
    page,
    api,
    guiUrl,
    expectNoRedErrors,
  }) => {
    let status = await api.get<McpStatus>("/mcp/status");
    expect(status.bind).toMatch(/^127\.0\.0\.1:/);
    if (status.status !== "running") {
      status = await api.post<McpStatus>("/mcp/start", {});
    }
    expect(status.status).toBe("running");
    const restarted = await api.post<McpStatus>("/mcp/restart", {});
    expect(restarted.status).toBe("running");

    await api.put("/settings/llm", {
      provider: "openai_compatible",
      model: "fixture-mcp-planner",
      endpoint: "http://127.0.0.1:9/v1",
    });

    const runnerId = await createRunner(page, guiUrl);
    await page.goto(appUrl(guiUrl, "/runners"));
    const runner = page.getByTestId(`runner-${runnerId}`);
    await expect(runner).toBeVisible();
    await expect(runner.getByText(/MCP tool runner\.web\.mcp_calculator/i)).toBeVisible();

    const tools = await api.get<McpTools>("/mcp/tools");
    expect(tools.tools.find((tool) => tool.id === runnerId)?.status).toBe("enabled");

    await runner.getByTestId(`runner-${runnerId}-run`).click();
    await runner.getByTestId(`runner-${runnerId}-input-number_1`).fill("1");
    await runner.getByTestId(`runner-${runnerId}-input-number_2`).fill("1");
    await runner.getByTestId(`runner-${runnerId}-input-operation`).fill("plus");
    await runner.getByTestId(`runner-${runnerId}-run-confirm`).click();
    await expect(runner.getByText("result", { exact: true })).toBeVisible();
    await expect(runner.getByText("2", { exact: true })).toBeVisible();
    await expect(runner.getByText(/Evidence local:\/\/runners\/web\.mcp_calculator\/run\/latest/)).toBeVisible();

    const listed = await mcpCall(restarted.bind, {
      jsonrpc: "2.0",
      id: 1,
      method: "tools/list",
      params: {},
    });
    expect(listed.result?.tools?.map((tool) => tool.name)).toContain("runner.web.mcp_calculator");

    const called = await mcpCall(restarted.bind, {
      jsonrpc: "2.0",
      id: 2,
      method: "tools/call",
      params: {
        name: "runner.web.mcp_calculator",
        arguments: { number_1: "1", number_2: "1", operation: "plus" },
      },
    });
    expect(called.result?.structuredContent?.runnerId).toBe(runnerId);
    expect(called.result?.structuredContent?.outputs.result).toBe("2");

    const missingInput = await mcpCall(restarted.bind, {
      jsonrpc: "2.0",
      id: 3,
      method: "tools/call",
      params: {
        name: "runner.web.mcp_calculator",
        arguments: { number_1: "1", operation: "plus" },
      },
    });
    expect(missingInput.error?.code).toBe(-32602);
    expect(missingInput.error?.message).toContain("number_2");
    expect(missingInput.error?.data?.missingInputs).toContain("number_2");

    page.once("dialog", async (dialog) => {
      expect(dialog.message()).toContain("web mcp_calculator");
      await dialog.accept();
    });
    await runner.getByRole("button", { name: /Delete/i }).click();
    await expect(page.getByTestId(`runner-${runnerId}`)).toHaveCount(0);
    const afterDelete = await api.get<McpTools>("/mcp/tools");
    expect(afterDelete.tools.find((tool) => tool.id === runnerId)).toBeUndefined();
    await expectNoRedErrors();
  });
});

async function createRunner(page: Page, guiUrl: string) {
  await page.goto(appUrl(guiUrl, "/create", { mode: "prompt" }));
  await page
    .getByTestId("prompt-textarea")
    .fill("Create a web calculator runner with number_1, number_2, operation, and result");
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

async function mcpCall(bind: string, payload: unknown): Promise<McpRpcResponse> {
  const response = await fetch(`http://${bind}/mcp`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
  });
  expect(response.ok).toBe(true);
  return (await response.json()) as McpRpcResponse;
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
