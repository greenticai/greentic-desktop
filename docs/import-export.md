# Runner Import And Export

Greentic Desktop runners can be moved between machines as YAML files. Import and export are available in Automate Hub and through the CLI.

## Import In Automate Hub

Open **Create Runner** and choose **Provide a runner file**.

You can import in two ways:

- **Upload YAML**: choose a local `.yaml` or `.yml` runner file.
- **Import URL**: provide a supported runner source URI.

Supported source schemes:

- `oci://...`
- `store://...`
- `repo://...`
- `file://...`

`oci://`, `store://`, and `repo://` sources are resolved through `greentic-distributor-client`. Greentic Desktop imports only the resolved YAML artifact. Unsupported schemes such as `https://` are rejected by the GUI before import.

If a runner with the same id already exists, the import fails by default. Enable **Replace an existing runner with the same id** only when you intend to overwrite the local runner YAML.

## Export In Automate Hub

Open **My Runners** and select **Export YAML** on a runner.

The downloaded file is the canonical runner YAML for that runner. It includes:

- runner id, name, version, mode, and description.
- declared inputs.
- declared secrets by name only.
- steps, assertions, outputs, and open questions.

The export does not include:

- secret values.
- evidence bundles.
- local run state.
- logs.
- API keys.

## CLI Import

Import a local YAML file:

```bash
greentic-desktop --import ./runners/my-runner.yaml
```

Import from a distributor source:

```bash
greentic-desktop --import repo://team/my-runner.yaml
greentic-desktop --import store://my-runner
greentic-desktop --import oci://registry.example.com/team/my-runner:0.1.0
```

The `runner import` subcommand is equivalent:

```bash
greentic-desktop runner import ./runners/my-runner.yaml
```

## CLI Export

Export an installed runner by id:

```bash
greentic-desktop --export my.runner.id --out ./my.runner.id.yaml
```

Or use the `runner export` subcommand:

```bash
greentic-desktop runner export my.runner.id --out ./my.runner.id.yaml
```

## Validate After Import

After importing, validate on the target machine:

```bash
greentic-desktop desktop validate \
  --workflow my.runner.id \
  --json
```

Add `--input`, `--expect-output`, `--expect-file-changed`, and `--expect-no-modal` for workflows that need live proof.
