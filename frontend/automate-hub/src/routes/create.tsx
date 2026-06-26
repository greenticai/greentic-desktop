import { createFileRoute, Link } from "@tanstack/react-router";
import { useMutation } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
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
} from "lucide-react";

type Mode = null | "prompt" | "record";
type CreateMode = Exclude<Mode, null>;

export const Route = createFileRoute("/create")({
  validateSearch: (search: Record<string, unknown>) => ({
    mode:
      search.mode === "prompt" || search.mode === "record"
        ? (search.mode as CreateMode)
        : undefined,
  }),
  head: () => ({ meta: [{ title: "Create Runner · Greentic Desktop" }] }),
  component: CreatePage,
});

function CreatePage() {
  const search = Route.useSearch();
  const [mode, setMode] = useState<Mode>(() => search.mode ?? null);
  return (
    <div className="p-8 md:p-12 max-w-5xl mx-auto">
      {mode === null && <ChooseMode onPick={setMode} />}
      {mode === "prompt" && <PromptWizard onBack={() => setMode(null)} />}
      {mode === "record" && <RecordWizard onBack={() => setMode(null)} />}
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
      <div className="grid md:grid-cols-2 gap-5">
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
      </div>
    </>
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
    mutationFn: () => api.testPlannerDraft(draft!.draftId, {}),
    onSuccess: (result) => {
      setTestResult(result);
      setMessage(null);
    },
    onError: (error) => setMessage(error instanceof Error ? error.message : "Test failed"),
  });
  const saveDraft = useMutation({
    mutationFn: () => api.savePlannerDraft(draft!.draftId),
    onSuccess: (result) => setMessage(`Saved ${result.runnerId}`),
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
              asChild={saveDraft.isSuccess}
              className="gap-2"
              disabled={!draft || saveDraft.isPending}
              onClick={() => !saveDraft.isSuccess && saveDraft.mutate()}
            >
              {saveDraft.isSuccess ? (
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
      {step === 0 && <PromptStep prompt={prompt} onPromptChange={setPrompt} />}
      {step === 1 && <IOStep draft={draft} />}
      {step === 2 && <StepsStep draft={draft} />}
      {step === 3 && (
        <TestStep
          draft={draft}
          result={testResult}
          busy={testDraft.isPending}
          onRun={() => testDraft.mutate()}
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
        placeholder="Open the CRM, create a new customer using company name and email, save it, and return the customer ID."
        className="text-base"
      />
      <div>
        <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-2">
          Examples
        </div>
        <div className="flex flex-wrap gap-2">
          {[
            "Download today's invoices from the supplier portal.",
            "Create a customer in the CRM.",
            "Look up a customer in the mainframe.",
            "Update a spreadsheet from a desktop report.",
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

function FieldList({ title, items }: { title: string; items: string[] }) {
  const [list, setList] = useState(items);
  return (
    <div className="rounded-xl border p-4">
      <div className="font-medium text-sm mb-3">{title}</div>
      <ul className="space-y-2">
        {list.map((f, i) => (
          <li key={i} className="flex items-center gap-2">
            <Input defaultValue={f} className="h-9" />
            <button
              onClick={() => setList(list.filter((_, j) => j !== i))}
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
        onClick={() => setList([...list, ""])}
      >
        <Plus className="h-3.5 w-3.5" /> Add field
      </Button>
    </div>
  );
}

function IOStep({ draft }: { draft: PlannerDraftDto | null }) {
  if (!draft) {
    return <div className="text-sm text-muted-foreground">Generate a draft first.</div>;
  }
  return (
    <div className="grid md:grid-cols-2 gap-4">
      <FieldList title="Inputs" items={draft.inputs} />
      <FieldList title="Outputs" items={draft.outputs} />
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
  onRun?: () => void;
}) {
  return (
    <div className="space-y-5">
      <div className="grid md:grid-cols-2 gap-3">
        <div>
          <Label>Company name</Label>
          <Input defaultValue="Acme Corp" className="mt-1.5" />
        </div>
        <div>
          <Label>Email address</Label>
          <Input defaultValue="hello@acme.com" className="mt-1.5" />
        </div>
      </div>
      <div className="flex gap-2">
        <Button onClick={onRun ?? (() => undefined)} className="gap-2" disabled={busy}>
          <Play className="h-4 w-4" /> Run Test
        </Button>
      </div>
      {result?.status === "passed" && (
        <div className="rounded-xl border border-success/30 bg-success/10 p-4 flex items-start gap-3">
          <CheckCircle2 className="h-5 w-5 text-success mt-0.5" />
          <div>
            <div className="font-medium text-sm">Test passed</div>
            <div className="text-xs text-muted-foreground mt-1">Evidence: {result.evidenceRef}</div>
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
  const [name, setName] = useState("Create customer in CRM");
  const [target, setTarget] = useState("browser");
  const [session, setSession] = useState<RecordingSummaryDto | null>(null);
  const [normalised, setNormalised] = useState<RecordingNormaliseResultDto | null>(null);
  const [recordingTest, setRecordingTest] = useState<RecordingTestResultDto | null>(null);
  const [finalised, setFinalised] = useState<RecordingFinaliseResultDto | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const start = useMutation({
    mutationFn: () => api.startRecording(name, target),
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
    mutationFn: () => api.testRecording(session!.sessionId),
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
  const prev = () => (step === 0 ? onBack() : setStep((s) => s - 1));

  const titles = [
    { t: "Name your runner", s: "Give it something memorable." },
    { t: "Choose what to record", s: "Where will the task happen?" },
    { t: "Recording", s: "Perform the task once. Greentic captures every step." },
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
              disabled={start.isPending || normalise.isPending}
            >
              {step === 2 ? "Stop Recording" : "Continue"} <ArrowRight className="h-4 w-4" />
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
        <div className="grid sm:grid-cols-2 gap-3">
          {[
            ["browser", "Browser task"],
            ["desktop", "Desktop app task"],
            ["remote", "Remote desktop task"],
            ["terminal", "Terminal/mainframe task"],
          ].map(([id, label]) => (
            <button
              key={id}
              onClick={() => setTarget(id)}
              className={`rounded-xl border p-4 text-left hover:border-primary/40 hover:bg-accent/40 ${
                target === id ? "border-primary bg-accent/50" : ""
              }`}
            >
              <div className="font-medium text-sm">{label}</div>
            </button>
          ))}
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
          testResult={recordingTest}
          finalised={finalised}
          onTest={() => test.mutate()}
          testing={test.isPending}
        />
      )}
    </WizardShell>
  );
}

function RecordingScreen({
  session,
  onAction,
}: {
  session: RecordingSummaryDto | null;
  onAction: (action: string, value?: string) => void;
}) {
  return (
    <div className="space-y-5">
      <div className="rounded-2xl border-2 border-destructive/30 bg-destructive/5 p-8 text-center">
        <div className="inline-flex items-center gap-2 text-destructive font-medium">
          <span className="relative flex h-3 w-3">
            <span className="absolute inline-flex h-full w-full rounded-full bg-destructive/40 animate-ping" />
            <span className="relative inline-flex h-3 w-3 rounded-full bg-destructive" />
          </span>
          Recording
        </div>
        <div className="text-4xl font-semibold tabular-nums mt-4">00:42</div>
        <div className="text-sm text-muted-foreground mt-2">
          {session?.sessionId ?? "Starting"} - {session?.state ?? "starting"}
        </div>
        <div className="mt-6 flex justify-center gap-2">
          <Button
            variant="outline"
            onClick={() => onAction(session?.state === "paused" ? "resume" : "pause")}
          >
            {session?.state === "paused" ? "Resume" : "Pause"}
          </Button>
          <Button variant="destructive" onClick={() => onAction("stop")}>
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
          onClick={() => onAction("mark-input", "input")}
        >
          <Circle className="h-3 w-3" /> Mark as input
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="gap-1.5"
          onClick={() => onAction("mark-output", "output")}
        >
          <Circle className="h-3 w-3" /> Mark as output
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="gap-1.5"
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
  testResult,
  finalised,
  onTest,
  testing,
}: {
  testResult: RecordingTestResultDto | null;
  finalised: RecordingFinaliseResultDto | null;
  onTest: () => void;
  testing: boolean;
}) {
  return (
    <div className="space-y-4">
      <Button onClick={onTest} disabled={testing} className="gap-2">
        <Play className="h-4 w-4" /> Run Test
      </Button>
      {testResult && (
        <div className="rounded-xl border border-success/30 bg-success/10 p-4 text-sm">
          Test {testResult.status}. Evidence: {testResult.evidenceRef}
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
