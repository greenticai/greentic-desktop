import { createServer, type Server } from "node:http";
import { expect, test } from "./fixtures/greentic";
import type { AddressInfo } from "node:net";

const webCalculatorDraft = JSON.stringify({
  runner_id: "web.fixture_calculator",
  version: "0.1.0-draft",
  summary: "Calculate a result in the local web calculator",
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

type PlannerDraft = {
  draftId: string;
  runnerId: string;
  requiredAdapters: string[];
  inputs: string[];
  outputs: string[];
};

type McpTools = {
  tools: Array<{ id: string; status: string; name: string }>;
};

test.describe("@web local web app automation", () => {
  test.use({ llmMockDraftJson: webCalculatorDraft, testPermissions: "all_ready" });

  test("@web creates a web runner, runs calculator inputs, and exposes the MCP tool", async ({
    page,
    api,
    guiUrl,
    expectNoRedErrors,
  }) => {
    const fixtures = await startWebFixtures();
    try {
      const app = await page.context().newPage();
      await app.goto(`${fixtures.url}/calculator`);
      await app.getByLabel("Number 1").fill("1");
      await app.getByLabel("Operation").selectOption("plus");
      await app.getByLabel("Number 2").fill("1");
      await app.getByRole("button", { name: "Calculate" }).click();
      await expect(app.getByTestId("result")).toHaveText("2");
      await app.close();

      await api.put("/settings/llm", {
        provider: "openai_compatible",
        model: "fixture-web-planner",
        endpoint: "http://127.0.0.1:9/v1",
      });

      await page.goto(appUrl(guiUrl, "/create", { mode: "prompt" }));
      await page
        .getByTestId("prompt-textarea")
        .fill(
          `Open ${fixtures.url}/calculator, enter number_1, operation, number_2, calculate, and return result`,
        );
      const draftResponse = page.waitForResponse(
        (response) =>
          response.url().includes("/api/v1/planner/drafts") &&
          response.request().method() === "POST",
      );
      await page.getByRole("button", { name: /Generate Draft Runner/i }).click();
      const draft = await expectData<PlannerDraft>(await draftResponse);
      expect(draft.runnerId).toBe("web.fixture_calculator");
      expect(draft.requiredAdapters).toContain("greentic.desktop.playwright");
      expect(draft.inputs).toEqual(["number_1", "number_2", "operation"]);
      expect(draft.outputs).toEqual(["result"]);

      await page.getByRole("button", { name: /Continue/i }).click();
      await page.getByRole("button", { name: /Continue/i }).click();
      await page.getByRole("button", { name: /Continue/i }).click();
      await expect(page.getByRole("heading", { name: /Save runner/i })).toBeVisible();
      await page.getByRole("button", { name: /Save Runner/i }).click();
      await page.getByRole("link", { name: /View Runner/i }).click();

      const runner = page.getByTestId("runner-web.fixture_calculator");
      await expect(runner).toBeVisible();
      await runner.getByTestId("runner-web.fixture_calculator-run").click();
      await runner.getByTestId("runner-web.fixture_calculator-input-number_1").fill("1");
      await runner.getByTestId("runner-web.fixture_calculator-input-number_2").fill("1");
      await runner.getByTestId("runner-web.fixture_calculator-input-operation").fill("plus");
      await runner.getByTestId("runner-web.fixture_calculator-run-confirm").click();
      await expect(runner.getByText("result")).toBeVisible();
      await expect(runner.getByText("2")).toBeVisible();

      await api.post("/runners/web.fixture_calculator/publish", {});
      const tools = await api.get<McpTools>("/mcp/tools");
      expect(tools.tools.find((tool) => tool.id === "web.fixture_calculator")?.status).toBe(
        "enabled",
      );
      const mcpTest = await api.raw.post("/api/v1/mcp/tools/web.fixture_calculator/test");
      expect(mcpTest.ok()).toBe(true);
      const mcpPayload = (await mcpTest.json()) as {
        ok: boolean;
        data: { status: string; outputs: Record<string, string> };
      };
      expect(mcpPayload.ok).toBe(true);
      expect(mcpPayload.data.status).toBe("passed");
      expect(mcpPayload.data.outputs.result).toBe("sample-output");
      await expectNoRedErrors();
    } finally {
      await fixtures.close();
    }
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

async function startWebFixtures(): Promise<{ url: string; close: () => Promise<void> }> {
  const server = createServer((request, response) => {
    const path = new URL(request.url ?? "/", "http://fixture.local").pathname;
    response.setHeader("content-type", "text/html; charset=utf-8");
    if (path === "/calculator") {
      response.end(calculatorHtml());
      return;
    }
    if (path === "/invoice") {
      response.end(invoiceHtml());
      return;
    }
    if (path === "/login") {
      response.end(loginHtml());
      return;
    }
    response.statusCode = 404;
    response.end("not found");
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = (server.address() as AddressInfo).port;
  return {
    url: `http://127.0.0.1:${port}`,
    close: () => closeServer(server),
  };
}

function closeServer(server: Server): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
}

function calculatorHtml() {
  return `<!doctype html>
    <html>
      <body>
        <main>
          <label>Number 1 <input name="number_1" aria-label="Number 1" data-testid="number_1"></label>
          <label>Operation
            <select name="operation" aria-label="Operation" data-testid="operation">
              <option value="plus">plus</option>
              <option value="minus">minus</option>
              <option value="multiply">multiply</option>
              <option value="divide">divide</option>
            </select>
          </label>
          <label>Number 2 <input name="number_2" aria-label="Number 2" data-testid="number_2"></label>
          <button type="button" onclick="calculate()">Calculate</button>
          <output data-testid="result" aria-live="polite"></output>
        </main>
        <script>
          function calculate() {
            const a = Number(document.querySelector('[name=number_1]').value);
            const b = Number(document.querySelector('[name=number_2]').value);
            const op = document.querySelector('[name=operation]').value;
            const value = op === 'plus' ? a + b : op === 'minus' ? a - b : op === 'multiply' ? a * b : a / b;
            document.querySelector('[data-testid=result]').textContent = String(value);
          }
        </script>
      </body>
    </html>`;
}

function invoiceHtml() {
  return `<!doctype html>
    <html><body>
      <label>Invoice ID <input aria-label="Invoice ID" data-testid="invoice_id"></label>
      <button type="button" onclick="document.querySelector('[data-testid=total]').textContent = document.querySelector('[data-testid=invoice_id]').value === 'INV-100' ? '42.50' : '0.00'">Lookup</button>
      <output data-testid="total"></output>
    </body></html>`;
}

function loginHtml() {
  return `<!doctype html>
    <html><body>
      <label>Username <input aria-label="Username" data-testid="username"></label>
      <label>Password <input aria-label="Password" type="password" data-testid="password"></label>
      <button type="button" onclick="document.querySelector('[data-testid=session_id]').textContent = 'session-redacted'">Login</button>
      <output data-testid="session_id"></output>
    </body></html>`;
}
