# PR-11 Prompt-to-Runner Planner

## Goal

Convert natural language instructions into structured runner drafts.

## Example Prompt

```text
Create a runner that opens the CRM web app, logs in with the service account, creates a customer using company name and email, and returns the customer ID.
```

## Planner Output

- Runner ID
- Inputs
- Outputs
- Risk level
- Required adapters
- Session profile
- Steps
- Assertions
- Evidence policy
- Open questions if required

## Prompt Contract

The planner should produce JSON/YAML only, then the validator checks it.

## Planning Context

The planner receives:

- Available adapters
- Available MCP tools
- Application metadata
- Existing runners
- LTM examples
- Security policies
- User prompt
- Current desktop observations if available

## Safety

The planner may create drafts, but it may not directly run high-risk actions without approval.

## Acceptance Criteria

- Can create a draft runner from a prompt.
- Can infer inputs and outputs.
- Can select likely adapter.
- Can produce valid runner YAML.
- Can flag risk level.
