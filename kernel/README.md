# OLGP custom DRM driver

This directory contains a small Linux kernel DRM driver for OLGP. It exposes a
`/dev/dri/cardN` node and a bounded submit/receive ioctl transport that a
userspace renderer daemon can use as the local control path for a virtual GPU.

It is licensed `GPL-2.0-only`; the rest of OLGP remains Apache-2.0.

## What it does

- Registers a `DRIVER_GEM` DRM device with shmem-backed GEM objects.
- Provides `OLGP_IOC_SUBMIT` and `OLGP_IOC_RECV` ioctls.
- Has a 64-entry bounded request queue with a wait queue for userspace.
- Binds through either:
  - a `platform_device` named `olgp-drm`, useful for userspace/vendor bring-up;
  - a PCI device with vendor `0x4f4c` and device `0x0001`, when a hypervisor or
    virtual-device layer presents one.

The driver does not emulate a proprietary GPU ABI. CUDA applications continue
to use the userspace CUDA/NVML shim path. This DRM path is for a real
`/dev/dri` device and a custom Vulkan/DRM userspace stack.

A host cannot invent a physical PCI device in software. To see this driver bind
through PCI, the PCI device must be presented by a hypervisor or virtual device
layer. The platform path still exposes `/dev/dri` without that device.

## Build

On a Linux host with kernel headers:

```sh
make -C kernel/olgp_drm
make -C kernel/olgp_drm tool
sudo insmod kernel/olgp_drm/olgp_drm.ko
```

For the platform fallback, register a `platform_device` named `olgp-drm` from
device tree, ACPI, or a small test module. Then:

```sh
./kernel/olgp_drm/olgp_drm_tool /dev/dri/card0
```

Expected output:

```text
request=1 status=1
```

## ABI

Defined in `olgp_drm.h`:

- `OLGP_IOC_SUBMIT` submits one bounded request.
- `OLGP_IOC_RECV` waits for and returns one request to the userspace renderer.
- Payloads are limited to 4096 bytes in this first driver revision.
