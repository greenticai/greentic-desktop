# PR-55 - Higher-Level Runner Schema and Semantic Actions

Goal: introduce a runner schema that is suitable for prompt authoring, recording, review, MCP publication, and replay, rather than exposing only low-level `RunnerStep` values.

Problem
`RunnerStep` is a good execution IR, but it is too low-level for users, planners, and MCP clients. It cannot clearly describe input types, secrets, output extractors, retry behavior, wait conditions, evidence policy, approval policy, or semantic actions like "open app", "fill form field", "submit", "read output", and "wait until visible".

Design
Add a higher-level `RunnerDefinition` in `greentic-desktop-runner-schema`:

- `runner_id`
- `version`
- `summary`
- `intent`
- `risk`
- `target_technologies`
- `inputs`
- `secrets`
- `workflow`
- `outputs`
- `assertions`
- `evidence_policy`
- `approval_policy`
- `compiled_steps`

Add semantic step variants:

- `Open`
- `Attach`
- `Observe`
- `Find`
- `Input`
- `Click`
- `Key`
- `Wait`
- `Extract`
- `Assert`
- `Screenshot`
- `Download`
- `Close`

Keep `RunnerStep` as compiled execution output. The new schema should compile to `RunnerStep` plus output extraction specs.

Input and output schema
Inputs and outputs must carry types:

- `string`
- `number`
- `boolean`
- `date`
- `enum`
- `file`
- `json`

Each input can declare required/default/redaction/validation metadata. Each output can declare required/extractor/type/failure behavior.

Acceptance criteria
The schema can represent web, native app, Java, terminal, and vision workflows.
Existing runner packages can migrate to the new schema with compiled steps preserved.
MCP can derive JSON input and output schemas from the runner definition.
The GUI can render input forms from the schema.
The replay engine can execute compiled steps while using semantic output extractor specs.
