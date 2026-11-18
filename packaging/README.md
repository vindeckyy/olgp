# Packaging manifest

`first-boot.json` is the machine-readable contract for a box installer or
first-boot wizard. It records the fixed ports, configuration paths, pairing
windows, and discovery TXT keys.

Installers should:

1. Run `olgpctl setup --dir /etc/olgp`.
2. Install the driver package with `olgpctl install-driver`.
3. Validate the manifest with `scripts/check-packaging.sh`.
4. Start `olgp-node --config /etc/olgp/node.toml`.

The manifest is not a cloud service contract. It describes only local box
state.
