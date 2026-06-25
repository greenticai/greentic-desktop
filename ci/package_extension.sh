#!/usr/bin/env bash
set -euo pipefail

extension_id="${1:?usage: ci/package_extension.sh EXTENSION_ID VERSION OUT_DIR}"
version="${2:?usage: ci/package_extension.sh EXTENSION_ID VERSION OUT_DIR}"
out_dir="${3:?usage: ci/package_extension.sh EXTENSION_ID VERSION OUT_DIR}"

artifact_name="${extension_id#greentic.desktop.}"
artifact_name="${artifact_name//./-}"
work_dir="${out_dir}/${artifact_name}"
package="${out_dir}/${artifact_name}.extension.tar.zst"

rm -rf "${work_dir}"
mkdir -p "${work_dir}/bin" "${work_dir}/sidecar" "${work_dir}/assets" "${work_dir}/schemas" "${work_dir}/examples" "${work_dir}/signatures"

cat >"${work_dir}/extension.toml" <<EOF
id = "${extension_id}"
name = "${artifact_name}"
version = "${version}"
publisher = "greenticai"
runtime = "sidecar"
entrypoint = "sidecar/index.js"

[distribution]
source = "oci://ghcr.io/greenticai/greentic-desktop/extensions/${artifact_name}:${version}"

[platforms]
windows = true
macos = true
linux = true

[capabilities]
tools = ["${artifact_name}.run"]

[permissions]
network = true
filesystem = "limited"
screen_capture = false
keyboard_mouse = false
EOF

printf 'manifest:%s:%s\n' "${extension_id}" "${version}" >"${work_dir}/manifest.cbor"
printf 'permissions:%s\n' "${extension_id}" >"${work_dir}/permissions.cbor"
printf 'capabilities:%s\n' "${extension_id}" >"${work_dir}/capabilities.cbor"
printf '{"spdxVersion":"SPDX-2.3","name":"%s","documentNamespace":"https://greentic.local/%s/%s"}\n' "${extension_id}" "${extension_id}" "${version}" >"${work_dir}/SBOM.spdx.json"
printf '# %s\n\nOfficial Greentic Desktop extension package.\n' "${extension_id}" >"${work_dir}/README.md"
printf 'console.log("greentic extension %s");\n' "${extension_id}" >"${work_dir}/sidecar/index.js"
printf '{"type":"module"}\n' >"${work_dir}/sidecar/package.json"
printf 'signature-placeholder\n' >"${work_dir}/signatures/${extension_id}.sig"

tar -C "${work_dir}" -cf "${package}" .

for required in extension.toml manifest.cbor permissions.cbor capabilities.cbor README.md SBOM.spdx.json signatures; do
  tar -tf "${package}" | grep -Eq "(^|\\./)${required}(/|$)"
done

printf '%s\n' "${package}"
