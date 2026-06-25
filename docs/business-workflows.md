# Business Workflows

A business workflow chains one or more runners into a larger process.

For example, onboarding a new customer could:

1. collect request data,
2. validate required fields,
3. call a CRM runner,
4. pass the CRM customer ID into a billing runner,
5. send a confirmation.

## Inputs And Outputs

Workflow steps can bind runner inputs from:

- the original request,
- the output of an earlier step,
- a literal value.

This lets a workflow connect desktop systems that do not have direct API integrations.

## Human Approval

Production workflows can require human approval before a runner submits data. If approval is required and missing, the workflow stops before the desktop action is performed.

## Evidence Across A Workflow

Each runner call can produce an evidence URI. The workflow collects those URIs so the full business process can be reviewed after it completes.

## Failure Handling

If a required input is missing, approval is missing, policy blocks the action, or a runner fails, the workflow returns a failure reason and the evidence collected up to that point.
