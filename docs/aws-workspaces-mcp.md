# AWS WorkSpaces MCP Forwarding

Greentic Desktop can be used with AWS desktop streaming when a runner needs to operate inside a managed desktop session instead of on the user's local machine.

AWS documents this capability as the WorkSpaces Applications managed MCP server. When agent access is enabled on a stack, an agent connects to the AWS-hosted MCP endpoint and AWS forwards desktop interaction tools into the active streaming session. The AWS endpoint is:

```text
https://agentaccess-mcp.<region>.api.aws/mcp
```

AWS requires SigV4 signing with the service name `agentaccess-mcp`, and every request must include the streaming session URL from `CreateStreamingURL` in the `X-Amzn-AgentAccess-Streaming-Session-Url` header. See the AWS guide: [WorkSpaces Applications MCP server](https://docs.aws.amazon.com/appstream2/latest/developerguide/agent-access-mcp-server.html).

Automate Hub can drive the local parts of this setup: install or verify extensions in **Settings**, create or record the runner in **Create**, test and publish it from **My Runners**, then copy local MCP configuration from **MCP Tools**. Do not claim AWS registration is complete until the AWS-side stack, streaming URL, and SigV4 MCP connection are configured.

## When To Use This Pattern

Use the AWS-forwarded pattern when:

- the target application only runs inside a WorkSpaces Applications image,
- the automation needs a controlled Windows desktop environment,
- you want AWS to host the low-level MCP desktop control endpoint,
- screenshots and activity should be monitored through AWS services,
- a Greentic runner should appear as a callable tool to an MCP-compatible agent.

## End-To-End Flow

1. **Prepare the AWS desktop.** Create or update a WorkSpaces Applications stack with agent access enabled, associate it with a fleet, and enable the computer input and computer vision settings required by AWS.
2. **Install Greentic Desktop in the image.** Install `greentic-desktop`, initialize the runtime, and install the adapter extensions needed by the runner.
3. **Record from a prompt.** Describe the task in plain language, such as "open the CRM, create a customer from company name and email, and return the customer ID." Greentic records or models the demonstrated desktop actions and redacts sensitive values.
4. **Convert the draft into a runner.** The prompt and recording become a `.gtpack` runner with inputs, secrets, desktop steps, assertions, outputs, and evidence policy.
5. **Approve and publish the runner.** Sign the approved runner and publish it to the runner registry for the right tenant, team, stage, and version channel.
6. **Build the forwarded MCP tool.** Convert the signed runner manifest and runner package into a forwarded tool descriptor. In the Greentic model, AWS-forwarded tools receive a forwarded name such as `forwarded___crm_create_customer`.
7. **Start the desktop session.** Generate an AWS streaming URL with `CreateStreamingURL`.
8. **Connect the agent to AWS MCP.** The agent connects to `https://agentaccess-mcp.<region>.api.aws/mcp` using Streamable HTTP, SigV4 signing, and the streaming URL header.
9. **List and call tools.** The agent lists tools through MCP, sees the forwarded desktop controls from AWS and the Greentic runner tools registered for the session, then calls the runner with business inputs.
10. **Review evidence.** The run returns structured outputs and evidence references. AWS activity can also be monitored through CloudTrail, CloudWatch, and S3 screenshot storage when configured.

## Greentic Runtime Setup In The Image

Install Greentic Desktop in the WorkSpaces Applications image or the image build process:

```bash
cargo binstall greentic-desktop
```

Initialize the runtime:

```bash
greentic-desktop init
```

Install the adapter extension needed by the first runner. For a web application:

```bash
greentic-desktop extension install greentic.desktop.playwright
```

Confirm the runtime can see its installed extensions and runner folder:

```bash
greentic-desktop extension list
greentic-desktop runner list
```

The modeled WorkSpaces install plan uses the same runtime pieces: install the runtime into the image, pull approved runners from the registry, install required adapters, start the MCP endpoint, and register available tools.

## Recording From A Prompt

Start with a prompt that is specific enough to create inputs, outputs, and checks:

```text
Create a runner named crm.create_customer.
Open the CRM web app, create a customer from company_name and email,
wait for the confirmation page, extract the customer_id,
and keep screenshot evidence of the confirmation.
```

The prompt-recording flow should identify:

- runner ID: `crm.create_customer`,
- inputs: `company_name`, `email`,
- secrets: CRM login or token references,
- required adapter: for example `greentic.desktop.playwright`,
- assertions: confirmation text or visible customer ID,
- outputs: `customer_id`,
- evidence: screenshots and redacted tool traces.

The installed CLI can plan a draft runner from a prompt with `greentic-desktop runner plan` and can manage recording sessions with `greentic-desktop record ...`. Registry publishing, production approval workflow, AWS forwarder registration, and live AWS API orchestration are still represented by Rust product models and tests while their production commands are being added.

## Convert And Publish The Runner

After review, the runner should be packaged and signed before production use. The signed manifest should capture:

- runner ID and version,
- lifecycle state such as published,
- stage such as staging or prod,
- tenant and team scope,
- required adapters,
- Greentic Desktop compatibility,
- package checksum,
- signature.

Published runners are expected to be signed. Greentic Desktop's policy model allows unsigned drafts during local authoring but rejects unsigned published runners when signature enforcement is enabled.

Place a local runner package in the runtime runner folder for discovery:

```text
~/.greentic/desktop/runners
```

Then confirm discovery:

```bash
greentic-desktop runner list
```

## Configure The MCP Runner Forwarder

The forwarded MCP tool is the tool-facing wrapper around the signed runner. It should include:

- the Greentic runner package,
- the signed runner manifest,
- the session profile for the WorkSpaces session,
- adapter capability metadata,
- input and output schemas,
- risk and approval policy,
- required permissions,
- evidence policy,
- forwarding mode set to AWS forwarded.

In the Greentic model, a runner such as:

```text
crm.create_customer
```

can be exposed locally as:

```text
crm.create_customer
```

and in AWS-forwarded mode as:

```text
forwarded___crm_create_customer
```

That forwarded name is what an upstream MCP-compatible client can use to distinguish a Greentic desktop runner that must execute through the WorkSpaces session.

## Connect Through The AWS WorkSpaces Applications MCP Server

Create a streaming URL for the user or agent session:

```bash
aws appstream create-streaming-url \
  --stack-name your-stack-name \
  --fleet-name your-fleet-name \
  --user-id your-agent-id \
  --validity 3600
```

Pass the returned `StreamingURL` to the MCP client as the AWS-required header:

```text
X-Amzn-AgentAccess-Streaming-Session-Url: <StreamingURL>
```

The MCP client must send SigV4-signed Streamable HTTP requests to:

```text
https://agentaccess-mcp.<region>.api.aws/mcp
```

AWS's example uses `mcp-proxy-for-aws` with:

```python
aws_iam_streamablehttp_client(
    endpoint="https://agentaccess-mcp.<region>.api.aws/mcp",
    aws_service="agentaccess-mcp",
    aws_region="<region>",
    headers={
        "X-Amzn-AgentAccess-Streaming-Session-Url": streaming_url,
    },
)
```

The IAM principal used by the agent needs permission for the AWS managed MCP service and streaming URL creation. AWS's getting-started guide shows `agentaccess-mcp:*`, `appstream:CreateStreamingURL`, and `appstream:DescribeFleets` as the core permissions for the tutorial setup.

## Seeing Forwarded MCP Tools

After the agent connects to the AWS endpoint, it can call `tools/list`. AWS exposes low-level desktop tools with the `agentaccess___` prefix, such as mouse, keyboard, and screenshot tools.

Greentic runner forwarders should be registered alongside the session so the agent also sees higher-level business tools, for example:

```json
{
  "name": "forwarded___crm_create_customer",
  "description": "Create a CRM customer through the WorkSpaces desktop",
  "inputSchema": {
    "required": ["company_name", "email"]
  }
}
```

The agent should call the Greentic runner tool for the business operation instead of manually chaining low-level mouse and keyboard calls:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "forwarded___crm_create_customer",
    "arguments": {
      "company_name": "Example Ltd",
      "email": "buyer@example.com"
    }
  }
}
```

Greentic Desktop handles the runner replay, policy checks, required secrets, approvals, and evidence. AWS handles the managed MCP connection into the streaming desktop session.

## Monitoring And Evidence

Use both Greentic and AWS evidence:

- Greentic returns runner outputs and evidence URIs for business audit.
- AWS CloudTrail can log agent session events and tool calls when data events are configured.
- AWS CloudWatch provides operational metrics for agent sessions.
- AWS S3 screenshot storage can retain screenshots when enabled on the stack.

Treat screenshots and tool traces as sensitive operational evidence. Configure retention, access control, and redaction policies before using this pattern in production.

## Current Scope

The current Greentic CLI can initialize the runtime, install extension manifests, discover `.gtpack` runner packages, and serve a minimal local MCP endpoint:

```bash
greentic-desktop mcp serve
```

The AWS WorkSpaces install plan, approved-runner pull, forwarded MCP tool builder, and forwarded runner call flow are implemented in this repository as models and tests. The production command surface for registry publish, AWS forwarder registration, and live AWS API orchestration is still being added.
