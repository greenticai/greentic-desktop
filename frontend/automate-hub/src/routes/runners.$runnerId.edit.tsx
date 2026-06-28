import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  FileCog,
  Play,
  Save,
  Sparkles,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { api } from "@/lib/api";
import type { PlannerTestResultDto, RunnerEditDraftDto, RunnerEditModelDto } from "@/lib/types";

export const Route = createFileRoute("/runners/$runnerId/edit")({
  head: () => ({ meta: [{ title: "Edit Runner · Greentic Desktop" }] }),
  component: RunnerEditPage,
});

function RunnerEditPage() {
  const { runnerId } = Route.useParams();
  const navigate = useNavigate();
  const [step, setStep] = useState(0);
  const [instruction, setInstruction] = useState("");
  const [draft, setDraft] = useState<RunnerEditDraftDto | null>(null);
  const [testResult, setTestResult] = useState<PlannerTestResultDto | null>(null);
  const detail = useQuery({
    queryKey: ["runner", runnerId],
    queryFn: () => api.runner(runnerId),
  });
  const createDraft = useMutation({
    mutationFn: async () => {
      const created = await api.createRunnerEditDraft(runnerId, instruction, "extend");
      return api.planRunnerEditDraft(runnerId, created.draftId, instruction);
    },
    onSuccess: (result) => {
      setDraft(result);
      setTestResult(null);
      setStep(result.openQuestions.length > 0 ? 1 : 2);
    },
  });
  const testDraft = useMutation({
    mutationFn: (sampleInputs: Record<string, string>) =>
      api.testRunnerEditDraft(runnerId, draft!.draftId, sampleInputs),
    onSuccess: setTestResult,
  });
  const applyDraft = useMutation({
    mutationFn: () => api.applyRunnerEditDraft(runnerId, draft!.draftId),
    onSuccess: () => {
      void navigate({ to: "/runners" });
    },
  });
  const runner = detail.data?.runner;
  const source = draft?.sourceRunner;
  const proposed = draft?.proposedRunner;
  const canApply = Boolean(
    draft && testResult?.status === "passed" && draft.openQuestions.length === 0,
  );

  return (
    <div className="p-8 md:p-12 max-w-5xl mx-auto">
      <Button variant="ghost" size="sm" className="mb-5 gap-1.5" asChild>
        <Link to="/runners">
          <ArrowLeft className="h-4 w-4" /> Back
        </Link>
      </Button>

      <div className="mb-8">
        <div className="flex items-center gap-3">
          <div className="h-11 w-11 rounded-xl bg-primary/10 flex items-center justify-center">
            <FileCog className="h-5 w-5 text-primary" />
          </div>
          <div>
            <h1 className="text-3xl font-semibold tracking-tight">
              Edit {runner?.name ?? runnerId}
            </h1>
            <p className="text-muted-foreground mt-1 text-sm">
              Extend the existing runner with a prompt. Greentic keeps the runner context loaded.
            </p>
          </div>
        </div>
      </div>

      <Stepper step={step} />

      {detail.isLoading && (
        <div className="rounded-lg border bg-card px-4 py-3 text-sm text-muted-foreground">
          Loading runner...
        </div>
      )}
      {detail.isError && (
        <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          Could not load runner.
        </div>
      )}

      {runner && step === 0 && (
        <section className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
          <div className="text-sm font-medium">Current runner</div>
          <p className="mt-2 text-sm text-muted-foreground">
            {runner.description ?? "Local runner package managed by Greentic Desktop."}
          </p>
          <RunnerFields
            model={{
              runnerId: runner.id,
              name: runner.name,
              description: runner.description ?? "",
              risk: runner.risk,
              requiredAdapters: runner.adapters ?? [],
              inputs: runner.inputs ?? [],
              outputs: runner.outputs ?? [],
              secrets: [],
              steps: [],
              assertions: [],
              yamlPreview: detail.data?.yamlPreview ?? "",
            }}
          />
          <div className="mt-6">
            <Label>Describe the change</Label>
            <Textarea
              rows={5}
              className="mt-2 text-base"
              value={instruction}
              onChange={(event) => setInstruction(event.target.value)}
              placeholder="Also support subtraction and return the displayed expression."
            />
          </div>
          {createDraft.isError && (
            <div className="mt-3 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {createDraft.error instanceof Error
                ? createDraft.error.message
                : "Could not create edit draft"}
            </div>
          )}
          <div className="mt-5 flex justify-end">
            <Button
              className="gap-2"
              disabled={!instruction.trim() || createDraft.isPending}
              onClick={() => createDraft.mutate()}
            >
              <Sparkles className="h-4 w-4" />
              {createDraft.isPending ? "Generating..." : "Generate Changes"}
            </Button>
          </div>
        </section>
      )}

      {draft && step === 1 && (
        <section className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
          <div className="text-sm font-medium">Open questions</div>
          <p className="mt-2 text-sm text-muted-foreground">
            Answer these before applying the edit. PR-76 will send answers back through apply.
          </p>
          <div className="mt-5 grid gap-3">
            {draft.openQuestions.map((question) => (
              <div key={question}>
                <Label>{question}</Label>
                <Input className="mt-1.5" placeholder="Type your answer" />
              </div>
            ))}
          </div>
          <div className="mt-5 flex justify-between">
            <Button variant="outline" onClick={() => setStep(0)}>
              Back
            </Button>
            <Button onClick={() => setStep(2)} disabled={draft.openQuestions.length > 0}>
              Continue <ArrowRight className="h-4 w-4" />
            </Button>
          </div>
        </section>
      )}

      {draft && source && proposed && step === 2 && (
        <section className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <div className="text-sm font-medium">Review changes</div>
              <div className="mt-1 text-xs text-muted-foreground">
                {draft.draftId} · base {draft.sourceChecksum}
              </div>
            </div>
            <Button
              className="gap-2"
              onClick={() => setStep(3)}
              disabled={draft.openQuestions.length > 0}
            >
              Test changes <ArrowRight className="h-4 w-4" />
            </Button>
          </div>
          <div className="mt-5 grid gap-4 md:grid-cols-2">
            <RunnerFields title="Source" model={source} />
            <RunnerFields title="Proposed" model={proposed} />
          </div>
          <div className="mt-5 grid gap-4 md:grid-cols-2">
            <ListPanel title="Open questions" items={draft.openQuestions} empty="None" />
            <ListPanel title="Change summary" items={draft.changeSummary} empty="No changes" />
          </div>
          {draft.patch && draft.patch.operations.length > 0 && (
            <div className="mt-5 rounded-xl border bg-background p-4">
              <div className="mb-3 text-sm font-medium">Structured patch operations</div>
              <div className="grid gap-2">
                {draft.patch.operations.map((operation, index) => (
                  <div key={index} className="rounded-lg border p-3 text-sm">
                    <div className="font-medium">{operation.operation}</div>
                    <div className="mt-1 text-xs text-muted-foreground">
                      {operation.target} · {operation.safety} risk
                    </div>
                    <div className="mt-2 text-xs">{operation.rationale}</div>
                  </div>
                ))}
              </div>
            </div>
          )}
          <pre className="mt-5 max-h-72 overflow-auto rounded-lg bg-muted p-3 text-xs">
            {draft.yamlPreview}
          </pre>
        </section>
      )}

      {draft && proposed && step === 3 && (
        <section className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
          <div className="text-sm font-medium">Test proposed runner</div>
          <p className="mt-2 text-sm text-muted-foreground">
            These sample values come from the proposed inputs, not a static demo form.
          </p>
          <EditTestForm
            draft={draft}
            result={testResult}
            busy={testDraft.isPending}
            onRun={(sampleInputs) => testDraft.mutate(sampleInputs)}
          />
          <div className="mt-5 flex justify-between">
            <Button variant="outline" onClick={() => setStep(2)}>
              Back
            </Button>
            <Button
              className="gap-2"
              onClick={() => setStep(4)}
              disabled={testResult?.status !== "passed"}
            >
              Continue <ArrowRight className="h-4 w-4" />
            </Button>
          </div>
        </section>
      )}

      {draft && step === 4 && (
        <section className="rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
          <div className="text-sm font-medium">Apply edited runner</div>
          <p className="mt-2 text-sm text-muted-foreground">
            Applying updates this runner in place, records the previous version, and refreshes the
            automatically exposed MCP tool.
          </p>
          {!canApply && (
            <div className="mt-4 rounded-xl border border-warning/40 bg-warning/10 p-4 text-sm">
              Apply is blocked until open questions are resolved and the proposed runner test
              passes.
            </div>
          )}
          {applyDraft.isError && (
            <div className="mt-4 rounded-xl border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
              {applyDraft.error instanceof Error
                ? applyDraft.error.message
                : "Could not apply edited runner"}
            </div>
          )}
          <div className="mt-5 flex justify-between">
            <Button variant="outline" onClick={() => setStep(3)}>
              Back
            </Button>
            <Button
              className="gap-2"
              disabled={!canApply || applyDraft.isPending}
              onClick={() => applyDraft.mutate()}
            >
              <Save className="h-4 w-4" />
              {applyDraft.isPending ? "Applying..." : "Apply Changes"}
            </Button>
          </div>
        </section>
      )}
    </div>
  );
}

function Stepper({ step }: { step: number }) {
  return (
    <div className="mb-6 flex items-center gap-2">
      {["Prompt", "Questions", "Review", "Test", "Apply"].map((label, index) => (
        <div key={label} className="min-w-0 flex-1">
          <div className={`h-1.5 rounded-full ${index <= step ? "bg-primary" : "bg-muted"}`} />
          <div className="mt-1 truncate text-[11px] text-muted-foreground">{label}</div>
        </div>
      ))}
    </div>
  );
}

function EditTestForm({
  draft,
  result,
  busy,
  onRun,
}: {
  draft: RunnerEditDraftDto;
  result: PlannerTestResultDto | null;
  busy: boolean;
  onRun: (sampleInputs: Record<string, string>) => void;
}) {
  const inputNames = useMemo(
    () => draft.proposedRunner.inputs ?? [],
    [draft.proposedRunner.inputs],
  );
  const [sampleInputs, setSampleInputs] = useState<Record<string, string>>({});
  const inputKey = inputNames.join("\n");

  useEffect(() => {
    setSampleInputs((current) => {
      const next: Record<string, string> = {};
      for (const input of inputNames) {
        next[input] = current[input] ?? sampleValueForInput(input);
      }
      return next;
    });
  }, [inputKey, inputNames]);

  return (
    <div className="mt-5 space-y-5">
      <div className="grid gap-3 md:grid-cols-2">
        {inputNames.map((input) => (
          <div key={input}>
            <Label>{fieldLabel(input)}</Label>
            <Input
              className="mt-1.5"
              value={sampleInputs[input] ?? ""}
              onChange={(event) =>
                setSampleInputs((current) => ({ ...current, [input]: event.target.value }))
              }
            />
          </div>
        ))}
      </div>
      <Button className="gap-2" disabled={busy} onClick={() => onRun(sampleInputs)}>
        <Play className="h-4 w-4" /> {busy ? "Running..." : "Run Test"}
      </Button>
      {result?.status === "passed" && (
        <div className="rounded-xl border border-success/30 bg-success/10 p-4 flex items-start gap-3">
          <CheckCircle2 className="h-5 w-5 text-success mt-0.5" />
          <div>
            <div className="font-medium text-sm">Test passed</div>
            <div className="text-xs text-muted-foreground mt-1">Evidence: {result.evidenceRef}</div>
            <div className="mt-3 grid gap-1 text-xs">
              {Object.entries(result.outputs).map(([key, value]) => (
                <div key={key}>
                  {key}: <span className="font-medium text-foreground">{value}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
      {result?.status === "failed" && (
        <div className="rounded-xl border border-warning/40 bg-warning/10 p-4 flex items-start gap-3">
          <AlertTriangle className="h-5 w-5 text-warning mt-0.5" />
          <div className="text-sm">The proposed runner test failed.</div>
        </div>
      )}
    </div>
  );
}

function fieldLabel(field: string) {
  return field
    .replace(/^inputs\./, "")
    .replace(/^outputs\./, "")
    .replace(/_/g, " ");
}

function sampleValueForInput(input: string) {
  const name = input.toLowerCase();
  if (name.includes("number_1") || name.includes("value_1")) {
    return "1";
  }
  if (name.includes("number_2") || name.includes("value_2")) {
    return "1";
  }
  if (name.includes("operation")) {
    return "+";
  }
  if (name.includes("precision")) {
    return "2";
  }
  if (name.includes("discount")) {
    return "10";
  }
  return "";
}

function ListPanel({ title, items, empty }: { title: string; items: string[]; empty: string }) {
  return (
    <div className="rounded-xl border bg-background p-4">
      <div className="mb-2 text-sm font-medium">{title}</div>
      {items.length > 0 ? (
        <ul className="space-y-1 text-sm">
          {items.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      ) : (
        <div className="text-sm text-muted-foreground">{empty}</div>
      )}
    </div>
  );
}

function RunnerFields({ title, model }: { title?: string; model: RunnerEditModelDto }) {
  return (
    <div className="mt-4 rounded-xl border bg-background p-4">
      {title && <div className="mb-3 text-sm font-medium">{title}</div>}
      <div className="grid gap-3 text-sm sm:grid-cols-2">
        <FieldGroup label="Inputs" items={model.inputs} />
        <FieldGroup label="Outputs" items={model.outputs} />
        <FieldGroup label="Secrets" items={model.secrets} />
        <FieldGroup label="Assertions" items={model.assertions} />
      </div>
    </div>
  );
}

function FieldGroup({ label, items }: { label: string; items: string[] }) {
  return (
    <div>
      <div className="text-xs font-medium uppercase text-muted-foreground">{label}</div>
      <div className="mt-1 text-sm">
        {items.length > 0 ? items.join(", ") : <span className="text-muted-foreground">None</span>}
      </div>
    </div>
  );
}
