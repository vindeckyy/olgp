#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
import pathlib

required = {
    "kernel/olgp_drm/olgp_abi.h": [
        "OLGP_IOC_SUBMIT",
        "OLGP_IOC_RECV",
        "OLGP_PCI_VENDOR",
        "struct olgp_submit",
    ],
    "kernel/olgp_drm/olgp_drm.h": [
        "olgp_abi.h",
        "struct olgp_drm",
    ],
    "kernel/olgp_drm/olgp_drv.c": [
        "platform_driver_register",
        "pci_register_driver",
        "drm_dev_register",
        "gem_create_object",
    ],
    "kernel/olgp_drm/olgp_ioctl.c": [
        "olgp_ioctl_submit",
        "olgp_ioctl_recv",
        "wait_event_interruptible",
    ],
    "kernel/olgp_drm/olgp_gem.c": ["drm_gem_object_init"],
    "kernel/olgp_drm/olgp_drm_tool.c": ["olgp_abi.h", "OLGP_IOC_SUBMIT", "OLGP_IOC_RECV"],
    "kernel/olgp_drm/Makefile": ["obj-m", "olgp_drm_tool"],
}
for path, needles in required.items():
    text = pathlib.Path(path).read_text()
    for needle in needles:
        if needle not in text:
            raise SystemExit(f"missing {needle!r} in {path}")
print("kernel driver static checks ok")
PY

make -n -C kernel/olgp_drm tool >/dev/null
