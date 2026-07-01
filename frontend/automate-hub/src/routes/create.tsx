import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { useMutation } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api";
import type { PlannerDraftDto, PlannerTestResultDto } from "@/lib/types";
import type {
  RecordingFinaliseResultDto,
  RecordingNormaliseResultDto,
  RecordingSummaryDto,
  RecordingTestResultDto,
} from "@/lib/types";
import {
  Wand2,
  Video,
  ArrowRight,
  ArrowLeft,
  Sparkles,
  Plus,
  X,
  Play,
  CheckCircle2,
  AlertTriangle,
  Circle,
  Save,
  FileUp,
  LinkIcon,
} from "lucide-react";

export const Route = createFileRoute("/create")({
  head: () => ({ meta: [{ title: "Create Runner · Greentic Desktop" }] }),
  component: CreatePage,
});

type Mode = null | "prompt" | "record" | "file";

function CreatePage() {
  const [mode, setMode] = useState<Mode>(() => {
    if (typeof window === "undefined") {
      return null;
    }
    const requested = new URLSearchParams(window.location.search).get("mode");
    return requested === "prompt" || requested === "record" || requested === "file"
      ? requested
      : null;
  });
  return (
    <div className="p-8 md:p-12 max-w-5xl mx-auto">
      {mode === null && <ChooseMode onPick={setMode} />}
      {mode === "prompt" && <PromptWizard onBack={() => setMode(null)} />}
      {mode === "record" && <RecordWizard onBack={() => setMode(null)} />}
      {mode === "file" && <RunnerFileWizard onBack={() => setMode(null)} />}
    </div>
  );
}

function ChooseMode({ onPick }: { onPick: (m: Mode) => void }) {
  return (
    <>
      <div className="text-center mb-10">
        <h1 className="text-3xl font-semibold tracking-tight">
          How do you want to create your runner?
        </h1>
        <p className="text-muted-foreground mt-3">
          Pick the way that feels easiest. You can always switch later.
        </p>
      </div>
      <div className="grid md:grid-cols-3 gap-5">
        <button
          onClick={() => onPick("prompt")}
          className="text-left rounded-2xl border bg-card p-7 shadow-[var(--shadow-card)] hover:shadow-[var(--shadow-elegant)] hover:border-primary/40 transition-all"
        >
          <div className="h-12 w-12 rounded-xl bg-primary/10 flex items-center justify-center mb-4">
            <Wand2 className="h-6 w-6 text-primary" />
          </div>
          <div className="font-semibold text-lg">Describe the task</div>
          <p className="text-sm text-muted-foreground mt-2 leading-relaxed">
            Type what you want to automate in plain English. Greentic drafts the steps for you.
          </p>
          <div className="mt-5 inline-flex items-center gap-1.5 text-sm font-medium text-primary">
            Generate Draft Runner <ArrowRight className="h-4 w-4" />
          </div>
        </button>
        <button
          onClick={() => onPick("record")}
          className="text-left rounded-2xl border bg-card p-7 shadow-[var(--shadow-card)] hover:shadow-[var(--shadow-elegant)] hover:border-primary/40 transition-all"
        >
          <div className="h-12 w-12 rounded-xl bg-primary/10 flex items-center justify-center mb-4">
            <Video className="h-6 w-6 text-primary" />
          </div>
          <div className="font-semibold text-lg">Record the task</div>
          <p className="text-sm text-muted-foreground mt-2 leading-relaxed">
            Open the app, perform the task once, and Greentic will capture the steps.
          </p>
          <div className="mt-5 inline-flex items-center gap-1.5 text-sm font-medium text-primary">
            Start Recording <ArrowRight className="h-4 w-4" />
          </div>
        </button>
        <button
          onClick={() => onPick("file")}
          className="text-left rounded-2xl border bg-card p-7 shadow-[var(--shadow-card)] hover:shadow-[var(--shadow-elegant)] hover:border-primary/40 transition-all"
        >
          <div className="h-12 w-12 rounded-xl bg-primary/10 flex items-center justify-center mb-4">
            <FileUp className="h-6 w-6 text-primary" />
          </div>
          <div className="font-semibold text-lg">Provide a runner file</div>
          <p className="text-sm text-muted-foreground mt-2 leading-relaxed">
            Upload runner YAML or import it from an oci://, store://, or repo:// source.
          </p>
          <div className="mt-5 inline-flex items-center gap-1.5 text-sm font-medium text-primary">
            Import Runner <ArrowRight className="h-4 w-4" />
          </div>
        </button>
      </div>
    </>
  );
}

function RunnerFileWizard({ onBack }: { onBack: () => void }) {
  const navigate = useNavigate();
  const [tab, setTab] = useState<"upload" | "source">("upload");
  const [source, setSource] = useState("");
  const [replace, setReplace] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const importYaml = useMutation({
    mutationFn: async () => {
      if (!selectedFile) throw new Error("Choose a YAML runner file first.");
      if (!selectedFile.name.match(/\.ya?ml$/i)) {
        throw new Error("Runner files must use .yaml or .yml.");
      }
      const yaml = await selectedFile.text();
      return api.importRunnerYaml(selectedFile.name, yaml, replace);
    },
    onSuccess: (result) => {
      setMessage(`Imported ${result.runnerName}`);
      void navigate({ to: "/runners" });
    },
    onError: (error) => setMessage(error instanceof Error ? error.message : "Import failed"),
  });
  const importSource = useMutation({
    mutationFn: () => {
      const trimmed = source.trim();
      if (!/^(oci|store|repo|file):\/\//.test(trimmed)) {
        throw new Error("Use an oci://, store://, repo://, or file:// runner source.");
      }
      return api.importRunnerSource(trimmed, replace);
    },
    onSuccess: (result) => {
      setMessage(`Imported ${result.runnerName}`);
      void navigate({ to: "/runners" });
    },
    onError: (error) => setMessage(error instanceof Error ? error.message : "Import failed"),
  });
  const busy = importYaml.isPending || importSource.isPending;

  return (
    <WizardShell
      onBack={onBack}
      step={0}
      total={1}
      title="Provide a runner file"
      subtitle="Import an existing runner YAML from local disk or a Greentic distributor source."
      footer={
        <>
          <Button variant="outline" onClick={onBack}>
            Back
          </Button>
          <Button
            onClick={() => (tab === "upload" ? importYaml.mutate() : importSource.mutate())}
            disabled={busy}
            className="gap-2"
          >
            <Save className="h-4 w-4" />
            {busy ? "Importing..." : "Import Runner"}
          </Button>
        </>
      }
    >
      {message && (
        <div
          className={`mb-4 text-sm ${
            message.toLowerCase().includes("imported") ? "text-success" : "text-destructive"
          }`}
        >
          {message}
        </div>
      )}
      <div className="mb-5 inline-flex rounded-lg border bg-muted p-1">
        <button
          type="button"
          onClick={() => setTab("upload")}
          className={`inline-flex items-center gap-2 rounded-md px-3 py-2 text-sm ${
            tab === "upload" ? "bg-background shadow-sm" : "text-muted-foreground"
          }`}
        >
          <FileUp className="h-4 w-4" /> Upload YAML
        </button>
        <button
          type="button"
          onClick={() => setTab("source")}
          className={`inline-flex items-center gap-2 rounded-md px-3 py-2 text-sm ${
            tab === "source" ? "bg-background shadow-sm" : "text-muted-foreground"
          }`}
        >
          <LinkIcon className="h-4 w-4" /> Import URL
        </button>
      </div>

      {tab === "upload" ? (
        <div className="space-y-3">
          <Label htmlFor="runner-yaml">Runner YAML</Label>
          <Input
            id="runner-yaml"
            type="file"
            accept=".yaml,.yml,application/x-yaml,text/yaml,text/plain"
            onChange={(event) => setSelectedFile(event.target.files?.[0] ?? null)}
          />
          {selectedFile && (
            <p className="text-xs text-muted-foreground">
              Selected {selectedFile.name} ({Math.max(1, Math.round(selectedFile.size / 1024))} KB)
            </p>
          )}
        </div>
      ) : (
        <div className="space-y-3">
          <Label htmlFor="runner-source">Runner source</Label>
          <Input
            id="runner-source"
            value={source}
            onChange={(event) => setSource(event.target.value)}
            placeholder="repo://team/runner.yaml"
          />
          <p className="text-xs text-muted-foreground">
            Supported sources: oci://, store://, repo://, and file://.
          </p>
        </div>
      )}

      <label className="mt-5 flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={replace}
          onChange={(event) => setReplace(event.target.checked)}
        />
        Replace an existing runner with the same id
      </label>
    </WizardShell>
  );
}

function Stepper({ step, total }: { step: number; total: number }) {
  return (
    <div className="flex items-center gap-2 mb-6">
      {Array.from({ length: total }).map((_, i) => (
        <div
          key={i}
          className={`h-1.5 flex-1 rounded-full ${i <= step ? "bg-primary" : "bg-muted"}`}
        />
      ))}
    </div>
  );
}

function WizardShell({
  onBack,
  step,
  total,
  title,
  subtitle,
  children,
  footer,
}: {
  onBack: () => void;
  step: number;
  total: number;
  title: string;
  subtitle?: string;
  children: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <div>
      <button
        onClick={onBack}
        className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground mb-4"
      >
        <ArrowLeft className="h-4 w-4" /> Back
      </button>
      <Stepper step={step} total={total} />
      <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
      {subtitle && <p className="text-muted-foreground mt-1.5 text-sm">{subtitle}</p>}
      <div className="mt-6 rounded-2xl border bg-card p-6 shadow-[var(--shadow-card)]">
        {children}
      </div>
      {footer && <div className="mt-5 flex justify-between gap-3">{footer}</div>}
    </div>
  );
}

function PromptWizard({ onBack }: { onBack: () => void }) {
  const navigate = useNavigate();
  const [step, setStep] = useState(0);
  const [prompt, setPrompt] = useState("");
  const [draft, setDraft] = useState<PlannerDraftDto | null>(null);
  const [testResult, setTestResult] = useState<PlannerTestResultDto | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const createDraft = useMutation({
    mutationFn: () => api.createPlannerDraft(prompt),
    onSuccess: (result) => {
      setDraft(result);
      setMessage(null);
      setStep(1);
    },
    onError: (error) => setMessage(error instanceof Error ? error.message : "Draft failed"),
  });
  const testDraft = useMutation({
    mutationFn: (sampleInputs: Record<string, string>) =>
      api.testPlannerDraft(draft!.draftId, sampleInputs),
    onSuccess: (result) => {
      setTestResult(result);
      setMessage(null);
    },
    onError: (error) => setMessage(error instanceof Error ? error.message : "Test failed"),
  });
  const saveDraft = useMutation({
    mutationFn: () => api.savePlannerDraft(draft!.draftId),
    onSuccess: () => {
      setMessage(null);
      void navigate({ to: "/runners" });
    },
    onError: (error) => setMessage(error instanceof Error ? error.message : "Save failed"),
  });
  const next = () => {
    if (step === 0) {
      createDraft.mutate();
      return;
    }
    setStep((s) => Math.min(s + 1, 4));
  };
  const prev = () => (step === 0 ? onBack() : setStep((s) => s - 1));

  const titles = [
    { t: "Describe the task", s: "Tell Greentic what you'd like to automate." },
    {
      t: "Confirm inputs and outputs",
      s: "We've guessed what your runner needs. Edit anything that's off.",
    },
    { t: "Review draft steps", s: "Here's what Greentic will do, in plain English." },
    { t: "Test runner", s: "Try it with sample values before saving." },
    { t: "Save runner", s: "Give it a name and you're done." },
  ];

  return (
    <WizardShell
      onBack={prev}
      step={step}
      total={5}
      title={titles[step].t}
      subtitle={titles[step].s}
      footer={
        <>
          <Button variant="outline" onClick={prev}>
            Back
          </Button>
          {step < 4 ? (
            <Button onClick={next} className="gap-2" disabled={createDraft.isPending}>
              {step === 0 ? "Generate Draft Runner" : "Continue"} <ArrowRight className="h-4 w-4" />
            </Button>
          ) : (
            <Button
              className="gap-2"
              disabled={!draft || saveDraft.isPending}
              onClick={() => saveDraft.mutate()}
            >
              <Save className="h-4 w-4" /> {saveDraft.isPending ? "Saving..." : "Save Runner"}
            </Button>
          )}
        </>
      }
    >
      {message && <div className="mb-4 text-sm text-destructive">{message}</div>}
      {step === 0 && <PromptStep prompt={prompt} onPromptChange={setPrompt} />}
      {step === 1 && <IOStep draft={draft} onDraftChange={setDraft} />}
      {step === 2 && <StepsStep draft={draft} />}
      {step === 3 && (
        <TestStep
          draft={draft}
          result={testResult}
          busy={testDraft.isPending}
          onRun={(sampleInputs) => testDraft.mutate(sampleInputs)}
        />
      )}
      {step === 4 && <SaveStep draft={draft} />}
    </WizardShell>
  );
}

function PromptStep({
  prompt,
  onPromptChange,
}: {
  prompt: string;
  onPromptChange: (prompt: string) => void;
}) {
  return (
    <div className="space-y-4">
      <Textarea
        rows={6}
        value={prompt}
        onChange={(event) => onPromptChange(event.target.value)}
        placeholder="Open a resource table, ask for resource_name, name, and email, append a row, save it, and return saved_status."
        className="text-base"
      />
      <div>
        <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-2">
          Examples
        </div>
        <div className="flex flex-wrap gap-2">
          {[
            "Open a resource table, append a row, save, and return saved_status.",
            "Download today's invoices from a supplier portal.",
            "Look up a resource status in the mainframe.",
            "Open a desktop form, fill provided fields, save, and return confirmation text.",
          ].map((e) => (
            <button
              key={e}
              onClick={() => onPromptChange(e)}
              className="rounded-full border bg-muted/40 hover:bg-muted px-3 py-1.5 text-xs"
            >
              <Sparkles className="h-3 w-3 inline mr-1.5 text-primary" />
              {e}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

function FieldList({
  title,
  items,
  onItemsChange,
}: {
  title: string;
  items: string[];
  onItemsChange: (items: string[]) => void;
}) {
  const [list, setList] = useState(items);
  const itemsKey = items.join("\n");
  useEffect(() => {
    setList(items);
  }, [itemsKey, items]);

  const updateList = (next: string[]) => {
    setList(next);
    onItemsChange(next.map((item) => item.trim()).filter(Boolean));
  };

  return (
    <div className="rounded-xl border p-4">
      <div className="font-medium text-sm mb-3">{title}</div>
      <ul className="space-y-2">
        {list.map((f, i) => (
          <li key={i} className="flex items-center gap-2">
            <Input
              value={f}
              className="h-9"
              onChange={(event) => {
                const next = [...list];
                next[i] = event.target.value;
                updateList(next);
              }}
            />
            <button
              onClick={() => updateList(list.filter((_, j) => j !== i))}
              className="h-9 w-9 rounded-md hover:bg-muted flex items-center justify-center text-muted-foreground"
            >
              <X className="h-4 w-4" />
            </button>
          </li>
        ))}
      </ul>
      <Button
        variant="outline"
        size="sm"
        className="mt-3 gap-1.5"
        onClick={() => updateList([...list, ""])}
      >
        <Plus className="h-3.5 w-3.5" /> Add field
      </Button>
    </div>
  );
}

function IOStep({
  draft,
  onDraftChange,
}: {
  draft: PlannerDraftDto | null;
  onDraftChange: (draft: PlannerDraftDto) => void;
}) {
  if (!draft) {
    return <div className="text-sm text-muted-foreground">Generate a draft first.</div>;
  }
  return (
    <div className="grid md:grid-cols-2 gap-4">
      <FieldList
        title="Inputs"
        items={draft.inputs}
        onItemsChange={(inputs) => onDraftChange({ ...draft, inputs })}
      />
      <FieldList
        title="Outputs"
        items={draft.outputs}
        onItemsChange={(outputs) => onDraftChange({ ...draft, outputs })}
      />
    </div>
  );
}

function StepsStep({ draft }: { draft?: PlannerDraftDto | null }) {
  const [open, setOpen] = useState(false);
  if (!draft) {
    return <div className="text-sm text-muted-foreground">Generate a draft first.</div>;
  }
  return (
    <div>
      <ol className="space-y-2">
        {draft.steps.map((s, i) => (
          <li key={s.id} className="flex items-center gap-3 rounded-lg border p-3 bg-background">
            <span className="h-6 w-6 rounded-full bg-primary/10 text-primary text-xs flex items-center justify-center font-semibold">
              {i + 1}
            </span>
            <span className="text-sm flex-1">{s.summary}</span>
            <button className="text-xs text-muted-foreground hover:text-foreground">Edit</button>
          </li>
        ))}
      </ol>
      <button
        onClick={() => setOpen(!open)}
        className="mt-4 text-xs text-muted-foreground hover:text-foreground"
      >
        {open ? "Hide" : "Advanced:"} View YAML
      </button>
      {open && (
        <pre className="mt-2 text-xs bg-muted rounded-lg p-3 overflow-auto font-mono">
          {draft.yamlPreview}
        </pre>
      )}
    </div>
  );
}

function TestStep({
  draft,
  result,
  busy,
  onRun,
}: {
  draft?: PlannerDraftDto | null;
  result?: PlannerTestResultDto | null;
  busy?: boolean;
  onRun?: (sampleInputs: Record<string, string>) => void;
}) {
  const [sampleInputs, setSampleInputs] = useState<Record<string, string>>({});
  const inputNames = useMemo(() => draft?.inputs ?? [], [draft?.inputs]);
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

  if (!draft) {
    return <div className="text-sm text-muted-foreground">Generate a draft first.</div>;
  }

  return (
    <div className="space-y-5">
      {inputNames.length > 0 ? (
        <div className="grid md:grid-cols-2 gap-3">
          {inputNames.map((input) => (
            <div key={input}>
              <Label>{fieldLabel(input)}</Label>
              <Input
                data-testid={`test-input-${input}`}
                value={sampleInputs[input] ?? ""}
                className="mt-1.5"
                onChange={(event) =>
                  setSampleInputs((current) => ({ ...current, [input]: event.target.value }))
                }
              />
            </div>
          ))}
        </div>
      ) : (
        <div className="rounded-lg border bg-muted/40 p-3 text-sm text-muted-foreground">
          This runner does not declare any inputs.
        </div>
      )}
      <div className="flex gap-2">
        <Button onClick={() => onRun?.(sampleInputs)} className="gap-2" disabled={busy || !draft}>
          <Play className="h-4 w-4" /> Run Test
        </Button>
      </div>
      {result?.status === "passed" && (
        <div className="rounded-xl border border-success/30 bg-success/10 p-4 flex items-start gap-3">
          <CheckCircle2 className="h-5 w-5 text-success mt-0.5" />
          <div>
            <div className="font-medium text-sm">Test passed</div>
            <div className="text-xs text-muted-foreground mt-1">Evidence: {result.evidenceRef}</div>
            {Object.keys(result.outputs).length > 0 && (
              <div className="mt-3 grid gap-1 text-xs">
                {Object.entries(result.outputs).map(([key, value]) => (
                  <div key={key}>
                    {key}: <span className="font-medium text-foreground">{value}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
      {result?.status === "failed" && (
        <div className="rounded-xl border border-warning/40 bg-warning/10 p-4 flex items-start gap-3">
          <AlertTriangle className="h-5 w-5 text-warning mt-0.5" />
          <div>
            <div className="font-medium text-sm">Greentic could not find the Save button.</div>
            <div className="text-xs text-muted-foreground mt-1">
              You can retry, edit the step, or record this part manually.
            </div>
            <div className="mt-3 flex gap-2">
              <Button size="sm" variant="outline">
                Retry
              </Button>
              <Button size="sm" variant="outline">
                Edit step
              </Button>
              <Button size="sm" variant="outline">
                Record this part
              </Button>
            </div>
          </div>
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
  if (name.includes("email")) {
    return "hello@example.com";
  }
  if (name.includes("company")) {
    return "Example Company";
  }
  return "";
}

function SaveStep({ draft }: { draft: PlannerDraftDto | null }) {
  return (
    <div className="space-y-4">
      <div>
        <Label>Runner name</Label>
        <Input value={draft?.name ?? ""} readOnly className="mt-1.5" />
      </div>
      <div>
        <Label>Short description</Label>
        <Textarea rows={3} value={draft?.description ?? ""} readOnly className="mt-1.5" />
      </div>
    </div>
  );
}

function RecordWizard({ onBack }: { onBack: () => void }) {
  const [step, setStep] = useState(0);
  const [name, setName] = useState("Append row to resource table");
  const [target, setTarget] = useState("browser");
  const [initialUrl, setInitialUrl] = useState("about:blank");
  const [session, setSession] = useState<RecordingSummaryDto | null>(null);
  const [normalised, setNormalised] = useState<RecordingNormaliseResultDto | null>(null);
  const [recordingTest, setRecordingTest] = useState<RecordingTestResultDto | null>(null);
  const [finalised, setFinalised] = useState<RecordingFinaliseResultDto | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const start = useMutation({
    mutationFn: () =>
      api.startRecording(name, target, target === "browser" ? initialUrl : undefined),
    onSuccess: (result) => {
      setSession(result);
      setStep(2);
      setMessage(null);
    },
    onError: (error) => setMessage(error instanceof Error ? error.message : "Recording failed"),
  });
  const action = useMutation({
    mutationFn: ({ action, value }: { action: string; value?: string }) =>
      api.recordingAction(session!.sessionId, action, value),
    onSuccess: (result) => setSession(result),
    onError: (error) => setMessage(error instanceof Error ? error.message : "Action failed"),
  });
  const normalise = useMutation({
    mutationFn: () => api.normaliseRecording(session!.sessionId),
    onSuccess: (result) => {
      setNormalised(result);
      setStep(3);
    },
    onError: (error) => setMessage(error instanceof Error ? error.message : "Normalise failed"),
  });
  const test = useMutation({
    mutationFn: (sampleInputs: Record<string, string>) =>
      api.testRecording(session!.sessionId, sampleInputs),
    onSuccess: setRecordingTest,
    onError: (error) => setMessage(error instanceof Error ? error.message : "Test failed"),
  });
  const finalise = useMutation({
    mutationFn: () => api.finaliseRecording(session!.sessionId),
    onSuccess: setFinalised,
    onError: (error) => setMessage(error instanceof Error ? error.message : "Save failed"),
  });
  const next = () => {
    if (step === 1) {
      start.mutate();
      return;
    }
    if (step === 2 && session) {
      normalise.mutate();
      return;
    }
    setStep((s) => Math.min(s + 1, 4));
  };
  const recordingSessionId = session?.sessionId;
  useEffect(() => {
    if (step !== 2 || !recordingSessionId || session?.captureState === "blocked") {
      return;
    }
    let cancelled = false;
    const refresh = () => {
      api
        .recording(recordingSessionId)
        .then((result) => {
          if (!cancelled) {
            setSession(result);
          }
        })
        .catch((error) => {
          if (!cancelled) {
            setMessage(error instanceof Error ? error.message : "Refresh failed");
          }
        });
    };
    const interval = window.setInterval(refresh, 1500);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [step, recordingSessionId, session?.captureState]);
  const prev = () => (step === 0 ? onBack() : setStep((s) => s - 1));
  const canReviewRecording =
    Boolean(session) && session?.captureState !== "blocked" && (session?.rawEvents ?? 0) > 1;

  const titles = [
    { t: "Name your runner", s: "Give it something memorable." },
    { t: "Choose what to record", s: "Where will the task happen?" },
    { t: "Recording", s: "Greentic records only the controlled session for this target." },
    { t: "Review recorded steps", s: "Here's what Greentic saw." },
    { t: "Test and save", s: "Run with sample values, then save." },
  ];

  return (
    <WizardShell
      onBack={prev}
      step={step}
      total={5}
      title={titles[step].t}
      subtitle={titles[step].s}
      footer={
        <>
          <Button variant="outline" onClick={prev}>
            Back
          </Button>
          {step < 4 ? (
            <Button
              onClick={next}
              className="gap-2"
              disabled={
                start.isPending || normalise.isPending || (step === 2 && !canReviewRecording)
              }
            >
              {step === 2 ? "Review Capture" : "Continue"} <ArrowRight className="h-4 w-4" />
            </Button>
          ) : (
            <Button
              asChild={Boolean(finalised)}
              className="gap-2"
              disabled={!session || finalise.isPending}
              onClick={() => !finalised && finalise.mutate()}
            >
              {finalised ? (
                <Link to="/runners">
                  <Save className="h-4 w-4" /> View Runner
                </Link>
              ) : (
                <>
                  <Save className="h-4 w-4" /> Save Runner
                </>
              )}
            </Button>
          )}
        </>
      }
    >
      {message && <div className="mb-4 text-sm text-destructive">{message}</div>}
      {step === 0 && (
        <div>
          <Label>Runner name</Label>
          <Input
            value={name}
            onChange={(event) => setName(event.target.value)}
            className="mt-1.5"
          />
        </div>
      )}
      {step === 1 && (
        <div className="space-y-4">
          <div className="grid sm:grid-cols-2 gap-3">
            {recordingTargets.map(({ id, label, detail, status, statusLabel }) => (
              <button
                key={id}
                onClick={() => setTarget(id)}
                className={`rounded-xl border p-4 text-left hover:border-primary/40 hover:bg-accent/40 ${
                  target === id ? "border-primary bg-accent/50" : ""
                }`}
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium text-sm">{label}</span>
                  <span className="rounded border px-1.5 py-0.5 text-[10px] uppercase tracking-normal text-muted-foreground">
                    {status}
                  </span>
                </div>
                <div className="text-xs text-muted-foreground mt-2">{detail}</div>
                <div className="text-xs text-muted-foreground mt-2">{statusLabel}</div>
              </button>
            ))}
          </div>
          {target === "browser" && (
            <div>
              <Label>Start URL</Label>
              <Input
                value={initialUrl}
                onChange={(event) => setInitialUrl(event.target.value)}
                placeholder="https://example.com"
                className="mt-1.5"
              />
            </div>
          )}
        </div>
      )}
      {step === 2 && (
        <RecordingScreen
          session={session}
          onAction={(name, value) => action.mutate({ action: name, value })}
        />
      )}
      {step === 3 && <RecordingReview normalised={normalised} />}
      {step === 4 && (
        <RecordingSave
          normalised={normalised}
          testResult={recordingTest}
          finalised={finalised}
          onTest={(sampleInputs) => test.mutate(sampleInputs)}
          testing={test.isPending}
        />
      )}
    </WizardShell>
  );
}

const recordingTargets = [
  {
    id: "browser",
    label: "Browser task",
    detail: "Greentic opens a controlled browser window. Existing browser tabs are not recorded.",
    status: "beta",
    statusLabel: "Real web replay and MCP evidence paths have fixture coverage.",
  },
  {
    id: "desktop",
    label: "Desktop app task",
    detail: "Uses native accessibility APIs and OS permissions for the current desktop session.",
    status: "experimental",
    statusLabel: "Requires native OS event capture; fixture E2Es are still required.",
  },
  {
    id: "java",
    label: "Java app task",
    detail: "Uses Java Access Bridge for Swing/AWT component events and trees.",
    status: "experimental",
    statusLabel: "Only for Java apps with a Java accessibility event source.",
  },
  {
    id: "remote",
    label: "Remote desktop task",
    detail:
      "Requires a Greentic-owned remote viewport, screen capture, input control, and calibration.",
    status: "experimental",
    statusLabel: "Requires viewport calibration and proof through structured assertions.",
  },
  {
    id: "terminal",
    label: "Terminal/mainframe task",
    detail: "Greentic opens or connects a controlled PTY, SSH, or TN3270 session.",
    status: "experimental",
    statusLabel: "Requires a Greentic-owned terminal runtime, not arbitrary shell tabs.",
  },
];

function RecordingScreen({
  session,
  onAction,
}: {
  session: RecordingSummaryDto | null;
  onAction: (action: string, value?: string) => void;
}) {
  const captureState = session?.captureState ?? "starting";
  const isBlocked = captureState === "blocked" || session?.state === "blocked";
  const isActive = captureState === "recording";
  const heartbeatSeconds = Number(session?.captureHeartbeatAt ?? 0);
  const heartbeatFresh =
    Number.isFinite(heartbeatSeconds) && Date.now() / 1000 - heartbeatSeconds < 60;
  const panelClass = isBlocked
    ? "rounded-2xl border-2 border-amber-300 bg-amber-50 p-8 text-center"
    : isActive
      ? "rounded-2xl border-2 border-emerald-300 bg-emerald-50 p-8 text-center"
      : "rounded-2xl border-2 border-destructive/30 bg-destructive/5 p-8 text-center";
  const statusClass = isBlocked
    ? "inline-flex items-center gap-2 text-amber-700 font-medium"
    : isActive
      ? "inline-flex items-center gap-2 text-emerald-700 font-medium"
      : "inline-flex items-center gap-2 text-destructive font-medium";
  return (
    <div className="space-y-5">
      <div className={panelClass}>
        <div className={statusClass}>
          <span className="relative flex h-3 w-3">
            {isActive && (
              <span className="absolute inline-flex h-full w-full rounded-full bg-destructive/40 animate-ping" />
            )}
            <span
              className={`relative inline-flex h-3 w-3 rounded-full ${
                isBlocked ? "bg-amber-500" : isActive ? "bg-emerald-500" : "bg-destructive"
              }`}
            />
          </span>
          {isBlocked
            ? "Capture blocked"
            : isActive && heartbeatFresh
              ? "Capture backend active"
              : isActive
                ? "Waiting for backend heartbeat"
                : "Starting capture"}
        </div>
        <div className="text-4xl font-semibold tabular-nums mt-4">{session?.rawEvents ?? 0}</div>
        <div className="text-sm text-muted-foreground mt-2">
          events captured · {session?.screenshots ?? 0} screenshots · {captureState}
        </div>
        <div className="text-xs text-muted-foreground mt-2">
          Backend: {session?.captureBackend ?? "not attached"} · Last event:{" "}
          {session?.lastEventSummary ?? "none"}
        </div>
        {isBlocked && (
          <div className="mt-4 rounded-lg border border-amber-300 bg-background p-3 text-left text-sm text-amber-800">
            {(session?.captureBlockedReasons.length
              ? session.captureBlockedReasons
              : ["No capture backend is active for this target."]
            ).map((reason) => (
              <div key={reason}>{reason}</div>
            ))}
          </div>
        )}
        <div className="mt-6 flex justify-center gap-2">
          <Button
            variant="outline"
            disabled={isBlocked}
            onClick={() => onAction(session?.state === "paused" ? "resume" : "pause")}
          >
            {session?.state === "paused" ? "Resume" : "Pause"}
          </Button>
          <Button variant="destructive" disabled={isBlocked} onClick={() => onAction("stop")}>
            Stop Recording
          </Button>
          <Button variant="ghost" onClick={() => onAction("cancel")}>
            Cancel
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap gap-2 justify-center">
        <Button
          variant="outline"
          size="sm"
          className="gap-1.5"
          disabled={isBlocked}
          onClick={() => onAction("mark-input", "input")}
        >
          <Circle className="h-3 w-3" /> Mark as input
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="gap-1.5"
          disabled={isBlocked}
          onClick={() => onAction("mark-output", "output")}
        >
          <Circle className="h-3 w-3" /> Mark as output
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="gap-1.5"
          disabled={isBlocked}
          onClick={() => onAction("mark-secret", "secret")}
        >
          <Circle className="h-3 w-3" /> Hide sensitive value
        </Button>
      </div>
    </div>
  );
}

function RecordingReview({ normalised }: { normalised: RecordingNormaliseResultDto | null }) {
  if (!normalised) {
    return <div className="text-sm text-muted-foreground">Stop recording to review steps.</div>;
  }
  return (
    <div className="space-y-3">
      {normalised.steps.map((step, index) => (
        <div key={index} className="rounded-lg border p-3 text-sm">
          {step}
        </div>
      ))}
      <pre className="text-xs bg-muted rounded-lg p-3 overflow-auto font-mono">
        {normalised.yamlPreview}
      </pre>
    </div>
  );
}

function RecordingSave({
  normalised,
  testResult,
  finalised,
  onTest,
  testing,
}: {
  normalised: RecordingNormaliseResultDto | null;
  testResult: RecordingTestResultDto | null;
  finalised: RecordingFinaliseResultDto | null;
  onTest: (sampleInputs: Record<string, string>) => void;
  testing: boolean;
}) {
  const inputNames = useMemo(() => normalised?.inputs ?? [], [normalised?.inputs]);
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
    <div className="space-y-5">
      <div>
        <div className="text-sm font-medium">Test inputs</div>
        <p className="mt-1 text-sm text-muted-foreground">
          Enter sample values for this recorded runner before running the test.
        </p>
      </div>
      {inputNames.length > 0 ? (
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
      ) : (
        <div className="rounded-lg border bg-muted/40 p-3 text-sm text-muted-foreground">
          This recorded runner does not declare any inputs.
        </div>
      )}
      <Button onClick={() => onTest(sampleInputs)} disabled={testing} className="gap-2">
        <Play className="h-4 w-4" /> {testing ? "Running..." : "Run Test"}
      </Button>
      {testResult && (
        <div className="rounded-xl border border-success/30 bg-success/10 p-4 text-sm">
          <div className="font-medium">Test {testResult.status}</div>
          <div className="mt-1 text-xs text-muted-foreground">
            Evidence: {testResult.evidenceRef}
          </div>
          {Object.keys(testResult.outputs).length > 0 && (
            <div className="mt-3 grid gap-2">
              {Object.entries(testResult.outputs).map(([key, value]) => (
                <div key={key} className="rounded-lg border bg-background px-3 py-2">
                  <span className="text-muted-foreground">{fieldLabel(key)}:</span>{" "}
                  <span className="font-medium text-foreground">{value}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
      {finalised && (
        <div className="rounded-xl border border-success/30 bg-success/10 p-4 text-sm">
          Saved {finalised.runnerId}
        </div>
      )}
    </div>
  );
}
